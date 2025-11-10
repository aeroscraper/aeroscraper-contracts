use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount, transfer, Transfer};
use crate::state::FeeStateAccount;
use crate::error::AerospacerFeesError;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct DistributeFeeParams {
    pub fee_amount: u64,
}

#[derive(Accounts)]
#[instruction(params: DistributeFeeParams)]
pub struct DistributeFee<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    
    #[account(
        mut,
        seeds = [b"fee_state"],
        bump
    )]
    pub state: Account<'info, FeeStateAccount>,
    
    #[account(mut)]
    pub payer_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub stability_pool_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub fee_address_1_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub fee_address_2_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<DistributeFee>, params: DistributeFeeParams) -> Result<()> {
    let state = &mut ctx.accounts.state;
    let fee_amount = params.fee_amount;
    
    if fee_amount == 0 {
        return Err(AerospacerFeesError::NoFeesToDistribute.into());
    }
    
    // CRITICAL: Validate payer owns the payer_token_account to prevent unauthorized draining
    require!(
        ctx.accounts.payer_token_account.owner == ctx.accounts.payer.key(),
        AerospacerFeesError::UnauthorizedTokenAccount
    );
    
    // Validate all token accounts have the same mint
    let payer_mint = ctx.accounts.payer_token_account.mint;
    require!(
        ctx.accounts.stability_pool_token_account.mint == payer_mint,
        AerospacerFeesError::InvalidTokenMint
    );
    require!(
        ctx.accounts.fee_address_1_token_account.mint == payer_mint,
        AerospacerFeesError::InvalidTokenMint
    );
    require!(
        ctx.accounts.fee_address_2_token_account.mint == payer_mint,
        AerospacerFeesError::InvalidTokenMint
    );
    
    // Update total fees collected
    state.total_fees_collected = state.total_fees_collected
        .checked_add(fee_amount)
        .ok_or(AerospacerFeesError::Overflow)?;
    
    msg!("Distributing fee amount: {}", fee_amount);
    msg!("Total fees collected: {}", state.total_fees_collected);
    
    if state.is_stake_enabled {
        // Validate stake contract address is set
        require!(
            state.stake_contract_address != Pubkey::default(),
            AerospacerFeesError::StakeContractNotSet
        );
        
        // Validate stability pool token account owner matches stake contract address
        require!(
            ctx.accounts.stability_pool_token_account.owner == state.stake_contract_address,
            AerospacerFeesError::InvalidStabilityPoolAccount
        );
        
        msg!("Distributing fees to stability pool");
        
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.payer_token_account.to_account_info(),
                to: ctx.accounts.stability_pool_token_account.to_account_info(),
                authority: ctx.accounts.payer.to_account_info(),
            },
        );
        
        transfer(transfer_ctx, fee_amount)?;
        
        msg!("Fees distributed to stability pool successfully: {}", fee_amount);
    } else {
        // Validate fee address token account owners using state values
        // Note: ctx.accounts.fee_address_1_token_account.owner refers to the TOKEN ACCOUNT's owner field
        // (the wallet that owns the tokens), not the account's program owner (which is always Token Program)
        
        msg!("Validating fee address 1 token account owner");
        msg!("Expected owner: {}", state.fee_address_1);
        msg!("Actual owner: {}", ctx.accounts.fee_address_1_token_account.owner);
        
        require!(
            ctx.accounts.fee_address_1_token_account.owner == state.fee_address_1,
            AerospacerFeesError::InvalidFeeAddress1
        );
        
        msg!("Validating fee address 2 token account owner");
        msg!("Expected owner: {}", state.fee_address_2);
        msg!("Actual owner: {}", ctx.accounts.fee_address_2_token_account.owner);
        
        require!(
            ctx.accounts.fee_address_2_token_account.owner == state.fee_address_2,
            AerospacerFeesError::InvalidFeeAddress2
        );
        
        let half_amount = fee_amount / 2;
        let remaining_amount = fee_amount - half_amount;
        
        msg!("Distributing fees to fee addresses (50/50 split)");
        msg!("Half amount: {}", half_amount);
        msg!("Remaining amount: {}", remaining_amount);
        
        if half_amount > 0 {
            let transfer_ctx_1 = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.payer_token_account.to_account_info(),
                    to: ctx.accounts.fee_address_1_token_account.to_account_info(),
                    authority: ctx.accounts.payer.to_account_info(),
                },
            );
            
            transfer(transfer_ctx_1, half_amount)?;
            msg!("Fees transferred to fee address 1: {}", half_amount);
        }
        
        if remaining_amount > 0 {
            let transfer_ctx_2 = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.payer_token_account.to_account_info(),
                    to: ctx.accounts.fee_address_2_token_account.to_account_info(),
                    authority: ctx.accounts.payer.to_account_info(),
                },
            );
            
            transfer(transfer_ctx_2, remaining_amount)?;
            msg!("Fees transferred to fee address 2: {}", remaining_amount);
        }
        
        msg!("Fees distributed to fee addresses successfully");
    }
    
    Ok(())
}

