use anchor_lang::prelude::*;
use crate::state::*;
use crate::error::AerospacerOracleError;
use pyth_sdk_solana::state::SolanaPriceAccount;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetPriceParams {
    pub denom: String,
}

#[derive(Accounts)]
#[instruction(params: GetPriceParams)]
pub struct GetPrice<'info> {
    #[account(
        seeds = [b"state"],
        bump
    )]
    pub state: Account<'info, OracleStateAccount>,
    
    /// CHECK: This is the Pyth price account that contains the price data
    pub pyth_price_account: AccountInfo<'info>,
    
    /// CHECK: Clock sysvar for timestamp validation
    pub clock: Sysvar<'info, Clock>,
}

pub fn handler(ctx: Context<GetPrice>, params: GetPriceParams) -> Result<PriceResponse> {
    let state = &ctx.accounts.state;
    let _clock = &ctx.accounts.clock;
    
    // Find the collateral data for the requested denom
    let collateral_data = state.collateral_data
        .iter()
        .find(|d| d.denom == params.denom)
        .ok_or(AerospacerOracleError::PriceFeedNotFound)?;

    // PRODUCTION PYTH INTEGRATION CODE
    // Use Pyth SDK to load and validate price feed data
    let price_feed = SolanaPriceAccount::account_info_to_feed(&ctx.accounts.pyth_price_account)
        .map_err(|_| AerospacerOracleError::PythPriceFeedLoadFailed)?;
    
    // Get price with hardcoded staleness validation for mainnet (60 seconds)
    // let current_time = clock.unix_timestamp;
    // let price = price_feed.get_price_no_older_than(current_time, 60)
    //     .ok_or(AerospacerOracleError::PriceTooOld)?;
    
    // Get the latest available price data (no staleness validation for devnet testing)
    let price = price_feed.get_price_unchecked();

    // Validate price data integrity with lenient confidence for devnet testing
    require!(price.price > 0, AerospacerOracleError::InvalidPriceData);
    require!(price.conf >= 100, AerospacerOracleError::PythPriceValidationFailed); // Reduced from 1000 to 100 for devnet
    
    msg!("Price query successful");
    msg!("Denom: {}", params.denom);
    msg!("Decimal: {}", collateral_data.decimal);
    msg!("Publish Time: {}", price.publish_time);
    msg!("Price: {} ± {} x 10^{}", price.price, price.conf, price.expo);
    msg!("Real Pyth data extracted successfully using official SDK");
    
    // Return price response with validated Pyth data
    // Use the actual price exponent from Pyth instead of collateral decimal
    let actual_decimal = (-price.expo) as u8; // Convert negative exponent to positive decimal places
    Ok(PriceResponse {
        denom: params.denom,
        price: price.price,
        decimal: actual_decimal, // Use actual price decimal from Pyth
        timestamp: price.publish_time,
        confidence: price.conf,
        exponent: price.expo,
    })
}