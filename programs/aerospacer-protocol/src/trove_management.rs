use anchor_lang::prelude::*;
use crate::state::*;
use crate::error::*;
use crate::oracle::*;
use crate::account_management::*;

/// Trove management utilities
/// This module provides clean, type-safe trove operations

/// Trove operation result
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct TroveOperationResult {
    pub success: bool,
    pub new_debt_amount: u64,
    pub new_collateral_amount: u64,
    pub new_icr: u64,
    pub message: String,
}

/// Liquidation operation result
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct LiquidationResult {
    pub liquidated_count: u32,
    pub total_debt_liquidated: u64,
    pub total_collateral_gained: u64,
    pub liquidation_gains: Vec<(String, u64)>, // Changed from HashMap to Vec for Anchor compatibility
}

/// Trove manager for handling all trove operations
pub struct TroveManager;

impl TroveManager {
    /// Open a new trove
    pub fn open_trove(
        trove_ctx: &mut TroveContext,
        collateral_ctx: &mut CollateralContext,
        oracle_ctx: &OracleContext,
        loan_amount: u64,
        collateral_amount: u64,
        collateral_denom: String,
    ) -> Result<TroveOperationResult> {
        // Validate minimum amounts
        require!(
            loan_amount >= MINIMUM_LOAN_AMOUNT,
            AerospacerProtocolError::LoanAmountBelowMinimum
        );
        
        require!(
            collateral_amount >= MINIMUM_COLLATERAL_AMOUNT,
            AerospacerProtocolError::CollateralBelowMinimum
        );
        
        // Get collateral price
        let price_data = oracle_ctx.get_price(&collateral_denom)?;
        oracle_ctx.validate_price(&price_data)?;
        
        // Calculate collateral value using proper price data
        let collateral_value = PriceCalculator::calculate_collateral_value(
            collateral_amount,
            price_data.price as u64, // Convert i64 to u64
            price_data.decimal,
        )?;
        
        msg!("DEBUG - Collateral amount: {}", collateral_amount);
        msg!("DEBUG - Price: {}", price_data.price);
        msg!("DEBUG - Price decimal: {}", price_data.decimal);
        msg!("DEBUG - Calculated collateral value: {}", collateral_value);
        msg!("DEBUG - Loan amount: {}", loan_amount);
        
        // Calculate ICR using proper calculation
        let icr = PriceCalculator::calculate_collateral_ratio(
            collateral_value,
            loan_amount,
        )?;
        
        msg!("DEBUG - Calculated ICR: {}", icr);
        msg!("DEBUG - Minimum ICR required: {}", trove_ctx.state.minimum_collateral_ratio);
        
        // Check minimum collateral ratio
        let minimum_ratio = trove_ctx.state.minimum_collateral_ratio as u64;
        require!(
            icr >= minimum_ratio,
            AerospacerProtocolError::CollateralBelowMinimum
        );
        
        // Update accounts
        trove_ctx.update_debt_amount(loan_amount)?;
        trove_ctx.update_liquidity_threshold(icr)?;
        collateral_ctx.update_collateral_amount(collateral_amount)?;
        
        // Update state
        trove_ctx.state.total_debt_amount = trove_ctx.state.total_debt_amount
            .checked_add(loan_amount)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        
        // Transfer collateral to protocol
        collateral_ctx.transfer_to_protocol(collateral_amount)?;
        
        // Note: Sorted list insertion happens in instruction handler via sorted_troves_simple::insert_trove
        // which requires Node account access not available at this layer
        
        Ok(TroveOperationResult {
            success: true,
            new_debt_amount: loan_amount,
            new_collateral_amount: collateral_amount,
            new_icr: icr,
            message: "Trove opened successfully".to_string(),
        })
    }
    
    /// Add collateral to existing trove
    pub fn add_collateral(
        trove_ctx: &mut TroveContext,
        collateral_ctx: &mut CollateralContext,
        oracle_ctx: &OracleContext,
        additional_amount: u64,
        collateral_denom: String,
    ) -> Result<TroveOperationResult> {
        // Get current trove info
        let trove_info = trove_ctx.get_trove_info()?;
        let collateral_info = collateral_ctx.get_collateral_info()?;
        
        // Get collateral price
        let price_data = oracle_ctx.get_price(&collateral_denom)?;
        oracle_ctx.validate_price(&price_data)?;
        
        // Calculate new collateral amount
        let new_collateral_amount = collateral_info.amount
            .checked_add(additional_amount)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        
        // Calculate new collateral value
        let new_collateral_value = PriceCalculator::calculate_collateral_value(
            new_collateral_amount,
            price_data.price as u64, // Convert i64 to u64
            price_data.decimal,
        )?;
        
        // Calculate new ICR
        let new_icr = PriceCalculator::calculate_collateral_ratio(
            new_collateral_value,
            trove_info.debt_amount,
        )?;
        
        // Check minimum collateral ratio (both are simple percentages)
        let minimum_ratio = trove_ctx.state.minimum_collateral_ratio as u64;
        require!(
            new_icr >= minimum_ratio,
            AerospacerProtocolError::CollateralBelowMinimum
        );
        
        // Update accounts
        collateral_ctx.update_collateral_amount(new_collateral_amount)?;
        trove_ctx.update_liquidity_threshold(new_icr)?;
        
        // Transfer collateral to protocol
        collateral_ctx.transfer_to_protocol(additional_amount)?;
        
        // Note: Sorted list operations happen in instruction handler via sorted_troves_simple
        
        Ok(TroveOperationResult {
            success: true,
            new_debt_amount: trove_info.debt_amount,
            new_collateral_amount: new_collateral_amount,
            new_icr: new_icr,
            message: "Collateral added successfully".to_string(),
        })
    }
    
    /// Remove collateral from existing trove
    pub fn remove_collateral(
        trove_ctx: &mut TroveContext,
        collateral_ctx: &mut CollateralContext,
        oracle_ctx: &OracleContext,
        remove_amount: u64,
        collateral_denom: String,
        bump: u8,
    ) -> Result<TroveOperationResult> {
        // Get current trove info
        let trove_info = trove_ctx.get_trove_info()?;
        let collateral_info = collateral_ctx.get_collateral_info()?;
        
        // Validate removal amount
        require!(
            remove_amount <= collateral_info.amount,
            AerospacerProtocolError::InvalidAmount
        );
        
        // Get collateral price
        let price_data = oracle_ctx.get_price(&collateral_denom)?;
        oracle_ctx.validate_price(&price_data)?;
        
        // Calculate new collateral amount
        let new_collateral_amount = collateral_info.amount
            .checked_sub(remove_amount)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        
        // Check minimum collateral amount
        require!(
            new_collateral_amount >= MINIMUM_COLLATERAL_AMOUNT,
            AerospacerProtocolError::CollateralBelowMinimum
        );
        
        // Calculate new collateral value
        let new_collateral_value = PriceCalculator::calculate_collateral_value(
            new_collateral_amount,
            price_data.price as u64, // Convert i64 to u64
            price_data.decimal,
        )?;
        
        // Calculate new ICR
        let new_icr = PriceCalculator::calculate_collateral_ratio(
            new_collateral_value,
            trove_info.debt_amount,
        )?;
        
        // Check minimum collateral ratio (both are simple percentages)
        let minimum_ratio = trove_ctx.state.minimum_collateral_ratio as u64;
        require!(
            new_icr >= minimum_ratio,
            AerospacerProtocolError::CollateralBelowMinimum
        );
        
        // Update accounts
        collateral_ctx.update_collateral_amount(new_collateral_amount)?;
        trove_ctx.update_liquidity_threshold(new_icr)?;
        // Transfer collateral back to user
        collateral_ctx.transfer_to_user(remove_amount, &collateral_denom, bump)?;
        
        // Note: Sorted list operations happen in instruction handler via sorted_troves_simple
        
        Ok(TroveOperationResult {
            success: true,
            new_debt_amount: trove_info.debt_amount,
            new_collateral_amount: new_collateral_amount,
            new_icr: new_icr,
            message: "Collateral removed successfully".to_string(),
        })
    }
    
    /// Borrow additional loan from existing trove
    pub fn borrow_loan(
        trove_ctx: &mut TroveContext,
        collateral_ctx: &mut CollateralContext,
        oracle_ctx: &OracleContext,
        additional_loan_amount: u64,
    ) -> Result<TroveOperationResult> {
        // Get current trove info
        let trove_info = trove_ctx.get_trove_info()?;
        let collateral_info = collateral_ctx.get_collateral_info()?;
        
        // Calculate new debt amount
        let new_debt_amount = trove_info.debt_amount
            .checked_add(additional_loan_amount)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        
        // Get collateral price
        let price_data = oracle_ctx.get_price(&collateral_info.denom)?;
        oracle_ctx.validate_price(&price_data)?;
        
        // Calculate collateral value
        let collateral_value = PriceCalculator::calculate_collateral_value(
            collateral_info.amount,
            price_data.price as u64, // Convert i64 to u64
            price_data.decimal,
        )?;
        
        // Calculate new ICR
        let new_icr = PriceCalculator::calculate_collateral_ratio(
            collateral_value,
            new_debt_amount,
        )?;
        
        // Check minimum collateral ratio
        let minimum_ratio = trove_ctx.state.minimum_collateral_ratio as u64;
        require!(
            new_icr >= minimum_ratio,
            AerospacerProtocolError::CollateralBelowMinimum
        );
        
        // Update accounts
        trove_ctx.update_debt_amount(new_debt_amount)?;
        trove_ctx.update_liquidity_threshold(new_icr)?;
        
        // Update state
        trove_ctx.state.total_debt_amount = trove_ctx.state.total_debt_amount
            .checked_add(additional_loan_amount)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        
        // Note: Sorted list operations happen in instruction handler via sorted_troves_simple
        
        Ok(TroveOperationResult {
            success: true,
            new_debt_amount: new_debt_amount,
            new_collateral_amount: collateral_info.amount,
            new_icr: new_icr,
            message: "Loan borrowed successfully".to_string(),
        })
    }
    
    /// Repay loan
    pub fn repay_loan(
        trove_ctx: &mut TroveContext,
        collateral_ctx: &mut CollateralContext,
        oracle_ctx: &OracleContext,
        repay_amount: u64,
        bump: u8,
    ) -> Result<TroveOperationResult> {
        // Get current trove info
        let trove_info = trove_ctx.get_trove_info()?;
        let collateral_info = collateral_ctx.get_collateral_info()?;
        
        // Validate repayment amount
        require!(
            repay_amount <= trove_info.debt_amount,
            AerospacerProtocolError::InvalidAmount
        );
        
        // Calculate new debt amount
        let new_debt_amount = trove_info.debt_amount
            .checked_sub(repay_amount)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        
        // Update state
        trove_ctx.state.total_debt_amount = trove_ctx.state.total_debt_amount
            .checked_sub(repay_amount)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        
        if new_debt_amount == 0 {
            // Full repayment - close trove
            trove_ctx.update_debt_amount(0)?;
            trove_ctx.update_liquidity_threshold(0)?;
            collateral_ctx.update_collateral_amount(0)?;

            // Return collateral to user
            collateral_ctx.transfer_to_user(collateral_info.amount, &collateral_info.denom, bump)?;
            
            // Note: Sorted list operations happen in instruction handler via sorted_troves_simple
            
            Ok(TroveOperationResult {
                success: true,
                new_debt_amount: 0,
                new_collateral_amount: 0,
                new_icr: 0,
                message: "Trove fully repaid and closed".to_string(),
            })
        } else {
            // Partial repayment
            // Get collateral price for ICR calculation
            let price_data = oracle_ctx.get_price(&collateral_info.denom)?;
            oracle_ctx.validate_price(&price_data)?;
            
            // Calculate collateral value
            let collateral_value = PriceCalculator::calculate_collateral_value(
                collateral_info.amount,
                price_data.price as u64, // Convert i64 to u64
                price_data.decimal,
            )?;
            
            // Calculate new ICR
            let new_icr = PriceCalculator::calculate_collateral_ratio(
                collateral_value,
                new_debt_amount,
            )?;
            
            // Update accounts
            trove_ctx.update_debt_amount(new_debt_amount)?;
            trove_ctx.update_liquidity_threshold(new_icr)?;
            
            // Note: Sorted list operations happen in instruction handler via sorted_troves_simple
            
            Ok(TroveOperationResult {
                success: true,
                new_debt_amount: new_debt_amount,
                new_collateral_amount: collateral_info.amount,
                new_icr: new_icr,
                message: "Partial repayment successful".to_string(),
            })
        }
    }
    
    /// Liquidate undercollateralized troves
    pub fn liquidate_troves(
        liquidation_ctx: &mut LiquidationContext,
        oracle_ctx: &OracleContext,
        liquidation_list: Vec<Pubkey>,
        remaining_accounts: &[AccountInfo],
        stability_pool_snapshot: &mut StabilityPoolSnapshot,
    ) -> Result<LiquidationResult> {
        let mut liquidated_count = 0u32;
        let mut total_debt_liquidated = 0u64;
        let mut total_collateral_gained = 0u64;
        let mut liquidation_gains = Vec::new();
        
        // Process each trove in the liquidation list
        for (i, user) in liquidation_list.iter().enumerate() {
            // Parse real trove data from remaining accounts
            let trove_data = parse_trove_data(user, i, remaining_accounts)?;
            
            // Validate trove is actually undercollateralized
            validate_trove_for_liquidation(&trove_data, oracle_ctx)?;
            
            // Calculate liquidation gains
            let mut trove_collateral_gain = 0u64;
            for (denom, amount) in &trove_data.collateral_amounts {
                trove_collateral_gain = trove_collateral_gain.saturating_add(*amount);
                
                // Find existing entry or add new one
                if let Some(existing) = liquidation_gains.iter_mut().find(|(d, _)| d == denom) {
                    existing.1 += *amount;
                } else {
                    liquidation_gains.push((denom.clone(), *amount));
                }
            }
            
            // Process liquidation
            liquidation_ctx.liquidate_trove(*user, trove_data.debt_amount, trove_data.collateral_amounts.clone())?;
            
            // Distribute seized collateral to stability pool stakers
            distribute_liquidation_gains_to_stakers(
                &mut liquidation_ctx.state,
                &trove_data.collateral_amounts,
                trove_data.debt_amount,
                stability_pool_snapshot,
            )?;
            
            // Update user accounts to zero (trove is closed)
            update_user_accounts_after_liquidation(user, i, remaining_accounts)?;
            
            // Update counters
            liquidated_count += 1;
            total_debt_liquidated = total_debt_liquidated.saturating_add(trove_data.debt_amount);
            total_collateral_gained = total_collateral_gained.saturating_add(trove_collateral_gain);
            
            // Note: Sorted list operations happen in instruction handler via sorted_troves_simple
            
            msg!("Liquidated trove: user={}, debt={}, collateral={}", 
                 user, trove_data.debt_amount, trove_collateral_gain);
        }
        
        Ok(LiquidationResult {
            liquidated_count,
            total_debt_liquidated,
            total_collateral_gained,
            liquidation_gains,
        })
    }
}

/// Trove data structure for liquidation
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct TroveData {
    pub user: Pubkey,
    pub debt_amount: u64,
    pub collateral_amounts: Vec<(String, u64)>,
    pub liquidity_ratio: u64,
}

/// Parse trove data from remaining accounts
fn parse_trove_data(
    user: &Pubkey,
    user_index: usize,
    remaining_accounts: &[AccountInfo],
) -> Result<TroveData> {
    let account_start = user_index * 4; // 4 accounts per user
    
    // Validate we have enough accounts
    require!(
        account_start + 3 < remaining_accounts.len(),
        AerospacerProtocolError::InvalidList
    );
    
    // Parse UserDebtAmount account
    let debt_account = &remaining_accounts[account_start];
    let debt_amount = parse_user_debt_amount(debt_account, user)?;
    
    // Parse UserCollateralAmount account
    let collateral_account = &remaining_accounts[account_start + 1];
    let collateral_amounts = parse_user_collateral_amount(collateral_account, user)?;
    
    // Parse LiquidityThreshold account
    let liquidity_account = &remaining_accounts[account_start + 2];
    let liquidity_ratio = parse_liquidity_threshold(liquidity_account, user)?;
    
    // Parse TokenAccount (for validation)
    let token_account = &remaining_accounts[account_start + 3];
    validate_token_account(token_account, user)?;
    
    Ok(TroveData {
        user: *user,
        debt_amount,
        collateral_amounts,
        liquidity_ratio,
    })
}

/// Parse UserDebtAmount from account info
fn parse_user_debt_amount(account_info: &AccountInfo, expected_user: &Pubkey) -> Result<u64> {
    // Validate account is owned by our program
    require!(
        account_info.owner == &crate::ID,
        AerospacerProtocolError::Unauthorized
    );
    
    // Validate account is mutable
    require!(
        account_info.is_writable,
        AerospacerProtocolError::Unauthorized
    );
    
    // Parse account data
    let account_data = account_info.try_borrow_data()?;
    let user_debt_amount = UserDebtAmount::try_from_slice(&account_data)?;
    
    // Validate ownership
    require!(
        user_debt_amount.owner == *expected_user,
        AerospacerProtocolError::Unauthorized
    );
    
    Ok(user_debt_amount.amount)
}

/// Parse UserCollateralAmount from account info
fn parse_user_collateral_amount(account_info: &AccountInfo, expected_user: &Pubkey) -> Result<Vec<(String, u64)>> {
    // Validate account is owned by our program
    require!(
        account_info.owner == &crate::ID,
        AerospacerProtocolError::Unauthorized
    );
    
    // Validate account is mutable
    require!(
        account_info.is_writable,
        AerospacerProtocolError::Unauthorized
    );
    
    // Parse account data
    let account_data = account_info.try_borrow_data()?;
    let user_collateral_amount = UserCollateralAmount::try_from_slice(&account_data)?;
    
    // Validate ownership
    require!(
        user_collateral_amount.owner == *expected_user,
        AerospacerProtocolError::Unauthorized
    );
    
    Ok(vec![(user_collateral_amount.denom, user_collateral_amount.amount)])
}

/// Parse LiquidityThreshold from account info
fn parse_liquidity_threshold(account_info: &AccountInfo, expected_user: &Pubkey) -> Result<u64> {
    // Validate account is owned by our program
    require!(
        account_info.owner == &crate::ID,
        AerospacerProtocolError::Unauthorized
    );
    
    // Validate account is mutable
    require!(
        account_info.is_writable,
        AerospacerProtocolError::Unauthorized
    );
    
    // Parse account data
    let account_data = account_info.try_borrow_data()?;
    let liquidity_threshold = LiquidityThreshold::try_from_slice(&account_data)?;
    
    // Validate ownership
    require!(
        liquidity_threshold.owner == *expected_user,
        AerospacerProtocolError::Unauthorized
    );
    
    Ok(liquidity_threshold.ratio)
}

/// Validate TokenAccount
fn validate_token_account(account_info: &AccountInfo, _expected_user: &Pubkey) -> Result<()> {
    // Validate account is owned by token program
    require!(
        account_info.owner == &anchor_spl::token::ID,
        AerospacerProtocolError::Unauthorized
    );
    
    Ok(())
}

/// Validate that a trove is actually undercollateralized and can be liquidated
fn validate_trove_for_liquidation(trove_data: &TroveData, oracle_ctx: &OracleContext) -> Result<()> {
    // Calculate current collateral value
    let mut total_collateral_value = 0u64;
    
    for (denom, amount) in &trove_data.collateral_amounts {
        let price_data = oracle_ctx.get_price(denom)?;
        let collateral_value = PriceCalculator::calculate_collateral_value(
            *amount,
            price_data.price as u64,
            price_data.decimal,
        )?;
        total_collateral_value = total_collateral_value.saturating_add(collateral_value);
    }
    
    // Calculate current ICR
    let current_icr = PriceCalculator::calculate_collateral_ratio(
        total_collateral_value,
        trove_data.debt_amount,
    )?;
    
    // Check if trove is undercollateralized (ICR < 110%)
    // Both current_icr and threshold are simple percentages
    let liquidation_threshold = 110u64; // 110%
    require!(
        current_icr < liquidation_threshold,
        AerospacerProtocolError::CollateralBelowMinimum // Reuse error for now
    );
    
    msg!("Trove validated for liquidation: ICR={}, threshold={}", 
         current_icr, liquidation_threshold);
    
    Ok(())
}

/// Update user accounts after liquidation (set to zero)
fn update_user_accounts_after_liquidation(
    user: &Pubkey,
    user_index: usize,
    remaining_accounts: &[AccountInfo],
) -> Result<()> {
    let account_start = user_index * 4;
    
    // Update UserDebtAmount to zero
    let debt_account = &remaining_accounts[account_start];
    let mut debt_data = debt_account.try_borrow_mut_data()?;
    let mut user_debt_amount = UserDebtAmount::try_from_slice(&debt_data)?;
    user_debt_amount.amount = 0;
    user_debt_amount.serialize(&mut &mut debt_data[..])?;
    
    // Update UserCollateralAmount to zero
    let collateral_account = &remaining_accounts[account_start + 1];
    let mut collateral_data = collateral_account.try_borrow_mut_data()?;
    let mut user_collateral_amount = UserCollateralAmount::try_from_slice(&collateral_data)?;
    user_collateral_amount.amount = 0;
    user_collateral_amount.serialize(&mut &mut collateral_data[..])?;
    
    // Update LiquidityThreshold to zero
    let liquidity_account = &remaining_accounts[account_start + 2];
    let mut liquidity_data = liquidity_account.try_borrow_mut_data()?;
    let mut liquidity_threshold = LiquidityThreshold::try_from_slice(&liquidity_data)?;
    liquidity_threshold.ratio = 0;
    liquidity_threshold.serialize(&mut &mut liquidity_data[..])?;
    
    msg!("Updated user accounts after liquidation: user={}", user);
    
    Ok(())
}

/// Distribute liquidation gains to stability pool stakers using Liquity's Product-Sum snapshot algorithm
/// 
/// This function updates global P and S factors to track:
/// - P factor: Pool depletion from debt burns (used to calculate compounded stakes)
/// - S factors: Cumulative collateral rewards per denomination (used to calculate gains)
/// 
/// The snapshot mechanism prevents post-liquidation gaming by capturing state at deposit time.
/// Actual per-user distribution is "lazy" - happens when users call withdraw_liquidation_gains.
/// 
/// # Arguments
/// * `state` - Mutable protocol state to update P factor and epoch
/// * `collateral_amounts` - Vector of (denom, amount) pairs seized from liquidation
/// * `debt_amount` - The debt amount that was liquidated (burned from pool)
/// * `stability_pool_snapshot` - StabilityPoolSnapshot account to update S factor
pub fn distribute_liquidation_gains_to_stakers(
    state: &mut StateAccount,
    collateral_amounts: &Vec<(String, u64)>,
    debt_amount: u64,
    stability_pool_snapshot: &mut StabilityPoolSnapshot,
) -> Result<()> {
    let total_stake = state.total_stake_amount;
    
    msg!("Distributing liquidation gains to stability pool (snapshot algorithm):");
    msg!("  Total stake in pool: {}", total_stake);
    msg!("  Debt liquidated: {}", debt_amount);
    msg!("  Current P factor: {}", state.p_factor);
    msg!("  Current epoch: {}", state.epoch);
    
    // If no stakers, collateral stays in vault (no distribution needed)
    if total_stake == 0 {
        msg!("  No stakers - seized collateral remains in protocol vault");
        return Ok(());
    }
    
    // STEP 1: Update P factor (tracks pool depletion from debt burn)
    // Formula: P_new = P_old × (total_stake - debt_liquidated) / total_stake
    let remaining_stake = total_stake.saturating_sub(debt_amount);
    
    if remaining_stake == 0 {
        // Pool completely depleted - start new epoch
        state.epoch = state.epoch
            .checked_add(1)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        state.p_factor = StateAccount::SCALE_FACTOR;
        state.total_stake_amount = 0;
        msg!("  Pool depleted to 0 - starting epoch {}", state.epoch);
        msg!("  P factor reset to SCALE_FACTOR");
    } else {
        // Calculate depletion ratio: (remaining_stake / total_stake)
        let depletion_ratio = (remaining_stake as u128)
            .checked_mul(StateAccount::SCALE_FACTOR)
            .ok_or(AerospacerProtocolError::OverflowError)?
            .checked_div(total_stake as u128)
            .ok_or(AerospacerProtocolError::DivideByZeroError)?;
        
        // Update P: P_new = P_old × depletion_ratio
        state.p_factor = state.p_factor
            .checked_mul(depletion_ratio)
            .ok_or(AerospacerProtocolError::OverflowError)?
            .checked_div(StateAccount::SCALE_FACTOR)
            .ok_or(AerospacerProtocolError::DivideByZeroError)?;
        
        state.total_stake_amount = remaining_stake;
        
        msg!("  Updated P factor: {} (depletion ratio: {})", state.p_factor, depletion_ratio);
        msg!("  Remaining stake: {}", remaining_stake);
    }
    
    // STEP 2: Update S factor for the collateral type (tracks cumulative rewards)
    // Formula: S_new = S_old + (collateral_seized / total_stake_before_liquidation)
    for (denom, amount) in collateral_amounts {
        // Verify the snapshot matches the collateral denomination
        require!(
            stability_pool_snapshot.denom == *denom,
            AerospacerProtocolError::InvalidAmount
        );
        
        // Calculate S increment: (collateral / total_stake) × SCALE_FACTOR
        let s_increment = (*amount as u128)
            .checked_mul(StateAccount::SCALE_FACTOR)
            .ok_or(AerospacerProtocolError::OverflowError)?
            .checked_div(total_stake as u128)
            .ok_or(AerospacerProtocolError::DivideByZeroError)?;
        
        // S_new = S_old + s_increment
        stability_pool_snapshot.s_factor = stability_pool_snapshot.s_factor
            .checked_add(s_increment)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        
        stability_pool_snapshot.total_collateral_gained = stability_pool_snapshot.total_collateral_gained
            .checked_add(*amount)
            .ok_or(AerospacerProtocolError::OverflowError)?;
        
        stability_pool_snapshot.epoch = state.epoch;
        
        msg!("  Updated S factor for {}: +{} (new S: {})", 
             denom, s_increment, stability_pool_snapshot.s_factor);
    }
    
    msg!("Liquidation gains distribution complete (snapshot algorithm)");
    
    Ok(())
}
