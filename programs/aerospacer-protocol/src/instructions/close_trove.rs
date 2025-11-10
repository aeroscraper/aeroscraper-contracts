use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount, Transfer, Burn};
use crate::state::*;
use crate::error::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CloseTroveParams {
    pub collateral_denom: String,
}

#[derive(Accounts)]
#[instruction(params: CloseTroveParams)]
pub struct CloseTrove<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"user_debt_amount", user.key().as_ref()],
        bump,
        constraint = user_debt_amount.owner == user.key() @ AerospacerProtocolError::Unauthorized,
        constraint = user_debt_amount.amount > 0 @ AerospacerProtocolError::TroveDoesNotExist
    )]
    pub user_debt_amount: Box<Account<'info, UserDebtAmount>>,

    #[account(
        mut,
        seeds = [b"user_collateral_amount", user.key().as_ref(), params.collateral_denom.as_bytes()],
        bump,
        constraint = user_collateral_amount.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub user_collateral_amount: Box<Account<'info, UserCollateralAmount>>,

    #[account(
        mut,
        close = user,
        seeds = [b"liquidity_threshold", user.key().as_ref()],
        bump,
        constraint = liquidity_threshold.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub liquidity_threshold: Box<Account<'info, LiquidityThreshold>>,

    #[account(mut)]
    pub state: Box<Account<'info, StateAccount>>,

    // User's stablecoin account (to pay off debt)
    #[account(
        mut,
        constraint = user_stablecoin_account.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub user_stablecoin_account: Box<Account<'info, TokenAccount>>,

    // User's collateral account (to receive collateral back)
    #[account(
        mut,
        constraint = user_collateral_account.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub user_collateral_account: Box<Account<'info, TokenAccount>>,

    // Protocol's collateral vault
    #[account(
        mut,
        seeds = [b"protocol_collateral_vault", params.collateral_denom.as_bytes()],
        bump,
        constraint = protocol_collateral_vault.mint == user_collateral_account.mint @ AerospacerProtocolError::InvalidMint
    )]
    pub protocol_collateral_vault: Box<Account<'info, TokenAccount>>,

    /// CHECK: This is the stable coin mint account
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
    pub total_collateral_amount: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<CloseTrove>, params: CloseTroveParams) -> Result<()> {
    // Validate collateral denomination
    require!(
        !params.collateral_denom.is_empty(),
        AerospacerProtocolError::InvalidAmount
    );
    
    // Apply pending redistribution rewards before closing trove
    use crate::trove_management::apply_pending_rewards;
    let total_collateral_data = ctx.accounts.total_collateral_amount.try_borrow_mut_data()?;
    let total_collateral: TotalCollateralAmount = TotalCollateralAmount::try_from_slice(&total_collateral_data[8..])?;
    drop(total_collateral_data);
    
    apply_pending_rewards(
        &mut ctx.accounts.user_debt_amount,
        &mut ctx.accounts.user_collateral_amount,
        &total_collateral,
    )?;
    
    let debt_amount = ctx.accounts.user_debt_amount.amount;
    let collateral_amount = ctx.accounts.user_collateral_amount.amount;
    
    // Validate user has sufficient stablecoins to repay full debt
    require!(
        ctx.accounts.user_stablecoin_account.amount >= debt_amount,
        AerospacerProtocolError::InsufficientCollateral
    );
    
    msg!("Closing trove for user: {}", ctx.accounts.user.key());
    msg!("Debt to repay: {} aUSD", debt_amount);
    msg!("Collateral to return: {} {}", collateral_amount, params.collateral_denom);
    
    // STEP 1: Update global state BEFORE token operations (for atomicity)
    // If any subsequent CPI fails, this will rollback automatically
    ctx.accounts.state.total_debt_amount = ctx.accounts.state.total_debt_amount
        .checked_sub(debt_amount)
        .ok_or(AerospacerProtocolError::OverflowError)?;
    
    // Update total collateral for this denomination
    let mut total_collateral_data = ctx.accounts.total_collateral_amount.try_borrow_mut_data()?;
    let mut total_collateral: TotalCollateralAmount = TotalCollateralAmount::try_from_slice(&total_collateral_data[8..])?;
    total_collateral.amount = total_collateral.amount
        .checked_sub(collateral_amount)
        .ok_or(AerospacerProtocolError::OverflowError)?;
    total_collateral.try_serialize(&mut &mut total_collateral_data[8..])?;
    drop(total_collateral_data);
    
    msg!("Updated global state - debt: {}, collateral tracked", ctx.accounts.state.total_debt_amount);
    
    // STEP 2: Burn stablecoins to repay debt
    if debt_amount > 0 {
        let burn_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.stable_coin_mint.to_account_info(),
                from: ctx.accounts.user_stablecoin_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        );
        anchor_spl::token::burn(burn_ctx, debt_amount)?;
        
        msg!("Burned {} aUSD to repay debt", debt_amount);
    }
    
    // STEP 3: Transfer collateral back to user
    if collateral_amount > 0 {
        // Get PDA seeds for signing
        let collateral_denom_bytes = params.collateral_denom.as_bytes();
        let seeds = &[
            b"protocol_collateral_vault",
            collateral_denom_bytes,
            &[ctx.bumps.protocol_collateral_vault],
        ];
        let signer_seeds = &[&seeds[..]];
        
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.protocol_collateral_vault.to_account_info(),
                to: ctx.accounts.user_collateral_account.to_account_info(),
                authority: ctx.accounts.protocol_collateral_vault.to_account_info(),
            },
            signer_seeds,
        );
        anchor_spl::token::transfer(transfer_ctx, collateral_amount)?;
        
        msg!("Transferred {} {} back to user", collateral_amount, params.collateral_denom);
    }
    
    // STEP 4: Zero out user accounts AFTER successful token operations
    ctx.accounts.user_debt_amount.amount = 0;
    ctx.accounts.user_collateral_amount.amount = 0;
    
    // NOTE: Sorted troves management moved off-chain
    // LiquidityThreshold account is automatically closed via Anchor's `close` constraint
    // This ensures proper lamport refund and account cleanup
    
    msg!("Trove closed successfully - All accounts cleaned up");
    msg!("Final state:");
    msg!("  Debt repaid: {} aUSD", debt_amount);
    msg!("  Collateral returned: {} {}", collateral_amount, params.collateral_denom);
    msg!("  Total protocol debt: {}", ctx.accounts.state.total_debt_amount);
    
    Ok(())
}
