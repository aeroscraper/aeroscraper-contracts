use std::collections::HashMap;

use anchor_lang::prelude::*;
use crate::state::*;
use crate::error::*;

// LiquidityData is now defined in trove_management.rs
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct LiquidityData {
    pub denom: String,
    pub liquidity: u64, // Equivalent to Decimal256
    pub decimal: u8,
}

// Exact replication of INJECTIVE utils.rs
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct CollateralGain {
    pub block_height: u64,
    pub total_collateral_amount: u64, // Equivalent to Uint256
    pub amount: u64, // Equivalent to Uint256
    pub denom: String,
}

// PriceResponse equivalent for Solana
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct PriceResponse {
    pub denom: String,
    pub price: u64, // Equivalent to Uint256
    pub decimal: u8,
}

// NOTE: This function has been removed - use OracleContext::get_price() instead
// All price queries should go through the oracle.rs CPI integration with Pyth Network

pub fn get_liquidation_gains<'a>(
    user: Pubkey,
    state_account: &StateAccount,
    user_liquidation_collateral_gain_accounts: &'a [AccountInfo<'a>],
    total_liquidation_collateral_gain_accounts: &'a [AccountInfo<'a>],
    user_stake_amount_accounts: &'a [AccountInfo<'a>],
) -> Result<Vec<CollateralGain>> {
    let mut collateral_gains: Vec<CollateralGain> = vec![];

    // In Injective: TOTAL_LIQUIDATION_COLLATERAL_GAIN.range(storage, None, None, Order::Ascending)
    // For Solana: we would iterate through TotalLiquidationCollateralGain PDAs
    for account_info in total_liquidation_collateral_gain_accounts {
        let total_gain: Account<TotalLiquidationCollateralGain> = Account::try_from(account_info)?;
        let block_height = total_gain.block_height;
        let collateral_denom = total_gain.denom.clone();
        let total_collateral_amount = total_gain.amount;
        let total_stake_amount = state_account.total_stake_amount;

        // In Injective: USER_LIQUIDATION_COLLATERAL_GAIN.may_load(storage, (sender.clone(), block_height))
        // For Solana: check if user has already claimed this gain
        let user_liq_gain_seeds = UserLiquidationCollateralGain::seeds(&user, block_height);
        let (user_liq_gain_pda, _bump) = Pubkey::find_program_address(&user_liq_gain_seeds, &crate::ID);
        let mut already_claimed = false;
        for account in user_liquidation_collateral_gain_accounts {
            if account.key() == user_liq_gain_pda {
                let user_gain_account: Account<UserLiquidationCollateralGain> = Account::try_from(account)?;
                already_claimed = user_gain_account.claimed;
                break;
            }
        }

        if !already_claimed {
            // In Injective: USER_STAKE_AMOUNT.may_load_at_height(storage, sender.clone(), block_height)
            // For Solana: check user stake at specific block height (simplified)
            let user_stake_seeds = UserStakeAmount::seeds(&user);
            let (user_stake_pda, _bump) = Pubkey::find_program_address(&user_stake_seeds, &crate::ID);
            let mut user_stake_amount = 0u64;
            for account in user_stake_amount_accounts {
                if account.key() == user_stake_pda {
                    let stake_account: Account<UserStakeAmount> = Account::try_from(account)?;
                    // In Injective: SnapshotMap allows querying at specific block height
                    // For Solana: we would need to implement snapshotting or use current stake
                    user_stake_amount = stake_account.amount;
                    break;
                }
            }

            if user_stake_amount > 0 && total_stake_amount > 0 {
                // In Injective: Decimal256::from_ratio(stake_amount, total_stake_amount)
                // For Solana: simplified calculation
                let stake_percentage = (user_stake_amount * 1_000_000_000_000_000_000) / total_stake_amount; // Simplified Decimal256
                
                // In Injective: calculate_stake_amount(total_collateral_amount, stake_percentage, false)
                // For Solana: simplified calculation
                let collateral_gain = (total_collateral_amount * stake_percentage) / 1_000_000_000_000_000_000;
                
                collateral_gains.push(CollateralGain {
                    block_height,
                    total_collateral_amount,
                    amount: collateral_gain,
                    denom: collateral_denom,
                });
            }
        }
    }

    Ok(collateral_gains)
}

// Safe arithmetic functions - Exact replication from INJECTIVE
pub fn safe_add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(AerospacerProtocolError::OverflowError.into())
}

pub fn safe_sub(a: u64, b: u64) -> Result<u64> {
    a.checked_sub(b).ok_or(AerospacerProtocolError::OverflowError.into())
}

pub fn safe_mul(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or(AerospacerProtocolError::OverflowError.into())
}

pub fn safe_div(a: u64, b: u64) -> Result<u64> {
    if b == 0 {
        return Err(AerospacerProtocolError::DivideByZeroError.into());
    }
    a.checked_div(b).ok_or(AerospacerProtocolError::OverflowError.into())
}

// Helper function to update total collateral amount
pub fn update_total_collateral_from_account_info(
    account_info: &AccountInfo,
    amount_change: i64,
) -> Result<()> {
    use crate::state::TotalCollateralAmount;
    
    // Deserialize the TotalCollateralAmount account
    let mut data = account_info.try_borrow_mut_data()?;
    let mut total_collateral = TotalCollateralAmount::try_deserialize(&mut &data[..])?;
    
    // Apply the change
    if amount_change >= 0 {
        total_collateral.amount = total_collateral.amount
            .checked_add(amount_change as u64)
            .ok_or(AerospacerProtocolError::OverflowError)?;
    } else {
        total_collateral.amount = total_collateral.amount
            .checked_sub(amount_change.abs() as u64)
            .ok_or(AerospacerProtocolError::OverflowError)?;
    }
    
    // Serialize back to account
    total_collateral.try_serialize(&mut &mut data[..])?;
    
    msg!("Updated total collateral by: {} (new total: {})", amount_change, total_collateral.amount);
    Ok(())
}

// Fee calculation utilities for protocol-fees integration
pub fn calculate_protocol_fee(amount: u64, fee_percentage: u8) -> Result<u64> {
    let fee = amount
        .checked_mul(fee_percentage as u64)
        .ok_or(AerospacerProtocolError::OverflowError)?
        .checked_div(100)
        .ok_or(AerospacerProtocolError::OverflowError)?;
    
    Ok(fee)
}

pub fn calculate_net_amount_after_fee(amount: u64, fee_percentage: u8) -> Result<u64> {
    let fee = calculate_protocol_fee(amount, fee_percentage)?;
    amount
        .checked_sub(fee)
        .ok_or(AerospacerProtocolError::OverflowError.into())
}

/// Calculate real ICR for a trove with multi-collateral support
/// 
/// Returns ICR as a simple percentage (not scaled)
/// Example: 150% ICR = 150, 200% ICR = 200
/// 
/// This replaces the previous mock implementation
pub fn get_trove_icr<'a>(
    user_debt_amount: &UserDebtAmount,
    user_collateral_amount_accounts: &'a [AccountInfo<'a>],
    collateral_prices: &HashMap<String, u64>,
    owner: Pubkey,
) -> Result<u64> {
    use crate::oracle::PriceCalculator;
    
    let debt = user_debt_amount.amount;
    
    // If no debt, return maximum ratio
    if debt == 0 {
        return Ok(u64::MAX);
    }
    
    // Collect all collateral amounts for this user
    let mut collateral_amounts: Vec<(String, u64)> = Vec::new();
    
    for account_info in user_collateral_amount_accounts {
        // Try to deserialize the account data directly
        let account_data = account_info.try_borrow_data()?;
        
        // Skip if account is too small to be a UserCollateralAmount
        if account_data.len() < 8 + UserCollateralAmount::LEN {
            continue;
        }
        
        // Try to deserialize as UserCollateralAmount
        if let Ok(collateral_account) = UserCollateralAmount::try_from_slice(&account_data[8..]) {
            // Verify it belongs to the owner
            if collateral_account.owner == owner && collateral_account.amount > 0 {
                collateral_amounts.push((
                    collateral_account.denom.clone(),
                    collateral_account.amount,
                ));
            }
        }
    }
    
    // If no collateral, return 0 ratio (fully liquidatable)
    if collateral_amounts.is_empty() {
        return Ok(0);
    }
    
    // Convert HashMap prices to Vec format for PriceCalculator
    // Prices are stored as raw values, we need to add decimal information
    let mut price_data: Vec<(String, u64, u8)> = Vec::new();
    
    for (denom, _amount) in &collateral_amounts {
        if let Some(price) = collateral_prices.get(denom) {
            // Get ADJUSTED decimal precision for each denom (to produce micro-USD values)
            // Formula: adjusted_decimal = token_decimals + price_exponent - 6
            // Must match the oracle's adjusted_decimal calculation
            let decimal = match denom.as_str() {
                "SOL" => 11,    // token(9) + price_exp(8) - target(6) = 11
                "USDC" => 8,    // token(6) + price_exp(8) - target(6) = 8
                "INJ" => 20,    // token(18) + price_exp(8) - target(6) = 20
                "ATOM" => 8,    // token(6) + price_exp(8) - target(6) = 8
                _ => 8,         // Default: assume 6 token decimals + 8 price exp - 6 = 8
            };
            
            price_data.push((denom.clone(), *price, decimal));
        }
    }
    
    // Calculate total collateral value and ICR
    let icr = PriceCalculator::calculate_trove_icr(
        &collateral_amounts,
        debt,
        &price_data,
    )?;
    
    Ok(icr)
}

/// Check if a trove's ICR meets the required minimum ratio
/// ICR and minimum_ratio are both simple percentages (e.g., 150 = 150%)
pub fn check_trove_icr_with_ratio(
    state_account: &StateAccount,
    icr: u64,
) -> Result<()> {
    let minimum_ratio = state_account.minimum_collateral_ratio as u64;
    
    require!(
        icr >= minimum_ratio,
        AerospacerProtocolError::CollateralBelowMinimum
    );
    
    Ok(())
}

/// Check if a trove is liquidatable based on its ICR
pub fn is_liquidatable_icr(icr: u64, liquidation_threshold: u64) -> bool {
    icr < liquidation_threshold
}

/// Get the liquidation threshold (typically 110%)
/// Returns as simple percentage: 110
pub fn get_liquidation_threshold() -> Result<u64> {
    // 110% ICR is the liquidation threshold
    Ok(110u64)
}

/// Check if ICR meets minimum collateral ratio requirement
/// Both ICR and minimum_collateral_ratio are simple percentages
pub fn check_minimum_icr(icr: u64, minimum_collateral_ratio: u8) -> Result<()> {
    let minimum_ratio = minimum_collateral_ratio as u64;
    
    require!(
        icr >= minimum_ratio,
        AerospacerProtocolError::CollateralBelowMinimum
    );
    
    Ok(())
}

// NOTE: Obsolete sorted list functions removed - using off-chain sorting architecture
// - get_first_trove: No longer needed (no sorted list state)
// - get_last_trove: No longer needed (no sorted list state)

/// Calculate compounded stake using Liquity Product-Sum algorithm
/// 
/// Formula: compounded_deposit = initial_deposit × (P_current / P_snapshot)
/// 
/// This accounts for pool depletion during liquidations:
/// - P_snapshot: P factor when user last deposited
/// - P_current: Current P factor
/// - Ratio P_current/P_snapshot represents the depletion factor
pub fn calculate_compounded_stake(
    initial_deposit: u64,
    p_snapshot: u128,
    p_current: u128,
) -> Result<u64> {
    // If P_snapshot is 0, this is first deposit or corrupted state - return initial
    if p_snapshot == 0 {
        return Ok(initial_deposit);
    }
    
    // If P_current is 0, pool is completely depleted - return 0
    if p_current == 0 {
        return Ok(0);
    }
    
    // Calculate: compounded = initial × (P_current / P_snapshot)
    // Use safe math to prevent overflow
    let deposit_u128 = initial_deposit as u128;
    
    // compounded = (deposit × P_current) / P_snapshot
    let numerator = deposit_u128
        .checked_mul(p_current)
        .ok_or(AerospacerProtocolError::OverflowError)?;
    
    let compounded = numerator
        .checked_div(p_snapshot)
        .ok_or(AerospacerProtocolError::DivideByZeroError)?;
    
    // Convert back to u64, capping at u64::MAX if overflow
    let result = if compounded > u64::MAX as u128 {
        u64::MAX
    } else {
        compounded as u64
    };
    
    Ok(result)
}

/// Calculate collateral gain using Liquity Product-Sum algorithm
/// 
/// Formula: gain = deposit × (S_current - S_snapshot) / P_snapshot
/// 
/// Where:
/// - S_snapshot: User's last recorded S factor for this collateral type
/// - S_current: Current S factor for this collateral type
/// - P_snapshot: User's P factor snapshot (accounts for pool depletion)
/// - deposit: User's stake amount
pub fn calculate_collateral_gain(
    deposit: u64,
    s_snapshot: u128,
    s_current: u128,
    p_snapshot: u128,
) -> Result<u64> {
    // If P_snapshot is 0, no valid snapshot exists - return 0
    if p_snapshot == 0 {
        return Ok(0);
    }
    
    // If S hasn't increased, no gain
    if s_current <= s_snapshot {
        return Ok(0);
    }
    
    // Calculate S_diff = S_current - S_snapshot
    let s_diff = s_current.saturating_sub(s_snapshot);
    
    // Calculate: gain = (deposit × S_diff) / P_snapshot
    let deposit_u128 = deposit as u128;
    
    let numerator = deposit_u128
        .checked_mul(s_diff)
        .ok_or(AerospacerProtocolError::OverflowError)?;
    
    let gain = numerator
        .checked_div(p_snapshot)
        .ok_or(AerospacerProtocolError::DivideByZeroError)?;
    
    // Convert back to u64, capping at u64::MAX if overflow
    let result = if gain > u64::MAX as u128 {
        u64::MAX
    } else {
        gain as u64
    };
    
    Ok(result)
}
