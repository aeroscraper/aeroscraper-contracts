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

### November 9, 2025 - Liquidation Refactoring
**Eliminated `remaining_accounts` Pattern from Liquidation Instructions:**
- Replaced manual PDA validation with Anchor's `init_if_needed` pattern for `StabilityPoolSnapshot` accounts
- Added `stability_pool_snapshot` as a regular account parameter in both `liquidate_trove` and `liquidate_troves` instructions
- Simplified `distribute_liquidation_gains_to_stakers` function by removing PDA searching logic (~60 lines eliminated)
- Auto-initialization: StabilityPoolSnapshot PDA is created automatically on first liquidation per collateral denomination
- **Benefits**: Cleaner code, type-safe accounts, automatic rent handling, eliminates manual PDA validation
- **Testing**: Code compiles successfully with `cargo check` - ready for integration testing

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