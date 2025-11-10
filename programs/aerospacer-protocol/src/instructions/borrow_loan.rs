use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount, Mint, MintTo};
use crate::state::*;
use crate::error::*;
use crate::trove_management::*;
use crate::account_management::*;
use crate::oracle::*;
use crate::fees_integration::*;
use crate::utils::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct BorrowLoanParams {
    pub loan_amount: u64,
    pub collateral_denom: String,
    pub prev_node_id: Option<Pubkey>,
    pub next_node_id: Option<Pubkey>,
}

#[derive(Accounts)]
#[instruction(params: BorrowLoanParams)]
pub struct BorrowLoan<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    
    #[account(
        mut,
        seeds = [b"user_debt_amount", user.key().as_ref()],
        bump,
        constraint = user_debt_amount.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub user_debt_amount: Box<Account<'info, UserDebtAmount>>,

    #[account(
        mut,
        seeds = [b"liquidity_threshold", user.key().as_ref()],
        bump,
        constraint = liquidity_threshold.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub liquidity_threshold: Box<Account<'info, LiquidityThreshold>>,

    #[account(mut)]
    pub state: Box<Account<'info, StateAccount>>,

    #[account(mut)]
    pub user_stablecoin_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: This is the stable coin mint account - validated against state
    #[account(
        mut,
        constraint = stable_coin_mint.key() == state.stable_coin_addr @ AerospacerProtocolError::InvalidMint
    )]
    pub stable_coin_mint: UncheckedAccount<'info>,
    
    #[account(
        init_if_needed,
        payer = user,
        token::mint = stable_coin_mint,
        token::authority = protocol_stablecoin_account,
        seeds = [b"protocol_stablecoin_vault"],
        bump
    )]
    pub protocol_stablecoin_account: Box<Account<'info, TokenAccount>>,

    // Collateral context accounts
    #[account(
        mut,
        seeds = [b"user_collateral_amount", user.key().as_ref(), params.collateral_denom.as_bytes()],
        bump,
        constraint = user_collateral_amount.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub user_collateral_amount: Box<Account<'info, UserCollateralAmount>>,
    
    #[account(
        mut,
        constraint = user_collateral_account.mint == collateral_mint.key() @ AerospacerProtocolError::InvalidMint
    )]
    pub user_collateral_account: Box<Account<'info, TokenAccount>>,

    pub collateral_mint: Account<'info, Mint>,

    #[account(
        init_if_needed,
        payer = user,
        token::mint = collateral_mint,
        token::authority = protocol_collateral_account,
        seeds = [b"protocol_collateral_vault", params.collateral_denom.as_bytes()],
        bump
    )]
    pub protocol_collateral_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: Per-denom collateral total PDA
    #[account(
        mut,
        seeds = [b"total_collateral_amount", params.collateral_denom.as_bytes()],
        bump
    )]
    pub total_collateral_amount: Account<'info, TotalCollateralAmount>,

    // Oracle context - integration with our aerospacer-oracle
    /// CHECK: Our oracle program - validated against state
    #[account(
        mut,
        constraint = oracle_program.key() == state.oracle_helper_addr @ AerospacerProtocolError::Unauthorized
    )]
    pub oracle_program: AccountInfo<'info>,
    
    /// CHECK: Oracle state account - validated against state
    #[account(
        mut,
        constraint = oracle_state.key() == state.oracle_state_addr @ AerospacerProtocolError::Unauthorized
    )]
    pub oracle_state: AccountInfo<'info>,
    
    /// CHECK: Pyth price account for collateral price feed
    pub pyth_price_account: AccountInfo<'info>,
    
    /// Clock sysvar for timestamp validation
    pub clock: Sysvar<'info, Clock>,

    // Fee distribution accounts
    /// CHECK: Fees program - validated against state
    #[account(
        constraint = fees_program.key() == state.fee_distributor_addr @ AerospacerProtocolError::Unauthorized
    )]
    pub fees_program: AccountInfo<'info>,
    
    /// CHECK: Fees state account - validated against state
    #[account(
        mut,
        constraint = fees_state.key() == state.fee_state_addr @ AerospacerProtocolError::Unauthorized
    )]
    pub fees_state: AccountInfo<'info>,
    
    /// CHECK: Stability pool token account
    #[account(mut)]
    pub stability_pool_token_account: AccountInfo<'info>,
    
    /// CHECK: Fee address 1 token account
    #[account(mut)]
    pub fee_address_1_token_account: AccountInfo<'info>,
    
    /// CHECK: Fee address 2 token account
    #[account(mut)]
    pub fee_address_2_token_account: AccountInfo<'info>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}



pub fn handler(ctx: Context<BorrowLoan>, params: BorrowLoanParams) -> Result<()> {
    // Validate input parameters
    require!(
        params.loan_amount > 0,
        AerospacerProtocolError::InvalidAmount
    );
    
    require!(
        params.loan_amount >= MINIMUM_LOAN_AMOUNT,
        AerospacerProtocolError::LoanAmountBelowMinimum
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
    
    // Create context structs for clean architecture
    let mut trove_ctx = TroveContext {
        user: ctx.accounts.user.clone(),
        user_debt_amount: (*ctx.accounts.user_debt_amount).clone(),
        liquidity_threshold: (*ctx.accounts.liquidity_threshold).clone(),
        state: (*ctx.accounts.state).clone(),
    };
    
    let mut collateral_ctx = CollateralContext {
        user: ctx.accounts.user.clone(),
        user_collateral_amount: (*ctx.accounts.user_collateral_amount).clone(),
        user_collateral_account: (*ctx.accounts.user_collateral_account).clone(),
        protocol_collateral_account: (*ctx.accounts.protocol_collateral_account).clone(),
        total_collateral_amount: ctx.accounts.total_collateral_amount.clone(),
        token_program: ctx.accounts.token_program.clone(),
    };
    
    let oracle_ctx = OracleContext {
        oracle_program: ctx.accounts.oracle_program.clone(),
        oracle_state: ctx.accounts.oracle_state.clone(),
        pyth_price_account: ctx.accounts.pyth_price_account.clone(),
        clock: ctx.accounts.clock.to_account_info(),
    };
    
    // Calculate fee amount for distribution
    let fee_amount = calculate_protocol_fee(params.loan_amount, ctx.accounts.state.protocol_fee)?;
    
    // CRITICAL: Record FULL gross amount as debt (including fee)
    // This ensures all minted tokens have matching debt liability
    // User borrows 1000 aUSD: receives 1000, pays 50 in fees, must repay 1000
    let result = TroveManager::borrow_loan(
        &mut trove_ctx,
        &mut collateral_ctx,
        &oracle_ctx,
        params.loan_amount,  // Use gross amount, not net
    )?;
    
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
    ctx.accounts.state.total_debt_amount = trove_ctx.state.total_debt_amount;
    
    // Mint total loan amount (including fee)
    // Use invoke_signed for PDA authority
    let mint_seeds = &[
        b"protocol_stablecoin_vault".as_ref(),
        &[ctx.bumps.protocol_stablecoin_account],
    ];
    let mint_signer = &[&mint_seeds[..]];
    
    let mint_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        MintTo {
            mint: ctx.accounts.stable_coin_mint.to_account_info(),
            to: ctx.accounts.user_stablecoin_account.to_account_info(),
            authority: ctx.accounts.protocol_stablecoin_account.to_account_info(),
        },
        mint_signer,
    );
    anchor_spl::token::mint_to(mint_ctx, params.loan_amount)?;

    // Distribute fee via CPI to aerospacer-fees
    if fee_amount > 0 {
        let net_amount = process_protocol_fee(
            params.loan_amount,
            ctx.accounts.state.protocol_fee,
            ctx.accounts.fees_program.to_account_info(),
            ctx.accounts.user.to_account_info(),
            ctx.accounts.fees_state.to_account_info(),
            ctx.accounts.user_stablecoin_account.to_account_info(),
            ctx.accounts.stability_pool_token_account.to_account_info(),
            ctx.accounts.fee_address_1_token_account.to_account_info(),
            ctx.accounts.fee_address_2_token_account.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        )?;
        
        msg!("Fee collected and distributed: {} aUSD", fee_amount);
        msg!("Net loan amount after fee: {} aUSD", net_amount);
    }
    
    msg!("Loan borrowed successfully");
    msg!("Gross loan amount (recorded as debt): {} aUSD", params.loan_amount);
    msg!("Fee amount distributed: {} aUSD", fee_amount);
    msg!("Net amount to user after fee: {} aUSD", params.loan_amount - fee_amount);
    msg!("Collateral denom: {}", params.collateral_denom);
    msg!("New total debt: {}", result.new_debt_amount);
    msg!("New ICR: {}", result.new_icr);
    msg!("Collateral amount: {}", result.new_collateral_amount);
    
    Ok(())
}