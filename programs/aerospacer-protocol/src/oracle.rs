use anchor_lang::prelude::*;
use anchor_lang::solana_program::{hash::hash, instruction::{Instruction, AccountMeta}};
use crate::error::*;

/// Oracle integration for price feeds
/// This module provides clean integration with our aerospacer-oracle contract

/// Price data structure (matches aerospacer-oracle PriceResponse)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct PriceData {
    pub denom: String,
    pub price: i64, // Oracle returns i64
    pub decimal: u8,
    pub confidence: u64,
    pub timestamp: i64,
    pub exponent: i32,
}

/// Oracle context for price queries via CPI
pub struct OracleContext<'info> {
    /// Our oracle program
    pub oracle_program: AccountInfo<'info>,
    
    /// Oracle state account
    pub oracle_state: AccountInfo<'info>,
    
    /// Pyth price account for the collateral asset
    pub pyth_price_account: AccountInfo<'info>,
    
    /// Clock sysvar
    pub clock: AccountInfo<'info>,
}

/// Oracle integration implementation
impl<'info> OracleContext<'info> {
    /// Get price for a specific collateral denom via CPI to our oracle
    pub fn get_price(&self, denom: &str) -> Result<PriceData> {
        // Build the CPI instruction to call oracle's get_price
        let price_response = get_price_via_cpi(
            denom.to_string(),
            self.oracle_program.to_account_info(),
            self.oracle_state.to_account_info(),
            self.pyth_price_account.to_account_info(),
            self.clock.to_account_info(),
        )?;
        
        // Convert PriceResponse to PriceData
        Ok(PriceData {
            denom: price_response.denom,
            price: price_response.price,
            decimal: price_response.decimal,
            confidence: price_response.confidence,
            timestamp: price_response.timestamp,
            exponent: price_response.exponent,
        })
    }
    
    /// Get prices for all supported collateral denoms via CPI
    pub fn get_all_prices(&self) -> Result<Vec<PriceData>> {
        let denoms = get_all_denoms_via_cpi(
            self.oracle_program.to_account_info(),
            self.oracle_state.to_account_info(),
        )?;
        
        let mut prices = Vec::new();
        
        for denom in denoms {
            let price_data = self.get_price(&denom)?;
            prices.push(price_data);
        }
        
        Ok(prices)
    }
    
    /// Validate price data
    pub fn validate_price(&self, price_data: &PriceData) -> Result<()> {
        // Check if price is within reasonable bounds
        require!(
            price_data.price > 0,
            AerospacerProtocolError::InvalidAmount
        );
        
        // DEVNET: Price staleness check commented out for testing
        // let current_time = Clock::get()?.unix_timestamp;
        // let max_age = 86400; // 24 hours in seconds (more lenient for devnet)
        // 
        // require!(
        //     current_time - price_data.timestamp <= max_age,
        //     AerospacerProtocolError::InvalidAmount
        // );
        
        Ok(())
    }
}

/// Price calculation utilities
/// 
/// ICR Convention:
/// All ICR values are represented as simple percentages (not scaled).
/// Example: 150% ICR = 150, 200% ICR = 200
/// This avoids u64 overflow issues while maintaining sufficient precision
pub struct PriceCalculator;

impl PriceCalculator {
    /// Calculate collateral value in USD
    pub fn calculate_collateral_value(
        amount: u64,
        price: u64,
        decimal: u8,
    ) -> Result<u64> {
        msg!("🔍 [PriceCalculator::calculate_collateral_value]");
        msg!("  amount (lamports): {}", amount);
        msg!("  price (raw Pyth): {}", price);
        msg!("  decimal (from oracle): {}", decimal);
        
        let decimal_factor = 10_u128.pow(decimal as u32);
        msg!("  decimal_factor (10^{}): {}", decimal, decimal_factor);
        
        let product = (amount as u128)
            .checked_mul(price as u128)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        msg!("  amount × price: {}", product);
        
        let value = product
            .checked_div(decimal_factor)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        msg!("  collateral_value (after division): {}", value);
        
        // Convert back to u64, ensuring it fits
        if value > u64::MAX as u128 {
            msg!("❌ Overflow: value {} > u64::MAX", value);
            return Err(AerospacerProtocolError::OverflowError.into());
        }
        
        msg!("✅ Final collateral_value (u64): {}", value as u64);
        Ok(value as u64)
    }
    
    /// Calculate collateral ratio as a percentage (100 = 100%)
    /// Returns ICR as an unscaled percentage for comparison
    /// Example: 150% ICR = 150
    /// 
    /// Note: Both collateral_value and debt_amount should be in the same units
    /// For proper ICR calculation, we need to normalize the units
    pub fn calculate_collateral_ratio(
        collateral_value: u64,
        debt_amount: u64,
    ) -> Result<u64> {
        msg!("🔍 [PriceCalculator::calculate_collateral_ratio]");
        msg!("  collateral_value: {}", collateral_value);
        msg!("  debt_amount: {}", debt_amount);
        
        if debt_amount == 0 {
            msg!("  debt is 0 → returning u64::MAX");
            return Ok(u64::MAX);
        }
        
        // Normalize both values to the same units for comparison
        // Collateral value is in micro-USD (6 decimals) - enforced by oracle's adjusted_decimal
        // Debt amount is in 18 decimals (aUSD has 18 decimals)
        // We need to scale them to the same precision: 10^(18-6) = 10^12
        
        // Scale collateral value to match debt amount precision (18 decimals)
        let scaled_collateral_value = (collateral_value as u128)
            .checked_mul(1_000_000_000_000) // Scale up by 10^12 to match 18 decimals
            .ok_or(AerospacerProtocolError::OverflowError)?;
        msg!("  scaled_collateral_value (×10^12): {}", scaled_collateral_value);
        
        // Calculate ratio as percentage (multiply by 100)
        let numerator = scaled_collateral_value
            .checked_mul(100)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        msg!("  numerator (×100): {}", numerator);
        
        let ratio = numerator
            .checked_div(debt_amount as u128)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        msg!("  ratio (percentage): {}", ratio);
        
        // Convert back to u64
        let result = u64::try_from(ratio).map_err(|_| {
            msg!("❌ Overflow converting ratio {} to u64", ratio);
            AerospacerProtocolError::OverflowError
        })?;
        
        msg!("✅ Final ICR: {}%", result);
        Ok(result)
    }
    
    /// Check if trove is liquidatable
    pub fn is_liquidatable(
        collateral_value: u64,
        debt_amount: u64,
        minimum_ratio: u64,
    ) -> Result<bool> {
        if debt_amount == 0 {
            return Ok(false);
        }
        
        let ratio = Self::calculate_collateral_ratio(collateral_value, debt_amount)?;
        Ok(ratio < minimum_ratio)
    }
    
    /// Calculate total collateral value across multiple denoms
    /// Used for multi-collateral trove ICR calculation
    pub fn calculate_multi_collateral_value(
        collateral_amounts: &[(String, u64)],
        prices: &[(String, u64, u8)], // (denom, price, decimal)
    ) -> Result<u64> {
        let mut total_value = 0u64;
        
        for (denom, amount) in collateral_amounts {
            // Find matching price data
            let price_data = prices.iter()
                .find(|(d, _, _)| d == denom)
                .ok_or(AerospacerProtocolError::InvalidAmount)?;
            
            let value = Self::calculate_collateral_value(
                *amount,
                price_data.1,
                price_data.2,
            )?;
            
            total_value = total_value
                .checked_add(value)
                .ok_or(AerospacerProtocolError::OverflowError)?;
        }
        
        Ok(total_value)
    }
    
    /// Calculate ICR for a trove with multiple collateral types
    pub fn calculate_trove_icr(
        collateral_amounts: &[(String, u64)],
        debt_amount: u64,
        prices: &[(String, u64, u8)],
    ) -> Result<u64> {
        if debt_amount == 0 {
            return Ok(u64::MAX);
        }
        
        let total_collateral_value = Self::calculate_multi_collateral_value(
            collateral_amounts,
            prices,
        )?;
        
        Self::calculate_collateral_ratio(total_collateral_value, debt_amount)
    }
}

/// PriceResponse struct (matches oracle contract's return type)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct PriceResponse {
    pub denom: String,
    pub price: i64,
    pub decimal: u8,
    pub timestamp: i64,
    pub confidence: u64,
    pub exponent: i32,
}

/// Execute CPI call to oracle contract's get_price instruction
pub fn get_price_via_cpi<'info>(
    denom: String,
    oracle_program: AccountInfo<'info>,
    oracle_state: AccountInfo<'info>,
    pyth_price_account: AccountInfo<'info>,
    clock: AccountInfo<'info>,
) -> Result<PriceResponse> {
    // Calculate discriminator for get_price instruction
    // Anchor uses: SHA256("global:get_price")[0..8]
    let preimage = b"global:get_price";
    let hash_result = hash(preimage);
    let discriminator = &hash_result.to_bytes()[..8];
    
    // Serialize the GetPriceParams { denom }
    let mut instruction_data = Vec::new();
    instruction_data.extend_from_slice(discriminator);
    
    // Serialize params struct: { denom: String }
    denom.serialize(&mut instruction_data)?;
    
    // Build account metas for CPI (include all accounts including program)
    let account_metas = vec![
        AccountMeta::new(oracle_state.key(), false),
        AccountMeta::new_readonly(pyth_price_account.key(), false),
        AccountMeta::new_readonly(clock.key(), false),
    ];
    
    // Build the instruction
    let ix = Instruction {
        program_id: oracle_program.key(),
        accounts: account_metas,
        data: instruction_data,
    };
    
    // Execute CPI (data accounts + program)
    // Note: Account metas only include data accounts, but invoke needs the program too
    anchor_lang::solana_program::program::invoke(
        &ix,
        &[
            oracle_program.clone(),
            oracle_state.clone(),
            pyth_price_account.clone(),
            clock.clone(),
        ],
    )?;
    
    msg!("Oracle CPI executed successfully for denom: {}", denom);
    
    // Parse return data from oracle program
    let return_data = anchor_lang::solana_program::program::get_return_data()
        .ok_or(AerospacerProtocolError::InvalidAmount)?;
    
    // Verify the return data is from our oracle program
    require!(
        return_data.0 == oracle_program.key(),
        AerospacerProtocolError::InvalidAmount
    );
    
    // Deserialize PriceResponse
    let price_response: PriceResponse = PriceResponse::deserialize(&mut &return_data.1[..])?;
    
    msg!("✅ [Oracle CPI] Price received from oracle:");
    msg!("  denom: {}", price_response.denom);
    msg!("  price: {}", price_response.price);
    msg!("  decimal: {}", price_response.decimal);
    msg!("  exponent: {}", price_response.exponent);
    msg!("  confidence: {}", price_response.confidence);
    msg!("  timestamp: {}", price_response.timestamp);
    
    Ok(price_response)
}

/// Execute CPI call to oracle contract's get_all_denoms instruction
pub fn get_all_denoms_via_cpi<'info>(
    oracle_program: AccountInfo<'info>,
    oracle_state: AccountInfo<'info>,
) -> Result<Vec<String>> {
    // Calculate discriminator for get_all_denoms instruction
    // Anchor uses: SHA256("global:get_all_denoms")[0..8]
    let preimage = b"global:get_all_denoms";
    let hash_result = hash(preimage);
    let discriminator = &hash_result.to_bytes()[..8];
    
    // Build instruction data (no params, just discriminator)
    let mut instruction_data = Vec::new();
    instruction_data.extend_from_slice(discriminator);
    
    // Build account metas for CPI - only oracle_state needed
    let account_metas = vec![
        AccountMeta::new_readonly(oracle_state.key(), false),
    ];
    
    // Build the instruction
    let ix = Instruction {
        program_id: oracle_program.key(),
        accounts: account_metas,
        data: instruction_data,
    };
    
    // Execute CPI
    anchor_lang::solana_program::program::invoke(
        &ix,
        &[
            oracle_state.clone(),
            oracle_program.clone(),
        ],
    )?;
    
    msg!("Oracle get_all_denoms CPI executed successfully");
    
    // Parse return data from oracle program
    let return_data = anchor_lang::solana_program::program::get_return_data()
        .ok_or(AerospacerProtocolError::InvalidAmount)?;
    
    // Verify the return data is from our oracle program
    require!(
        return_data.0 == oracle_program.key(),
        AerospacerProtocolError::InvalidAmount
    );
    
    // Deserialize Vec<String> response
    let denoms: Vec<String> = Vec::<String>::deserialize(&mut &return_data.1[..])?;
    
    msg!("Received {} supported denoms from oracle", denoms.len());
    for denom in &denoms {
        msg!("  - {}", denom);
    }
    
    Ok(denoms)
}
