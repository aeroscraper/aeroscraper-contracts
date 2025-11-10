use anchor_lang::prelude::*;

// Exact replication of INJECTIVE error.rs
#[error_code]
pub enum AerospacerProtocolError {
    #[msg("Unauthorized")]
    Unauthorized,
    
    #[msg("Invalid reply ID")]
    InvalidReplyID,
    
    #[msg("Error while instantiating cw20 contract")]
    ReplyError,
    
    #[msg("Invalid funds")]
    InvalidFunds,
    
    #[msg("Invalid amount")]
    InvalidAmount,
    
    #[msg("Invalid address")]
    InvalidAddress,
    
    #[msg("Invalid account data")]
    InvalidAccountData,
    
    #[msg("Invalid mint")]
    InvalidMint,
    
    #[msg("Trove already exists")]
    TroveExists,
    
    #[msg("Trove does not exist")]
    TroveDoesNotExist,
    
    #[msg("Invalid collateral ratio")]
    InvalidCollateralRatio,
    
    #[msg("No liquidation collateral rewards found for sender")]
    CollateralRewardsNotFound,
    
    #[msg("Not enough liquidity for redeem")]
    NotEnoughLiquidityForRedeem,
    
    #[msg("Collateral below minimum")]
    CollateralBelowMinimum,
    
    #[msg("Insufficient collateral")]
    InsufficientCollateral,
    
    #[msg("Loan amount below minimum")]
    LoanAmountBelowMinimum,
    
    #[msg("Invalid decimal")]
    InvalidDecimal,
    
    #[msg("Invalid list")]
    InvalidList,
    
    #[msg("Divide by zero error")]
    DivideByZeroError,
    
    #[msg("Overflow error")]
    OverflowError,
    
    #[msg("Funds error")]
    FundsError,
    
    #[msg("Checked from ratio error")]
    CheckedFromRatioError,
    
    #[msg("Decimal256 range exceeded")]
    Decimal256RangeExceeded,
    
    #[msg("Conversion overflow error")]
    ConversionOverflowError,
    
    #[msg("Checked multiply fraction error")]
    CheckedMultiplyFractionError,
    
    #[msg("Math overflow error")]
    MathOverflow,
    
    #[msg("Invalid snapshot")]
    InvalidSnapshot,
    
    #[msg("Missing snapshot account in remaining_accounts")]
    MissingSnapshotAccount,
    
    #[msg("Invalid snapshot account - does not match expected PDA")]
    InvalidSnapshotAccount,
}