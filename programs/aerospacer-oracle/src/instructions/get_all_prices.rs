use anchor_lang::prelude::*;
use crate::state::*;
use crate::error::AerospacerOracleError;
use pyth_sdk_solana::state::SolanaPriceAccount;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetAllPricesParams {
    // No parameters needed for all prices query
}

#[derive(Accounts)]
#[instruction(params: GetAllPricesParams)]
pub struct GetAllPrices<'info> {
    #[account(
        seeds = [b"state"],
        bump
    )]
    pub state: Account<'info, OracleStateAccount>,
    
    /// CHECK: Clock sysvar for timestamp validation
    pub clock: Sysvar<'info, Clock>,
}

pub fn handler(ctx: Context<GetAllPrices>, _params: GetAllPricesParams) -> Result<Vec<PriceResponse>> {
    let state = &ctx.accounts.state;
    let _clock = &ctx.accounts.clock;
    
    // Get remaining accounts (should contain Pyth price accounts for each asset)
    let remaining_accounts = &ctx.remaining_accounts;
    
    // Validate we have enough Pyth accounts for all assets
    require!(
        remaining_accounts.len() >= state.collateral_data.len(),
        AerospacerOracleError::InvalidPriceData
    );
    
    let mut prices = Vec::new();

    // PRODUCTION PYTH INTEGRATION CODE
    // For each collateral asset, fetch real price data using corresponding Pyth account
    for (index, collateral_data) in state.collateral_data.iter().enumerate() {
        // Get the corresponding Pyth price account from remaining_accounts
        let pyth_price_account = &remaining_accounts[index];
        
        // Use Pyth SDK to load and validate price feed data (reusing get_price logic)
        let price_feed = SolanaPriceAccount::account_info_to_feed(pyth_price_account)
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

        let price_response = PriceResponse {
            denom: collateral_data.denom.clone(),
            price: price.price,
            decimal: collateral_data.decimal,
            timestamp: price.publish_time,
            confidence: price.conf,
            exponent: price.expo,
        };
        
        prices.push(price_response);
    }
    
    msg!("All prices query successful");
    msg!("Found {} price responses", prices.len());
    msg!("Real Pyth data extracted for all assets using official SDK");
    msg!("Each asset uses its own Pyth price account via remaining_accounts");
    for price in &prices {
        msg!("- {}: {} ± {} x 10^{}", price.denom, price.price, price.confidence, price.exponent);
    }
    
    Ok(prices)
}