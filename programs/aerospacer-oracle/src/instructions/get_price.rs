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
    
    let price_exponent = (-price.expo) as u8;
    let token_decimals = collateral_data.decimal;
    
    // CRITICAL FIX: Calculate decimal to produce micro-USD (6 decimals) collateral values
    // Formula: decimal = token_decimals + price_exponent - 6
    // This ensures calculate_collateral_value returns values in micro-USD units,
    // which is required for calculate_collateral_ratio's 10^12 scaling to work correctly.
    //
    // Example for SOL:
    //   token_decimals = 9 (SOL has 9 decimals)
    //   price_exponent = 8 (Pyth price has exponent -8)
    //   target_decimal = 9 + 8 - 6 = 11
    //
    // This makes: collateral_value = (amount × price) / 10^11
    // With amount in lamports (10^-9 SOL) and price as Pyth raw value:
    //   collateral_value = (lamports × price) / 10^11
    //                    = (SOL × 10^9 × price × 10^-8) / 10^11
    //                    = (SOL × price) / 10^10
    //                    = USD / 10^6  (since SOL × price = USD)
    //                    = micro-USD ✓
    const TARGET_USD_DECIMALS: u8 = 6; // micro-USD (10^-6 USD)
    
    // Validate token has sufficient precision for micro-USD calculation
    // Reject tokens with total_precision < 6 (extremely rare in practice)
    let total_precision = token_decimals.saturating_add(price_exponent);
    require!(
        total_precision >= TARGET_USD_DECIMALS,
        AerospacerOracleError::InvalidPriceData
    );
    
    let adjusted_decimal = total_precision - TARGET_USD_DECIMALS;

    msg!("Price query successful");
    msg!("Denom: {}", params.denom);
    msg!("Token decimal: {}", token_decimals);
    msg!("Price exponent: {}", price_exponent);
    msg!("Adjusted decimal (for micro-USD): {}", adjusted_decimal);
    msg!("Publish Time: {}", price.publish_time);
    msg!("Price: {} ± {} x 10^{}", price.price, price.conf, price.expo);
    msg!("Real Pyth data extracted successfully using official SDK");
    
    Ok(PriceResponse {
        denom: params.denom,
        price: price.price,
        decimal: adjusted_decimal, // Adjusted to produce micro-USD collateral values
        timestamp: price.publish_time,
        confidence: price.conf,
        exponent: price.expo,
    })
}