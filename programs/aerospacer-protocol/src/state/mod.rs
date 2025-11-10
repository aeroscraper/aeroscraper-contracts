use anchor_lang::prelude::*;

// Exact replication of INJECTIVE state.rs
// Main state account (equivalent to INJECTIVE's ADMIN, ORACLE_HELPER_ADDR, FEE_DISTRIBUTOR_ADDR, MINIMUM_COLLATERAL_RATIO, PROTOCOL_FEE, STABLE_COIN_ADDR, TOTAL_DEBT_AMOUNT, TOTAL_STAKE_AMOUNT)
#[account]
pub struct StateAccount {
    pub admin: Pubkey,
    pub oracle_helper_addr: Pubkey,          // Oracle program ID
    pub oracle_state_addr: Pubkey,           // Oracle state account address  
    pub fee_distributor_addr: Pubkey,        // aerospacer-fees program ID
    pub fee_state_addr: Pubkey,              // aerospacer-fees state account address
    pub minimum_collateral_ratio: u64,
    pub protocol_fee: u8,
    pub stable_coin_addr: Pubkey,
    pub stable_coin_code_id: u64,
    pub total_debt_amount: u64, // Equivalent to Uint256
    pub total_stake_amount: u64, // Equivalent to Uint256
    
    // Stability Pool Snapshot Variables (Liquity Product-Sum Algorithm)
    pub p_factor: u128,  // Product/depletion factor - tracks cumulative pool depletion from debt burns (starts at SCALE_FACTOR)
    pub epoch: u64,      // Current epoch - increments when pool is completely depleted to 0
}

impl StateAccount {
    pub const LEN: usize = 8 + 32 + 32 + 32 + 32 + 32 + 8 + 1 + 32 + 8 + 8 + 8 + 16 + 8; // Added oracle_state_addr + fee_state_addr + stable_coin_code_id, minimum_collateral_ratio now u64
    
    // Scale factor for precision in P/S calculations (10^18, same as Liquity)
    pub const SCALE_FACTOR: u128 = 1_000_000_000_000_000_000;
    
    pub fn seeds() -> [&'static [u8]; 1] {
        [b"state"]
    }
}

// User debt amount (equivalent to INJECTIVE's USER_DEBT_AMOUNT: Map<Addr, Uint256>)
#[account]
pub struct UserDebtAmount {
    pub owner: Pubkey,
    pub amount: u64,
    pub l_debt_snapshot: u128,
}

impl UserDebtAmount {
    pub const LEN: usize = 8 + 32 + 8 + 16;
    pub fn seeds(owner: &Pubkey) -> [&[u8]; 2] {
        [b"user_debt_amount", owner.as_ref()]
    }
}

// User collateral amount (equivalent to INJECTIVE's USER_COLLATERAL_AMOUNT: Map<(Addr, String), Uint256>)
#[account]
pub struct UserCollateralAmount {
    pub owner: Pubkey,
    pub denom: String,
    pub amount: u64,
    pub l_collateral_snapshot: u128,
}

impl UserCollateralAmount {
    pub const LEN: usize = 8 + 32 + 32 + 8 + 16;
    pub fn seeds<'a>(owner: &'a Pubkey, denom: &'a str) -> [&'a [u8]; 3] {
        [b"user_collateral_amount", owner.as_ref(), denom.as_bytes()]
    }
}

// User stake amount with snapshots (equivalent to INJECTIVE's USER_STAKE_AMOUNT: SnapshotMap<Addr, Uint256>)
#[account]
pub struct UserStakeAmount {
    pub owner: Pubkey,
    pub amount: u64,                    // Current staked amount
    pub p_snapshot: u128,               // User's P factor snapshot at last deposit (for compounded stake calculation)
    pub epoch_snapshot: u64,            // Epoch when user last deposited (for epoch transition tracking)
    pub last_update_block: u64,         // Last block when stake was updated
}

impl UserStakeAmount {
    pub const LEN: usize = 8 + 32 + 8 + 16 + 8 + 8; // Added p_snapshot(16) + epoch_snapshot(8) + last_update_block(8)
    pub fn seeds(owner: &Pubkey) -> [&[u8]; 2] {
        [b"user_stake_amount", owner.as_ref()]
    }
}

// Liquidity threshold (equivalent to INJECTIVE's LIQUIDITY_THRESHOLD: Map<Addr, Decimal256>)
#[account]
pub struct LiquidityThreshold {
    pub owner: Pubkey,
    pub ratio: u64, // Equivalent to Decimal256
}

impl LiquidityThreshold {
    pub const LEN: usize = 8 + 32 + 8;
    pub fn seeds(owner: &Pubkey) -> [&[u8]; 2] {
        [b"liquidity_threshold", owner.as_ref()]
    }
}

// Total collateral amount (equivalent to INJECTIVE's TOTAL_COLLATERAL_AMOUNT: Map<String, Uint256>)
#[account]
pub struct TotalCollateralAmount {
    pub denom: String,
    pub amount: u64,
    pub l_collateral: u128,
    pub l_debt: u128,
}

impl TotalCollateralAmount {
    pub const LEN: usize = 8 + 32 + 8 + 16 + 16;
    pub fn seeds(denom: &str) -> [&[u8]; 2] {
        [b"total_collateral_amount", denom.as_bytes()]
    }
}

// User liquidation collateral gain (equivalent to INJECTIVE's USER_LIQUIDATION_COLLATERAL_GAIN: Map<(Addr, u64), bool>)
#[account]
pub struct UserLiquidationCollateralGain {
    pub user: Pubkey,
    pub block_height: u64,
    pub claimed: bool,
}

impl UserLiquidationCollateralGain {
    pub const LEN: usize = 8 + 32 + 8 + 1;
    pub fn seeds(user: &Pubkey, block_height: u64) -> [&[u8]; 3] {
        let block_height_bytes = Box::leak(block_height.to_le_bytes().to_vec().into_boxed_slice());
        [b"user_liq_gain", user.as_ref(), block_height_bytes]
    }
}

// Total liquidation collateral gain (equivalent to INJECTIVE's TOTAL_LIQUIDATION_COLLATERAL_GAIN: Map<(u64, String), Uint256>)
#[account]
pub struct TotalLiquidationCollateralGain {
    pub block_height: u64,
    pub denom: String,
    pub amount: u64, // Equivalent to Uint256
}

impl TotalLiquidationCollateralGain {
    pub const LEN: usize = 8 + 8 + 32 + 8; // String length needs to be considered
    pub fn seeds(block_height: u64, denom: &str) -> [&[u8]; 3] {
        let block_height_bytes = Box::leak(block_height.to_le_bytes().to_vec().into_boxed_slice());
        [b"total_liq_gain", block_height_bytes, denom.as_bytes()]
    }
}

// REMOVED: Node and SortedTrovesState structs
// NEW ARCHITECTURE: Off-chain sorting with on-chain validation
// - Client fetches all troves via RPC (no size limits)
// - Client sorts by ICR off-chain
// - Client passes 2-3 neighbor hints via remainingAccounts (~6-9 accounts)
// - Contract validates ICR ordering without storing linked list

// Stability Pool Snapshot - tracks cumulative collateral rewards per denomination
// This is the global "S" factor from Liquity's Product-Sum algorithm
#[account]
pub struct StabilityPoolSnapshot {
    pub denom: String,                  // Collateral denomination (e.g., "SOL", "USDC")
    pub s_factor: u128,                 // Sum: cumulative collateral-per-unit-staked (scaled by SCALE_FACTOR)
    pub total_collateral_gained: u64,  // Total collateral seized and distributed this epoch
    pub epoch: u64,                     // Current epoch (resets when pool depletes to 0)
}

impl StabilityPoolSnapshot {
    pub const LEN: usize = 8 + 32 + 16 + 8 + 8; // denom(32) + s_factor(16) + total(8) + epoch(8)
    
    pub fn seeds(denom: &str) -> [&[u8]; 2] {
        [b"stability_pool_snapshot", denom.as_bytes()]
    }
}

// User Collateral Snapshot - tracks user's S snapshot for each collateral type
// Captures the S value when user stakes, enabling gain calculation on withdrawal
#[account]
pub struct UserCollateralSnapshot {
    pub owner: Pubkey,
    pub denom: String,
    pub s_snapshot: u128,               // User's S factor snapshot at last deposit
    pub pending_collateral_gain: u64,  // Unclaimed gains from previous epochs
}

impl UserCollateralSnapshot {
    pub const LEN: usize = 8 + 32 + 32 + 16 + 8; // owner(32) + denom(32) + s_snapshot(16) + pending(8)
    
    pub fn seeds<'a>(owner: &'a Pubkey, denom: &'a str) -> [&'a [u8]; 3] {
        [b"user_collateral_snapshot", owner.as_ref(), denom.as_bytes()]
    }
}

// Constants to match INJECTIVE exactly
pub const MINIMUM_LOAN_AMOUNT: u64 = 1_000_000_000_000_000; // 0.001 aUSD with 18 decimals
pub const MINIMUM_COLLATERAL_AMOUNT: u64 = 1_000_000; // 0.001 SOL with 9 decimals
pub const DEFAULT_MINIMUM_COLLATERAL_RATIO: u64 = 115_000_000; // 115% in micro-percent (115 * 1_000_000)
pub const DEFAULT_PROTOCOL_FEE: u8 = 5; // 5%

// Decimal fractions to match INJECTIVE
pub const DECIMAL_FRACTION_6: u128 = 1_000_000;
pub const DECIMAL_FRACTION_18: u128 = 1_000_000_000_000_000_000;