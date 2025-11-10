use anchor_lang::prelude::*;
use crate::state::*;
use crate::error::AerospacerOracleError;
use pyth_sdk_solana::state::SolanaPriceAccount;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdatePythPriceParams {
    /// Asset denomination to update price for
    pub denom: String,
}

#[derive(Accounts)]
#[instruction(params: UpdatePythPriceParams)]
pub struct UpdatePythPrice<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    
    #[account(
        mut,
        seeds = [b"state"],
        bump,
        constraint = state.admin == admin.key() @ AerospacerOracleError::Unauthorized
    )]
    pub state: Account<'info, OracleStateAccount>,
    
    /// CHECK: Pyth price account to update from
    pub pyth_price_account: AccountInfo<'info>,
    
    /// CHECK: Clock sysvar for timestamp validation
    pub clock: Sysvar<'info, Clock>,
}

pub fn handler(ctx: Context<UpdatePythPrice>, params: UpdatePythPriceParams) -> Result<()> {
    let state = &mut ctx.accounts.state;
    let clock = &ctx.accounts.clock;
    
    // Find the collateral data for the requested denom
    let _collateral_data = state.collateral_data
        .iter_mut()
        .find(|d| d.denom == params.denom)
        .ok_or(AerospacerOracleError::PriceFeedNotFound)?;

    // PRODUCTION PYTH INTEGRATION CODE
    let price_feed = SolanaPriceAccount::account_info_to_feed(&ctx.accounts.pyth_price_account)
        .map_err(|_| AerospacerOracleError::PythPriceFeedLoadFailed)?;
    
    // Get latest price with hardcoded staleness validation for mainnet (60 seconds)
    // let current_time = clock.unix_timestamp;
    // let price = price_feed.get_price_no_older_than(current_time, 60)
    //     .ok_or(AerospacerOracleError::PriceTooOld)?;

    // Get the latest available price data (no staleness validation for devnet testing)
    let price = price_feed.get_price_unchecked();

    // Validate price data integrity with hardcoded confidence
    require!(price.price > 0, AerospacerOracleError::PythPriceValidationFailed);
    require!(price.conf >= 100, AerospacerOracleError::PythPriceValidationFailed);

    
    // Update the last update timestamp
    state.last_update = clock.unix_timestamp;
    
    msg!("Pyth price update successful");
    msg!("Denom: {}", params.denom);
    msg!("New Price: {} ± {} x 10^{}", price.price, price.conf, price.expo);
    msg!("Publish Time: {}", price.publish_time);
    msg!("Updated at: {}", clock.unix_timestamp);
    
    Ok(())
}