# ICR/MCR Calculation Fix - Deployment Guide

## Overview
This guide explains how to build and deploy the fixes for the CollateralBelowMinimum error caused by incorrect ICR calculations.

## What Was Fixed

### Root Cause
The oracle was returning only the Pyth price exponent (8) as the decimal, causing collateral values to be calculated with wrong precision. This made on-chain ICR calculations fail even when the ICR was well above the 115% minimum.

### Changes Made

1. **Oracle Decimal Adjustment** (`programs/aerospacer-oracle/src/instructions/get_price.rs`)
   - Now calculates: `adjusted_decimal = (token_decimals + price_exponent) - 6`
   - For SOL: `9 + 8 - 6 = 11` (instead of `8`)
   - Validates that `total_precision >= 6` to prevent underflow
   - Ensures collateral values are in micro-USD (10^-6 USD) units

2. **Debug Logging** (multiple files)
   - Added comprehensive logging throughout the ICR/MCR calculation pipeline
   - Logs show: oracle response, collateral value calculation, ICR calculation, MCR check
   - Helps diagnose any future issues

3. **Multi-Collateral Alignment** (`programs/aerospacer-protocol/src/utils/mod.rs`)
   - Updated hardcoded decimals to match oracle output
   - SOL: 11, USDC: 8, INJ: 20, ATOM: 8

## Build and Deploy Instructions

### Prerequisites
- Solana CLI tools installed (`solana-cli >= 1.14`)
- Anchor CLI installed (`anchor-cli >= 0.31.1`)
- Wallet with devnet SOL for deployment fees
- Access to the deployer wallet (`~/.config/solana/id.json`)

### Step 1: Build the Programs

```bash
# Navigate to project root
cd /path/to/aerospacer-protocol

# Build all programs (this will take a few minutes)
anchor build

# Verify builds succeeded
ls -la target/deploy/
# You should see:
# - aerospacer_oracle.so
# - aerospacer_protocol.so
# - aerospacer_fees.so
```

### Step 2: Deploy to Devnet

```bash
# Set cluster to devnet
solana config set --url devnet

# Verify you're on devnet
solana config get
# Should show: RPC URL: https://api.devnet.solana.com

# Deploy oracle first (it's a dependency)
anchor deploy --program-name aerospacer-oracle --provider.cluster devnet

# Deploy protocol
anchor deploy --program-name aerospacer-protocol --provider.cluster devnet

# Fees program doesn't need redeployment (no changes)
```

### Step 3: Verify Deployment

```bash
# Check program info
solana program show 8Fu4YnUkfmrGQ3PTVoPfsAGjQ6NistGsiKpBEkPhzA2K  # oracle
solana program show HQbV7SKnWuWPHEci5eejsnJG7qwYuQkGzJHJ6nhLZhxk  # protocol

# Verify last deployed slot matches current deployment
```

### Step 4: Test the Fix

After deployment, test with your existing trove scenario:

**Expected Behavior:**
- Collateral: 0.89 SOL (~890,000,000 lamports)
- Debt: 0.0395 aUSD existing + 0.1 aUSD borrowing = 0.1495 aUSD total
- SOL Price: ~$163
- Expected ICR: ~83,235% (well above 115% MCR)
- **Result:** ✅ Transaction should succeed

**Debug Logs to Check:**
The transaction logs should now show:
```
Token decimal: 9
Price exponent: 8
Adjusted decimal (for micro-USD): 11
collateral_value: 124437497 (micro-USD)
scaled_collateral_value: 124437497000000000000
ratio (percentage): 83235
✅ ICR 83235 >= MCR 115 → Check passed
```

### Step 5: Monitor Logs

When you attempt a borrow transaction from the frontend, check the Solana Explorer for detailed logs:

1. Open https://explorer.solana.com/?cluster=devnet
2. Search for your transaction signature
3. View "Program Instruction Logs"
4. Look for the new debug messages prefixed with 🔍, 📊, and ✅/❌

## Troubleshooting

### If Build Fails
- Ensure Rust toolchain is up to date: `rustup update`
- Clean and rebuild: `anchor clean && anchor build`
- Check Anchor version: `anchor --version` (should be 0.31.1)

### If Deploy Fails
- Check wallet has enough SOL: `solana balance`
- Verify you have upgrade authority for the programs
- Try increasing compute budget if hitting limits

### If Test Still Fails
1. Check the logs for the exact ICR values calculated on-chain
2. Verify oracle is returning `adjusted_decimal: 11` for SOL
3. Ensure you're testing on devnet (not localnet)
4. Check that Pyth price feed is returning valid data

## Expected Improvement

**Before Fix:**
- Oracle returned decimal: 8
- Collateral value: 124,437,497,341,100 (wrong units)
- ICR calculation: Failed or incorrect
- Result: ❌ CollateralBelowMinimum error

**After Fix:**
- Oracle returns adjusted_decimal: 11
- Collateral value: 124,437,497 micro-USD ✓
- ICR calculation: ~83,235% ✓
- Result: ✅ Transaction succeeds

## Files Changed

1. `programs/aerospacer-oracle/src/instructions/get_price.rs`
2. `programs/aerospacer-protocol/src/oracle.rs`
3. `programs/aerospacer-protocol/src/trove_management.rs`
4. `programs/aerospacer-protocol/src/utils/mod.rs`

All changes have been reviewed and approved by the architect agent.

## Questions or Issues?

If you encounter any issues during deployment or testing, check:
1. Solana Explorer logs for detailed error messages
2. The new debug logging output for exact calculation values
3. Oracle state to ensure SOL collateral is properly configured with decimal=9
