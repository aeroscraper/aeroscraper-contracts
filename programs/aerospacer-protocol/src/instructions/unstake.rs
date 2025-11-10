use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount, Transfer};
use crate::state::*;
use crate::utils::*;
use crate::error::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UnstakeParams {
    pub amount: u64, // Equivalent to Uint256
}

#[derive(Accounts)]
#[instruction(params: UnstakeParams)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"user_stake_amount", user.key().as_ref()],
        bump,
        constraint = user_stake_amount.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub user_stake_amount: Account<'info, UserStakeAmount>,

    #[account(mut)]
    pub state: Account<'info, StateAccount>,

    #[account(
        mut,
        constraint = user_stablecoin_account.owner == user.key() @ AerospacerProtocolError::Unauthorized
    )]
    pub user_stablecoin_account: Account<'info, TokenAccount>,

    /// CHECK: Protocol stablecoin vault PDA
    #[account(
        mut,
        seeds = [b"protocol_stablecoin_vault"],
        bump
    )]
    pub protocol_stablecoin_vault: AccountInfo<'info>,

    /// CHECK: This is the stable coin mint account
    #[account(
        constraint = stable_coin_mint.key() == state.stable_coin_addr @ AerospacerProtocolError::InvalidMint
    )]
    pub stable_coin_mint: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}



pub fn handler(ctx: Context<Unstake>, params: UnstakeParams) -> Result<()> {
    // Validate input parameters
    require!(
        params.amount > 0,
        AerospacerProtocolError::InvalidAmount
    );

    let user_stake_amount = &mut ctx.accounts.user_stake_amount;
    let state = &mut ctx.accounts.state;

    // SNAPSHOT: Calculate compounded stake accounting for pool depletion
    let compounded_stake = calculate_compounded_stake(
        user_stake_amount.amount,
        user_stake_amount.p_snapshot,
        state.p_factor,
    )?;

    // Check if user has enough compounded stake (NOT original deposit)
    require!(
        compounded_stake >= params.amount,
        AerospacerProtocolError::InvalidAmount
    );
    
    // CRITICAL: Allow full withdrawal even if below minimum (to prevent fund trapping after liquidations)
    // Only enforce minimum for partial withdrawals
    let is_full_withdrawal = params.amount == compounded_stake;
    if !is_full_withdrawal {
        require!(
            params.amount >= MINIMUM_LOAN_AMOUNT,
            AerospacerProtocolError::InvalidAmount
        );
    }

    // Transfer stablecoin back to user from protocol vault (Injective: CW20 transfer)
    let transfer_seeds = &[
        b"protocol_stablecoin_vault".as_ref(),
        &[ctx.bumps.protocol_stablecoin_vault],
    ];
    let transfer_signer = &[&transfer_seeds[..]];

    let transfer_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.protocol_stablecoin_vault.to_account_info(),
            to: ctx.accounts.user_stablecoin_account.to_account_info(),
            authority: ctx.accounts.protocol_stablecoin_vault.to_account_info(),
        },
        transfer_signer,
    );
    anchor_spl::token::transfer(transfer_ctx, params.amount)?;

    // Update user stake amount - subtract from original deposit proportionally
    let remaining_compounded = safe_sub(compounded_stake, params.amount)?;
    
    // Calculate new deposit amount: remaining_compounded / (P_current / P_snapshot)
    // = remaining_compounded * P_snapshot / P_current
    let new_deposit = if remaining_compounded == 0 {
        0u64
    } else {
        let remaining_128 = remaining_compounded as u128;
        let numerator = remaining_128
            .checked_mul(user_stake_amount.p_snapshot)
            .ok_or(AerospacerProtocolError::MathOverflow)?;
        let result = numerator
            .checked_div(state.p_factor)
            .ok_or(AerospacerProtocolError::MathOverflow)?;
        u64::try_from(result)
            .map_err(|_| AerospacerProtocolError::MathOverflow)?
    };

    user_stake_amount.amount = new_deposit;
    user_stake_amount.last_update_block = Clock::get()?.slot;
    
    // CRITICAL FIX: Update snapshots to current state after withdrawal
    // Without this, future compounding uses stale P/epoch and misprices stakes
    if new_deposit > 0 {
        // Partial withdrawal - refresh snapshots to current scale
        user_stake_amount.p_snapshot = state.p_factor;
        user_stake_amount.epoch_snapshot = state.epoch;
        msg!("Snapshots refreshed: P={}, epoch={}", state.p_factor, state.epoch);
    } else {
        // Full withdrawal - clear snapshots for hygiene
        user_stake_amount.p_snapshot = 0;
        user_stake_amount.epoch_snapshot = 0;
        msg!("Full withdrawal - snapshots cleared");
    }

    // Update state
    state.total_stake_amount = safe_sub(state.total_stake_amount, params.amount)?;

    msg!("Unstaked successfully (compounded stake calculated)");
    msg!("User: {}", ctx.accounts.user.key());
    msg!("Amount withdrawn: {} aUSD", params.amount);
    msg!("Compounded stake before: {} aUSD", compounded_stake);
    msg!("Remaining deposit: {} aUSD", user_stake_amount.amount);
    msg!("Total protocol stake: {} aUSD", state.total_stake_amount);

    Ok(())
}