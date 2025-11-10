use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount, Mint, Burn};
use crate::state::*;
use crate::error::*;
use crate::trove_management::*;
use crate::account_management::*;
use crate::oracle::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RepayLoanParams {
    pub amount: u64,
    pub collateral_denom: String,
    pub prev_node_id: Option<Pubkey>,
    pub next_node_id: Option<Pubkey>,
}

#[derive(Accounts)]
#[instruction(params: RepayLoanParams)]
pub struct RepayLoan<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    
    #[account(
        mut,
        seeds = [b"user_debt_amount", user.key().as_ref()],
        bump,
        constraint = user_debt_amount.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub user_debt_amount: Account<'info, UserDebtAmount>,

    #[account(
        mut,
        seeds = [b"user_collateral_amount", user.key().as_ref(), params.collateral_denom.as_bytes()],
        bump,
        constraint = user_collateral_amount.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub user_collateral_amount: Account<'info, UserCollateralAmount>,

    #[account(
        mut,
        seeds = [b"liquidity_threshold", user.key().as_ref()],
        bump,
        constraint = liquidity_threshold.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub liquidity_threshold: Account<'info, LiquidityThreshold>,
    
    #[account(mut)]
    pub state: Account<'info, StateAccount>,
    
    #[account(mut)]
    pub user_stablecoin_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = user_collateral_account.mint == collateral_mint.key() @ AerospacerProtocolError::InvalidMint
    )]
    pub user_collateral_account: Account<'info, TokenAccount>,

    pub collateral_mint: Account<'info, Mint>,
    
    #[account(
        init_if_needed,
        payer = user,
        token::mint = collateral_mint,
        token::authority = protocol_collateral_account,
        seeds = [b"protocol_collateral_vault", params.collateral_denom.as_bytes()],
        bump
    )]
    pub protocol_collateral_account: Account<'info, TokenAccount>,

    /// CHECK: Stable coin mint - used for burn (supply change) - validated against state
    #[account(
        mut,
        constraint = stable_coin_mint.key() == state.stable_coin_addr @ AerospacerProtocolError::InvalidMint
    )]
    pub stable_coin_mint: UncheckedAccount<'info>,

    /// CHECK: Per-denom collateral total PDA
    #[account(
        mut,
        seeds = [b"total_collateral_amount", params.collateral_denom.as_bytes()],
        bump
    )]
    pub total_collateral_amount: Account<'info, TotalCollateralAmount>,

    // Oracle context - UncheckedAccount to reduce stack usage
    /// CHECK: Our oracle program - validated against state in handler
    pub oracle_program: UncheckedAccount<'info>,
    
    /// CHECK: Oracle state account - validated against state in handler
    #[account(mut)]
    pub oracle_state: UncheckedAccount<'info>,
    
    /// CHECK: Pyth price account for collateral price feed
    pub pyth_price_account: UncheckedAccount<'info>,
    
    /// CHECK: Clock sysvar - validated in handler if needed
    pub clock: UncheckedAccount<'info>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<RepayLoan>, params: RepayLoanParams) -> Result<()> {
    // Validate oracle accounts
    require!(
        ctx.accounts.oracle_program.key() == ctx.accounts.state.oracle_helper_addr,
        AerospacerProtocolError::Unauthorized
    );
    require!(
        ctx.accounts.oracle_state.key() == ctx.accounts.state.oracle_state_addr,
        AerospacerProtocolError::Unauthorized
    );
    
    // Validate input parameters
    require!(
        params.amount > 0,
        AerospacerProtocolError::InvalidAmount
    );
    
    require!(
        !params.collateral_denom.is_empty(),
        AerospacerProtocolError::InvalidAmount
    );
    
    // Check if user has existing trove
    require!(
        ctx.accounts.user_debt_amount.amount > 0,
        AerospacerProtocolError::TroveDoesNotExist
    );
    
    // Check if user has sufficient stablecoins
    require!(
        params.amount <= ctx.accounts.user_stablecoin_account.amount,
        AerospacerProtocolError::InsufficientCollateral
    );
    
    // Check if repayment amount doesn't exceed debt
    require!(
        params.amount <= ctx.accounts.user_debt_amount.amount,
        AerospacerProtocolError::InvalidAmount
    );
    
    // Create contexts in scoped block to reduce stack usage
    let result = {
        let mut trove_ctx = TroveContext {
            user: ctx.accounts.user.clone(),
            user_debt_amount: ctx.accounts.user_debt_amount.clone(),
            liquidity_threshold: ctx.accounts.liquidity_threshold.clone(),
            state: ctx.accounts.state.clone(),
        };
        
        let mut collateral_ctx = CollateralContext {
            user: ctx.accounts.user.clone(),
            user_collateral_amount: ctx.accounts.user_collateral_amount.clone(),
            user_collateral_account: ctx.accounts.user_collateral_account.clone(),
            protocol_collateral_account: ctx.accounts.protocol_collateral_account.clone(),
            total_collateral_amount: ctx.accounts.total_collateral_amount.clone(),
            token_program: ctx.accounts.token_program.clone(),
        };
        
        let oracle_ctx = OracleContext {
            oracle_program: ctx.accounts.oracle_program.to_account_info(),
            oracle_state: ctx.accounts.oracle_state.to_account_info(),
            pyth_price_account: ctx.accounts.pyth_price_account.to_account_info(),
            clock: ctx.accounts.clock.to_account_info(),
        };
        
        // Use TroveManager for clean implementation
        let result = TroveManager::repay_loan(
            &mut trove_ctx,
            &mut collateral_ctx,
            &oracle_ctx,
            params.amount,
            ctx.bumps.protocol_collateral_account,
        )?;
        
        // Update state before contexts are dropped
        ctx.accounts.state.total_debt_amount = trove_ctx.state.total_debt_amount;
        
        Ok::<_, Error>(result)
    }?;
    
    // CRITICAL: Validate ICR ordering if neighbor hints provided
    // Production clients MUST provide neighbor hints via remainingAccounts for proper sorted list maintenance
    // Pattern: [prev_LiquidityThreshold, next_LiquidityThreshold] or [prev_LT] or [next_LT] or []
    // Optional for backward compatibility with tests, but REQUIRED in production
    if !ctx.remaining_accounts.is_empty() {
        use crate::sorted_troves;
        
        msg!("Validating ICR ordering with {} neighbor account(s)", ctx.remaining_accounts.len());
        
        let prev_icr = if ctx.remaining_accounts.len() >= 1 {
            let prev_lt = &ctx.remaining_accounts[0];
            let prev_data = prev_lt.try_borrow_data()?;
            let prev_threshold = LiquidityThreshold::try_deserialize(&mut &prev_data[..])?;
            let prev_owner = prev_threshold.owner;
            let prev_ratio = prev_threshold.ratio;
            drop(prev_data);
            
            // Verify this is a real PDA, not a fake account
            sorted_troves::verify_liquidity_threshold_pda(prev_lt, prev_owner, ctx.program_id)?;
            
            Some(prev_ratio)
        } else {
            None
        };
        
        let next_icr = if ctx.remaining_accounts.len() >= 2 {
            let next_lt = &ctx.remaining_accounts[1];
            let next_data = next_lt.try_borrow_data()?;
            let next_threshold = LiquidityThreshold::try_deserialize(&mut &next_data[..])?;
            let next_owner = next_threshold.owner;
            let next_ratio = next_threshold.ratio;
            drop(next_data);
            
            // Verify this is a real PDA, not a fake account
            sorted_troves::verify_liquidity_threshold_pda(next_lt, next_owner, ctx.program_id)?;
            
            Some(next_ratio)
        } else {
            None
        };
        
        // Validate ordering BEFORE updating state
        sorted_troves::validate_icr_ordering(result.new_icr, prev_icr, next_icr)?;
        msg!("✓ ICR ordering validated successfully");
    } else {
        msg!("⚠ WARNING: No neighbor hints provided - skipping ICR ordering validation");
        msg!("⚠ Production clients MUST provide neighbor hints for sorted list integrity");
    }
    
    // Update the actual accounts with the results
    ctx.accounts.user_debt_amount.amount = result.new_debt_amount;
    ctx.accounts.liquidity_threshold.ratio = result.new_icr;
    ctx.accounts.user_collateral_amount.amount = result.new_collateral_amount;

    // NOTE: Sorted troves management moved off-chain
    // If debt is fully repaid, trove is automatically removed from off-chain sorted list
    if result.new_debt_amount == 0 {
        msg!("Trove fully repaid - ready for off-chain list cleanup");
    }

    // Burn stablecoin
    let burn_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Burn {
            mint: ctx.accounts.stable_coin_mint.to_account_info(),
            from: ctx.accounts.user_stablecoin_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        },
    );
    anchor_spl::token::burn(burn_ctx, params.amount)?;
    
    msg!("Loan repaid successfully");
    msg!("Amount: {} aUSD", params.amount);
    msg!("Collateral denom: {}", params.collateral_denom);
    msg!("New debt amount: {}", result.new_debt_amount);
    msg!("New ICR: {}", result.new_icr);
    msg!("Collateral amount: {}", result.new_collateral_amount);
    
    Ok(())
}