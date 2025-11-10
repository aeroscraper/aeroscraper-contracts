use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount, Mint, MintTo};
use crate::state::*;
use crate::error::*;
use crate::account_management::*;
use crate::oracle::*;
use crate::trove_management::TroveManager;
use crate::state::{MINIMUM_LOAN_AMOUNT, MINIMUM_COLLATERAL_AMOUNT};
use crate::fees_integration::*;
use crate::utils::*;

// Oracle integration is now handled via our aerospacer-oracle contract

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct OpenTroveParams {
    pub loan_amount: u64,
    pub collateral_denom: String,
    pub collateral_amount: u64,
}

#[derive(Accounts)]
#[instruction(params: OpenTroveParams)]
pub struct OpenTrove<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    
    // Trove context accounts - Box<> to reduce stack usage
    #[account(
        init,
        payer = user,
        space = 8 + UserDebtAmount::LEN,
        seeds = [b"user_debt_amount", user.key().as_ref()],
        bump
    )]
    pub user_debt_amount: Box<Account<'info, UserDebtAmount>>,
    
    #[account(
        init,
        payer = user,
        space = 8 + LiquidityThreshold::LEN,
        seeds = [b"liquidity_threshold", user.key().as_ref()],
        bump
    )]
    pub liquidity_threshold: Box<Account<'info, LiquidityThreshold>>,
    
    // Collateral context accounts
    #[account(
        init,
        payer = user,
        space = 8 + UserCollateralAmount::LEN,
        seeds = [b"user_collateral_amount", user.key().as_ref(), params.collateral_denom.as_bytes()],
        bump
    )]
    pub user_collateral_amount: Box<Account<'info, UserCollateralAmount>>,
    
    #[account(
        mut,
        constraint = user_collateral_account.owner == user.key() @ AerospacerProtocolError::Unauthorized,
        constraint = user_collateral_account.mint == collateral_mint.key() @ AerospacerProtocolError::InvalidMint
    )]
    pub user_collateral_account: Box<Account<'info, TokenAccount>>,
    
    pub collateral_mint: Box<Account<'info, Mint>>,
    
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
        init_if_needed,
        payer = user,
        space = 8 + TotalCollateralAmount::LEN,
        seeds = [b"total_collateral_amount", params.collateral_denom.as_bytes()],
        bump
    )]
    pub total_collateral_amount: Box<Account<'info, TotalCollateralAmount>>,
    
    // State account - Box<> to reduce stack usage
    #[account(mut)]
    pub state: Box<Account<'info, StateAccount>>,
    
    // Token accounts - Box<> to reduce stack usage
    #[account(
        mut,
        constraint = user_stablecoin_account.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub user_stablecoin_account: Box<Account<'info, TokenAccount>>,
    
    #[account(
        init_if_needed,
        payer = user,
        token::mint = stable_coin_mint,
        token::authority = protocol_stablecoin_account,
        seeds = [b"protocol_stablecoin_vault"],
        bump
    )]
    pub protocol_stablecoin_account: Box<Account<'info, TokenAccount>>,
    
    #[account(
        mut,
        constraint = stable_coin_mint.key() == state.stable_coin_addr @ AerospacerProtocolError::InvalidMint
    )]
    pub stable_coin_mint: Box<Account<'info, Mint>>,
    
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
    
    // Fee distribution accounts - UncheckedAccount to reduce stack usage
    /// CHECK: Fees program - validated against state in handler
    pub fees_program: UncheckedAccount<'info>,
    
    /// CHECK: Fees state account - validated against state in handler
    #[account(mut)]
    pub fees_state: UncheckedAccount<'info>,
    
    /// CHECK: Stability pool token account
    #[account(mut)]
    pub stability_pool_token_account: UncheckedAccount<'info>,
    
    /// CHECK: Fee address 1 token account
    #[account(mut)]
    pub fee_address_1_token_account: UncheckedAccount<'info>,
    
    /// CHECK: Fee address 2 token account
    #[account(mut)]
    pub fee_address_2_token_account: UncheckedAccount<'info>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<OpenTrove>, params: OpenTroveParams) -> Result<()> {
    // Validate oracle accounts
    require!(
        ctx.accounts.oracle_program.key() == ctx.accounts.state.oracle_helper_addr,
        AerospacerProtocolError::Unauthorized
    );
    require!(
        ctx.accounts.oracle_state.key() == ctx.accounts.state.oracle_state_addr,
        AerospacerProtocolError::Unauthorized
    );
    
    // Validate fee accounts
    require!(
        ctx.accounts.fees_program.key() == ctx.accounts.state.fee_distributor_addr,
        AerospacerProtocolError::Unauthorized
    );
    require!(
        ctx.accounts.fees_state.key() == ctx.accounts.state.fee_state_addr,
        AerospacerProtocolError::Unauthorized
    );
    
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
        params.collateral_amount > 0,
        AerospacerProtocolError::InvalidAmount
    );
    
    require!(
        params.collateral_amount >= MINIMUM_COLLATERAL_AMOUNT,
        AerospacerProtocolError::CollateralBelowMinimum
    );
    
    require!(
        !params.collateral_denom.is_empty(),
        AerospacerProtocolError::InvalidAmount
    );
    
    // Check if user already has a trove (should be 0 for new trove)
    require!(
        ctx.accounts.user_debt_amount.amount == 0,
        AerospacerProtocolError::TroveExists
    );
    
    // Check if user has sufficient collateral
    require!(
        ctx.accounts.user_collateral_account.amount >= params.collateral_amount,
        AerospacerProtocolError::InsufficientCollateral
    );
    
    // Initialize user debt amount
    ctx.accounts.user_debt_amount.owner = ctx.accounts.user.key();
    ctx.accounts.user_debt_amount.amount = 0; // Will be set below
    ctx.accounts.user_debt_amount.l_debt_snapshot = 0; // Will be set to current global L value later
    
    // Initialize user collateral amount
    ctx.accounts.user_collateral_amount.owner = ctx.accounts.user.key();
    ctx.accounts.user_collateral_amount.denom = params.collateral_denom.clone();
    ctx.accounts.user_collateral_amount.amount = 0; // Will be set below
    ctx.accounts.user_collateral_amount.l_collateral_snapshot = 0; // Will be set to current global L value later
    
    // Initialize liquidity threshold
    ctx.accounts.liquidity_threshold.owner = ctx.accounts.user.key();
    ctx.accounts.liquidity_threshold.ratio = 0; // Will be set below
    
    // Calculate opening fee BEFORE trove operations
    let fee_amount = calculate_protocol_fee(params.loan_amount, ctx.accounts.state.protocol_fee)?;
    let net_loan_amount = params.loan_amount.saturating_sub(fee_amount);
    
    msg!("Opening fee: {} aUSD ({}%)", fee_amount, ctx.accounts.state.protocol_fee);
    msg!("Net loan amount: {} aUSD", net_loan_amount);
    
    // Create contexts in scoped block to reduce stack usage
    // Execute trove operations and capture results
    let result = {
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
            total_collateral_amount: (*ctx.accounts.total_collateral_amount).clone(),
            token_program: ctx.accounts.token_program.clone(),
        };
        
        let oracle_ctx = OracleContext {
            oracle_program: ctx.accounts.oracle_program.to_account_info(),
            oracle_state: ctx.accounts.oracle_state.to_account_info(),
            pyth_price_account: ctx.accounts.pyth_price_account.to_account_info(),
            clock: ctx.accounts.clock.to_account_info(),
        };
        
        // Use TroveManager with NET loan amount (after fee)
        let result = TroveManager::open_trove(
            &mut trove_ctx,
            &mut collateral_ctx,
            &oracle_ctx,
            net_loan_amount,  // Use net amount for debt recording
            params.collateral_amount,
            params.collateral_denom.clone(),
        )?;
        
        // Update state total debt before contexts are dropped
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
            // First account is previous neighbor's LiquidityThreshold
            let prev_lt = &ctx.remaining_accounts[0];
            let prev_data = prev_lt.try_borrow_data()?;
            let prev_threshold = LiquidityThreshold::try_deserialize(&mut &prev_data[..])?;
            let prev_owner = prev_threshold.owner;
            let prev_ratio = prev_threshold.ratio;
            drop(prev_data);
            
            // Verify this is a real PDA, not a fake account
            sorted_troves::verify_liquidity_threshold_pda(prev_lt, prev_owner, ctx.program_id)?;
            
            msg!("Previous neighbor: owner={}, ICR={}", prev_owner, prev_ratio);
            Some(prev_ratio)
        } else {
            None
        };
        
        let next_icr = if ctx.remaining_accounts.len() >= 2 {
            // Second account is next neighbor's LiquidityThreshold
            let next_lt = &ctx.remaining_accounts[1];
            let next_data = next_lt.try_borrow_data()?;
            let next_threshold = LiquidityThreshold::try_deserialize(&mut &next_data[..])?;
            let next_owner = next_threshold.owner;
            let next_ratio = next_threshold.ratio;
            drop(next_data);
            
            // Verify this is a real PDA, not a fake account
            sorted_troves::verify_liquidity_threshold_pda(next_lt, next_owner, ctx.program_id)?;
            
            msg!("Next neighbor: owner={}, ICR={}", next_owner, next_ratio);
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
    
    // Initialize total_collateral_amount if it was just created
    if ctx.accounts.total_collateral_amount.denom.is_empty() {
        ctx.accounts.total_collateral_amount.denom = params.collateral_denom.clone();
        ctx.accounts.total_collateral_amount.amount = params.collateral_amount;
        ctx.accounts.total_collateral_amount.l_debt = 0;
        ctx.accounts.total_collateral_amount.l_collateral = 0;
        
        msg!("First trove for {} - initializing L factors to 0", params.collateral_denom);
    } else {
        // Update existing total
        ctx.accounts.total_collateral_amount.amount = ctx.accounts.total_collateral_amount.amount
            .checked_add(params.collateral_amount)
            .ok_or(AerospacerProtocolError::OverflowError)?;
    }
    
    // CRITICAL: Set L snapshots to current global values to prevent unearned retroactive rewards
    // When a new trove opens after redistributions have occurred, it should NOT receive rewards
    // from liquidations that happened before it existed
    ctx.accounts.user_debt_amount.l_debt_snapshot = ctx.accounts.total_collateral_amount.l_debt;
    ctx.accounts.user_collateral_amount.l_collateral_snapshot = ctx.accounts.total_collateral_amount.l_collateral;
    
    msg!("Initialized user L snapshots: l_debt={}, l_collateral={}", 
         ctx.accounts.user_debt_amount.l_debt_snapshot,
         ctx.accounts.user_collateral_amount.l_collateral_snapshot);
    
    // Mint full loan amount to user first (user requested full amount, will pay fee from it)
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
    
    // Distribute opening fee via CPI to aerospacer-fees
    if fee_amount > 0 {
        let _net_amount = process_protocol_fee(
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
        
        msg!("Opening fee collected and distributed: {} aUSD", fee_amount);
        msg!("Net loan amount after fee: {} aUSD", net_loan_amount);
    }
    
    // Log success
    msg!("Trove opened successfully");
    msg!("User: {}", ctx.accounts.user.key());
    msg!("Loan amount: {} aUSD (fee: {})", params.loan_amount, fee_amount);
    msg!("Collateral: {} {}", params.collateral_amount, params.collateral_denom);
    msg!("ICR: {}", result.new_icr);
    
    Ok(())
}