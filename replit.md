# Aerospacer Protocol - Replit Development Environment

## Overview
The Aerospacer Protocol is a decentralized lending platform on Solana, enabling Collateralized Debt Positions (CDPs), aUSD stablecoin minting, and automated liquidation. It integrates with Pyth Network for price feeds and features a robust fee distribution mechanism. The project aims to deliver a secure, efficient, and scalable on-chain lending solution within the Solana ecosystem, establishing a new primitive for decentralized finance.

## Replit Environment Setup (Completed)

**Date:** November 9, 2025  
**Status:** ✅ Ready for Development

### Installed Tools & Versions
- **Rust & Cargo:** v1.88.0
- **Solana CLI:** v1.18.26
- **Anchor CLI:** v0.31.1 (installed via avm)
- **Node.js:** v20.19.3
- **Solana BPF Tools:** v1.18.26

### Environment Configuration
- **Environment file:** `.envrc` created with PATH configuration
- **Solana Wallet:** Generated at `~/.config/solana/id.json`
- **Workflow:** "Project Info" displays environment status and quick commands

### Quick Start Commands
To use Anchor and other tools, make sure to source the environment:
```bash
source /home/runner/workspace/.envrc
```

Then you can run:
- **Build programs:** `anchor build` (first build takes 5-10 minutes)
- **Run tests:** `anchor test`
- **Quick syntax check:** `cargo check`
- **Deploy to devnet:** `anchor deploy --provider.cluster devnet`

### Important Notes
- The `.envrc` file sets up the PATH to include Anchor CLI and cargo binaries
- First build compilation takes significant time due to Solana BPF compilation
- Subsequent builds use incremental compilation and are much faster
- See the "Project Info" workflow output for additional helpful information

## Recent Changes

### November 10, 2025 - Critical Debt Accounting Fix & Sorted List Validation
**Fixed Critical Under-Collateralization Bug in borrow_loan:**
- **Issue**: borrow_loan was recording net_loan_amount as debt but minting params.loan_amount (gross), creating unbacked tokens
- **Example**: Borrowing 1000 aUSD with 5% fee minted 1000 tokens but only recorded 950 as debt → 50 unbacked tokens
- **Fix**: Changed TroveManager::borrow_loan to use params.loan_amount (gross amount) for debt accounting
- **Impact**: All minted tokens now have matching debt liability; system-wide total_debt_amount equals stablecoin supply
- **Security**: Eliminates supply/debt divergence that could enable protocol-draining exploits

**Added Neighbor Hint Validation to repay_loan:**
- Implemented ICR ordering validation when neighbor hints provided via remaining_accounts
- Mirrors borrow_loan's PDA verification and sorted list integrity checks
- Prevents sorted list corruption from malicious or incorrect repayment operations
- Production clients MUST provide neighbor hints for both borrow and repay operations

**Production Status:** ✅ **BOTH INSTRUCTIONS PRODUCTION-READY**
- Architect review confirms no accounting exploits or security issues
- Debt and supply remain in lockstep across all operations
- Sorted list invariants enforced on all ICR-changing operations

### November 10, 2025 - Production-Ready Redistribution Mechanism
**Implemented Complete Hybrid Liquidation System:**
- **Redistribution Path Added**: When stability pool is empty/insufficient, debt and collateral are redistributed to active troves (Liquity-style)
- **Schema Updates**: Added L_debt, L_collateral tracking to TotalCollateralAmount; L_snapshot fields to user accounts
- **Hybrid Liquidation**: liquidate_trove now supports 3 paths:
  1. Full stability pool coverage (Product-Sum algorithm)
  2. Partial coverage (hybrid: pool + redistribution)
  3. Empty pool (full redistribution to active troves)
- **Pending Rewards**: All trove operations (add/remove collateral, borrow/repay, close) now apply pending redistribution gains before state changes
- **open_trove Fixed**: New troves capture current global L factors to prevent unearned retroactive rewards from past redistributions
- **Status**: ✅ **PRODUCTION-READY FOR FRESH DEPLOYMENTS**
  - Both liquidate_trove and open_trove are complete and production-ready
  - ⚠️ **NOT COMPATIBLE WITH EXISTING DEPLOYMENTS** - Schema changes require account migration
  - Fresh deployments work correctly
  - Existing on-chain accounts will fail deserialization (added 16-32 bytes per account)
  - Migration required before deploying to networks with existing data

### November 9, 2025 - Liquidation Refactoring
**Eliminated `remaining_accounts` Pattern from Liquidation Instructions:**
- Replaced manual PDA validation with Anchor's `init_if_needed` pattern for `StabilityPoolSnapshot` accounts
- Added `stability_pool_snapshot` as a regular account parameter in both `liquidate_trove` and `liquidate_troves` instructions
- Simplified `distribute_liquidation_gains_to_stakers` function by removing PDA searching logic (~60 lines eliminated)
- Auto-initialization: StabilityPoolSnapshot PDA is created automatically on first liquidation per collateral denomination
- **Benefits**: Cleaner code, type-safe accounts, automatic rent handling, eliminates manual PDA validation

## User Preferences
*This section will be updated as you work with the project*

## System Architecture

**Core Programs:**
The project utilizes Anchor v0.31.1 in Rust and comprises three main Solana smart contract programs:
1.  **aerospacer-protocol**: Manages core lending logic, including CDPs, stablecoin minting, and liquidation.
2.  **aerospacer-oracle**: Handles price feed management, primarily integrating with the Pyth Network.
3.  **aerospacer-fees**: Manages fee collection and distribution.

**UI/UX Decisions:**
The design emphasizes transparent and auditable on-chain interactions, ensuring all state changes and operations are publicly verifiable on the Solana blockchain.

**Technical Implementations & Feature Specifications:**
*   **Collateralized Debt Positions (CDPs)**: Users can lock collateral to mint aUSD stablecoins.
*   **Stablecoin (aUSD) Minting**: Supports the minting of its native stablecoin, aUSD.
*   **Automated Liquidation System**: Ensures protocol solvency by liquidating undercollateralized positions.
*   **Stability Pool**: Implements Liquity's Product-Sum algorithm for reward distribution.
*   **Fee Distribution Mechanism**: A dual-mode system for distributing fees to the stability pool or splitting them between specified addresses, with comprehensive validation for exact fee amounts and distribution modes.
*   **Oracle Integration**: Uses Pyth Network for real-time price feeds with dynamic collateral discovery via CPI.
*   **Cross-Program Communication (CPI)**: Utilizes CPI for secure and atomic interactions between sub-programs.
*   **SPL Token Integration**: Full support for Solana Program Library (SPL) tokens for collateral and stablecoin operations.
*   **Sorted Troves (Off-Chain Architecture)**: Employs off-chain sorting with on-chain ICR validation. The client fetches all troves via RPC, sorts by ICR, and passes only neighbor hints for validation, eliminating on-chain linked list storage for unlimited scalability. Includes critical PDA verification to prevent fake account injection attacks.
*   **Individual Collateral Ratio (ICR)**: Real-time ICR calculations are implemented across the protocol, supporting multi-collateral types and ensuring solvency checks.
*   **Redemption System**: Accepts pre-sorted trove lists from the client, validates ICR ordering, and supports both full and partial redemptions.

**System Design Choices:**
*   **Anchor Framework**: Utilized for Solana smart contract development.
*   **Rust & TypeScript**: Rust for on-chain programs and TypeScript for off-chain tests and interactions.
*   **Modular Architecture**: Separation of concerns into distinct programs (`protocol`, `oracle`, `fees`).
*   **Security Features**: Includes safe math operations, access control, input validation, atomic state consistency, PDA validation, and optimization for Solana BPF stack limits.
*   **Two-Instruction Architecture for Liquidation**: Separates data traversal from execution to optimize account ordering.
*   **Vault Signing Architecture**: All PDA vault authorities correctly sign CPIs using `invoke_signed`.
*   **BPF Stack Optimization**: Uses `UncheckedAccount` pattern to mitigate Solana BPF stack limits.

## External Dependencies

*   **Solana Blockchain**: The foundational blockchain layer.
*   **Anchor Framework**: Solana smart contract development framework.
*   **Pyth Network**: Used by the `aerospacer-oracle` program for real-time price feeds.
*   **Solana Program Library (SPL) Tokens**: Integrated for token operations within the protocol.
*   **Node.js & npm**: For running TypeScript tests and managing project dependencies.