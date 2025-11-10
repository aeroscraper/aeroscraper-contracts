import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AerospacerFees } from "../target/types/aerospacer_fees";
import { PublicKey, Keypair } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

async function main() {
    console.log("\n🔧 Updating Aerospacer Fee Configuration on Devnet\n");
    
    // Set up provider (using devnet)
    const provider = anchor.AnchorProvider.env();
    anchor.setProvider(provider);
    
    const feesProgram = anchor.workspace.AerospacerFees as Program<AerospacerFees>;
    const admin = provider.wallet as anchor.Wallet;
    
    console.log("📝 Admin wallet:", admin.publicKey.toString());
    console.log("📝 Fees program:", feesProgram.programId.toString());
    
    // Load keypairs from files (using relative paths from project root)
    const feeAddress1Path = path.join(__dirname, "..", "keys", "fee-addresses", "fee_address_1.json");
    const feeAddress2Path = path.join(__dirname, "..", "keys", "fee-addresses", "fee_address_2.json");
    const stakingAddressPath = path.join(__dirname, "..", "keys", "staking-address", "staking_address.json");
    
    console.log("\n📂 Loading keypairs from files...");
    
    const feeAddress1Keypair = Keypair.fromSecretKey(
        new Uint8Array(JSON.parse(fs.readFileSync(feeAddress1Path, "utf-8")))
    );
    const feeAddress2Keypair = Keypair.fromSecretKey(
        new Uint8Array(JSON.parse(fs.readFileSync(feeAddress2Path, "utf-8")))
    );
    const stakingAddressKeypair = Keypair.fromSecretKey(
        new Uint8Array(JSON.parse(fs.readFileSync(stakingAddressPath, "utf-8")))
    );
    
    console.log("✅ Fee Address 1:", feeAddress1Keypair.publicKey.toString());
    console.log("✅ Fee Address 2:", feeAddress2Keypair.publicKey.toString());
    console.log("✅ Staking Address:", stakingAddressKeypair.publicKey.toString());
    
    // Derive fee state PDA
    const [feeState] = PublicKey.findProgramAddressSync(
        [Buffer.from("fee_state")],
        feesProgram.programId
    );
    
    console.log("\n📍 Fee State PDA:", feeState.toString());
    
    // Fetch current state
    console.log("\n📊 Current Fee State:");
    try {
        const currentState = await feesProgram.account.feeStateAccount.fetch(feeState);
        console.log("  Admin:", currentState.admin.toString());
        console.log("  Fee Address 1:", currentState.feeAddress1.toString());
        console.log("  Fee Address 2:", currentState.feeAddress2.toString());
        console.log("  Stake Contract Address:", currentState.stakeContractAddress.toString());
        console.log("  Is Stake Enabled:", currentState.isStakeEnabled);
        console.log("  Total Fees Collected:", currentState.totalFeesCollected.toString());
    } catch (error) {
        console.log("  ❌ Could not fetch current state:", error.message);
    }
    
    // Step 1: Update fee addresses
    console.log("\n🔄 Step 1: Updating fee addresses...");
    try {
        const tx1 = await feesProgram.methods
            .setFeeAddresses({
                feeAddress1: feeAddress1Keypair.publicKey.toString(),
                feeAddress2: feeAddress2Keypair.publicKey.toString(),
            })
            .accounts({
                admin: admin.publicKey,
                state: feeState,
            })
            .rpc();
        
        console.log("✅ Fee addresses updated!");
        console.log("   Transaction:", tx1);
    } catch (error) {
        console.log("❌ Failed to update fee addresses:", error.message);
        throw error;
    }
    
    // Step 2: Update staking contract address
    console.log("\n🔄 Step 2: Updating staking contract address...");
    try {
        const tx2 = await feesProgram.methods
            .setStakeContractAddress({
                address: stakingAddressKeypair.publicKey.toString(),
            })
            .accounts({
                admin: admin.publicKey,
                state: feeState,
            })
            .rpc();
        
        console.log("✅ Staking contract address updated!");
        console.log("   Transaction:", tx2);
    } catch (error) {
        console.log("❌ Failed to update staking contract address:", error.message);
        throw error;
    }
    
    // Fetch and display updated state
    console.log("\n📊 Updated Fee State:");
    try {
        const updatedState = await feesProgram.account.feeStateAccount.fetch(feeState);
        console.log("  Admin:", updatedState.admin.toString());
        console.log("  Fee Address 1:", updatedState.feeAddress1.toString());
        console.log("  Fee Address 2:", updatedState.feeAddress2.toString());
        console.log("  Stake Contract Address:", updatedState.stakeContractAddress.toString());
        console.log("  Is Stake Enabled:", updatedState.isStakeEnabled);
        console.log("  Total Fees Collected:", updatedState.totalFeesCollected.toString());
        
        // Verify addresses match expected values
        console.log("\n✅ Verification:");
        const match1 = updatedState.feeAddress1.toString() === feeAddress1Keypair.publicKey.toString();
        const match2 = updatedState.feeAddress2.toString() === feeAddress2Keypair.publicKey.toString();
        const match3 = updatedState.stakeContractAddress.toString() === stakingAddressKeypair.publicKey.toString();
        
        console.log("  Fee Address 1 match:", match1 ? "✅" : "❌");
        console.log("  Fee Address 2 match:", match2 ? "✅" : "❌");
        console.log("  Staking Address match:", match3 ? "✅" : "❌");
        
        if (match1 && match2 && match3) {
            console.log("\n🎉 All addresses updated successfully!");
            console.log("🚀 You can now run your tests - the fee distribution validation will pass.");
        } else {
            console.log("\n⚠️  Warning: Some addresses don't match expected values");
        }
    } catch (error) {
        console.log("  ❌ Could not fetch updated state:", error.message);
    }
}

main().catch((error) => {
    console.error("\n❌ Error:", error);
    process.exit(1);
});
