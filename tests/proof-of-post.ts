import { Connection, PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { ProofOfPost } from "../target/types/proof_of_post";
import { assert } from "chai";
import keccak256 from "keccak256";
import fetch from "node-fetch";

const PROGRAM_ID = new PublicKey(
  "5MQLTq2D5ZhUAc6TDoAMXfnMeA32bo5DUxYco5LDMKAA"
);
const POST_VERIFICATION_IMAGE_ID =
  "e4836295bfe6bd17f8907d071535ff03fdf24aa6bc562792833b17dfc44703bb";
const BONSOL_PROGRAM_ID = new PublicKey(
  "BoNsHRcyLLNdtnoDf8hiCNZpyehMC4FDMxs6NTxFi3ew"
);

class ProofOfPostClient {
  private connection: Connection;
  private payer: Keypair;
  private provider: anchor.AnchorProvider;
  private wallet;
  private program: Program<ProofOfPost>;

  constructor(connection: Connection, payer: Keypair) {
    this.connection = connection;
    this.payer = payer;
    anchor.setProvider(anchor.AnchorProvider.env());
    this.provider = anchor.AnchorProvider.env();
    this.wallet = anchor.AnchorProvider.env().wallet;
    this.program = anchor.workspace.ProofOfPost as Program<ProofOfPost>;
  }

  // Get PDA addresses
  getPostProofConfigPDA(creator: PublicKey, seeds: string): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("postproofconfig"), creator.toBuffer(), Buffer.from(seeds)],
      this.program.programId
    );
  }

  getPostVerificationLogPDA(verifier: PublicKey, configPDA: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      // [Buffer.from("postverificationlog"), verifier.toBuffer(), Buffer.from(postUrl)],
      [Buffer.from("postverificationlog"), verifier.toBuffer(), configPDA.toBuffer()],
      this.program.programId
    );
  }

  // Get PDA for execution tracker (Bonsol specific)
  getExecutionTrackerPDA(executionIdBuffer: Buffer): [PublicKey, number] {
    return PublicKey.findProgramAddressSync([executionIdBuffer], PROGRAM_ID);
  }

  // Convert web URL or AT-URI to Bluesky API URL
  async convertToApiUrl(postId: string): Promise<string> {
    // If already an API URL, return as-is
    if (postId.startsWith("https://public.api.bsky.app/") || 
        postId.startsWith("https://api.bsky.app/")) {
      return postId;
    }

    // If it's an AT-URI, convert directly
    if (postId.startsWith("at://")) {
      return `https://public.api.bsky.app/xrpc/app.bsky.feed.getPosts?uris=${postId}`;
    }

    // If it's a web URL, we need to resolve handle to DID
    if (postId.startsWith("https://bsky.app/profile/")) {
      const parts = postId.split('/');
      if (parts.length >= 7) {
        const handle = parts[4];
        const rkey = parts[6];

        // Resolve handle to DID
        const resolveUrl = `https://public.api.bsky.app/xrpc/com.atproto.identity.resolveHandle?handle=${handle}`;
        const response = await fetch(resolveUrl);
        const data = await response.json() as { did: string };
        const did = data.did;

        // Construct AT-URI and then API URL
        const atUri = `at://${did}/app.bsky.feed.post/${rkey}`;
        return `https://public.api.bsky.app/xrpc/app.bsky.feed.getPosts?uris=${atUri}`;
      }
    }

    throw new Error("Invalid post ID format");
  }

  // Fetch URL and get byte size of response
  async getUrlResponseSize(url: string): Promise<number> {
    try {
      const response = await fetch(url);
      const text = await response.text();
      const bytes = Buffer.from(text, 'utf-8');
      console.log(`📏 URL response size: ${bytes.length} bytes`);
      return bytes.length;
    } catch (error) {
      console.error("Failed to fetch URL:", error);
      throw error;
    }
  }

  // Create a new PostProofConfig
  async createConfig(
    seeds: string,
    keywords: string[],
    rewardAmount: number,
    maxClaimers: number
  ): Promise<void> {
    console.log("🔧 Creating PostProofConfig...");

    const [configPDA] = this.getPostProofConfigPDA(this.payer.publicKey, seeds);

    try {
      const tx = await this.program.methods
        .createConfig({
          seeds,
          keywords,
          rewardAmount: new anchor.BN(rewardAmount),
          maxClaimers: new anchor.BN(maxClaimers),
        })
        .accounts({
          // postProofConfig: configPDA,
          creator: this.payer.publicKey,
          // systemProgram: SystemProgram.programId,
        })
        .signers([this.payer])
        .rpc();

      console.log("✅ Config created. Transaction:", tx);
      console.log("📍 Config PDA:", configPDA.toString());

      // Fetch and display config
      const config = await this.program.account.postProofConfig.fetch(configPDA);
      console.log("📋 Config details:");
      console.log("   Seeds:", config.seeds);
      console.log("   Keywords:", config.keywords);
      console.log("   Reward:", config.rewardAmount.toString(), "lamports");
      console.log("   Max claimers:", config.maxClaimers.toString());
      console.log("   Active:", config.active);
    } catch (error) {
      console.error("❌ Create config failed:", error);
      throw error;
    }
  }

  // Update an existing config
  async updateConfig(
    seeds: string,
    updates: {
      active?: boolean;
      maxClaimers?: number;
      rewardAmount?: number;
    }
  ): Promise<void> {
    console.log("🔄 Updating PostProofConfig...");

    const [configPDA] = this.getPostProofConfigPDA(this.payer.publicKey, seeds);

    try {
      const tx = await this.program.methods
        .updateConfig({
          active: updates.active !== undefined ? updates.active : null,
          maxClaimers: updates.maxClaimers !== undefined ? new anchor.BN(updates.maxClaimers) : null,
          rewardAmount: updates.rewardAmount !== undefined ? new anchor.BN(updates.rewardAmount) : null,
        })
        .accounts({
          postProofConfig: configPDA,
          // creator: this.payer.publicKey,
        })
        .signers([this.payer])
        .rpc();

      console.log("✅ Config updated. Transaction:", tx);
    } catch (error) {
      console.error("❌ Update config failed:", error);
      throw error;
    }
  }

  // Verify a post
  async verifyPost(
    configPDA: PublicKey,
    postId: string
  ): Promise<void> {
    console.log("🔍 Verifying post...");
    console.log("📝 Post ID:", postId);

    // Convert post ID to API URL
    const apiUrl = await this.convertToApiUrl(postId);
    console.log("🌐 API URL:", apiUrl);

    // Get response size
    const postSize = await this.getUrlResponseSize(apiUrl);

    // Create unique request ID
    const currentReqId = `verify-${Date.now()}`;
    const executionIdBuffer = Buffer.from(currentReqId);

    // Get PDAs (Bonsol specific)
    const [requesterAccount] = this.getExecutionTrackerPDA(executionIdBuffer);

    const hash = keccak256(Buffer.from(POST_VERIFICATION_IMAGE_ID));
    const [imageIdAccount] = PublicKey.findProgramAddressSync(
      [Buffer.from("deployment"), hash],
      BONSOL_PROGRAM_ID
    );

    const [executionAccount] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("execution"),
        this.payer.publicKey.toBuffer(),
        executionIdBuffer,
      ],
      BONSOL_PROGRAM_ID
    );

    const [verificationLogPDA] = this.getPostVerificationLogPDA(
      this.payer.publicKey,
      configPDA
    );

    console.log("🔑 Requester Account:", requesterAccount.toBase58());
    console.log("🔑 Execution Account:", executionAccount.toBase58());
    console.log("🔑 Image ID Account:", imageIdAccount.toBase58());
    console.log("🔑 Verification Log PDA:", verificationLogPDA.toBase58());

    try {
      const tx = await this.program.methods
        .verifyPost({
          currentReqId,
          postUrl: apiUrl,
          postSize: new anchor.BN(postSize),
          tip: new anchor.BN(100000), // 0.0001 SOL tip
        })
        .accounts({
          postProofConfig: configPDA,
          // postVerificationLog: verificationLogPDA,
          verifier: this.payer.publicKey,
          // bonsolProgram: BONSOL_PROGRAM_ID,
          // requester: requesterAccount,
          executionRequest: executionAccount,
          deploymentAccount: imageIdAccount,
          // postProofProgram: PROGRAM_ID,
          // systemProgram: SystemProgram.programId,
        })
        .signers([this.payer])
        .rpc();

      console.log("✅ Verification request submitted. Transaction:", tx);
      console.log("⏳ Waiting for ZK proof (this takes 4-5 minutes)...");
      console.log("💡 Check verification log later with getVerificationStatus()");
    } catch (error) {
      console.error("❌ Verify post failed:", error);
      throw error;
    }
  }

  // Get config details
  async getConfigDetails(creator: PublicKey, seeds: string): Promise<void> {
    console.log("📋 Fetching config details...");

    const [configPDA] = this.getPostProofConfigPDA(creator, seeds);

    try {
      const config = await this.program.account.postProofConfig.fetch(configPDA);
      console.log("📋 Config Details:");
      console.log("   Creator:", config.creator.toString());
      console.log("   Seeds:", config.seeds);
      console.log("   Keywords:", config.keywords);
      console.log("   Claimers Count:", config.claimersCount.toString());
      console.log("   Reward Amount:", config.rewardAmount.toString(), "lamports");
      console.log("   Max Claimers:", config.maxClaimers.toString());
      console.log("   Active:", config.active);
      console.log("   Created Slot:", config.createdSlot.toString());
    } catch (error) {
      console.error("❌ Config not found:", error);
    }
  }

  // Helper function to load keypair from file
  static loadKeypairFromFile(filePath: string): Keypair {
    const secretKeyString = fs.readFileSync(filePath, "utf8");
    const secretKey = Uint8Array.from(JSON.parse(secretKeyString));
    return Keypair.fromSecretKey(secretKey);
  }

  // Helper function to airdrop SOL for testing
  async airdropSol(lamports: number = 2000000000): Promise<void> {
    try {
      const airdropSignature = await this.connection.requestAirdrop(
        this.payer.publicKey,
        lamports
      );
      await this.connection.confirmTransaction(airdropSignature);
      console.log(
        `💰 Airdropped ${lamports / 1000000000} SOL to ${this.payer.publicKey.toString()}`
      );
    } catch (error) {
      console.error("❌ Airdrop failed:", error);
    }
  }
}

// Configuration
const RPC_URL = process.env.RPC_URL || "http://localhost:8899";
const KEYPAIR_PATH =
  process.env.KEYPAIR_PATH ||
  path.join(process.env.HOME || "", ".config/solana/id.json");

// Initialize connection and client
let connection: Connection;
let client: ProofOfPostClient;

// Setup function
const setup = async (payer: Keypair): Promise<void> => {
  console.log("🔧 Setting up connection and client...");

  connection = new Connection(RPC_URL, "confirmed");

  try {
    if (fs.existsSync(KEYPAIR_PATH)) {
      payer = ProofOfPostClient.loadKeypairFromFile(KEYPAIR_PATH);
      console.log("🔑 Loaded keypair from file:", KEYPAIR_PATH);
    } else {
      payer = Keypair.generate();
      console.log("🔑 Generated new keypair for testing");
    }
  } catch (error) {
    console.log("⚠️ Could not load keypair from file, generating new one");
    payer = Keypair.generate();
  }

  client = new ProofOfPostClient(connection, payer);

  console.log("💼 Payer public key:", payer.publicKey.toString());

  // Check balance and airdrop if needed
  const balance = await connection.getBalance(payer.publicKey);
  console.log("💰 Current balance:", balance / 1000000000, "SOL");

  if (balance < 1000000000) {
    await client.airdropSol();
  }
};

// Main execution function
const main = async (): Promise<void> => {
  console.log("🌟 Starting Proof of Post Demo\n");

  try {
    let payer: Keypair;
    payer = ProofOfPostClient.loadKeypairFromFile(KEYPAIR_PATH);
    console.log("🔑 Loaded keypair from file:", KEYPAIR_PATH);

    // Setup connection and client
    await setup(payer);

    console.log("\n" + "=".repeat(60));

    // Create config
    console.log("\n📝 Step 1: Create Config");
    // await client.createConfig(
    //   "campaign-v1",
    //   ["some", "sushi", "reading"],
    //   10000, // 0.001 SOL reward
    //   100 // max 100 claimers
    // );

    console.log("\n" + "=".repeat(60));

    // Test URL conversion and size detection
    console.log("\n🔗 Step 2: Test URL Conversion");
    const testPostUrl = "https://bsky.app/profile/redderbeanpaste.bsky.social/post/3m2kn5qv7322b";
    const apiUrl = await client.convertToApiUrl(testPostUrl);
    console.log("✅ Converted URL:", apiUrl);
    
    const size = await client.getUrlResponseSize(apiUrl);
    console.log("✅ Response size:", size, "bytes");

    console.log("\n" + "=".repeat(60));

    // Verify a post
    console.log("\n🔍 Step 3: Verify Post");
    const [configPDA] = client.getPostProofConfigPDA(payer.publicKey, "campaign-v1");
    await client.verifyPost(configPDA, testPostUrl);

    console.log("\n" + "=".repeat(60));

    // Get config details
    console.log("\n📊 Step 4: Get Config Details");
    await client.getConfigDetails(payer.publicKey, "campaign-v1");

    console.log("\n" + "=".repeat(60));

    console.log("\n✨ Demo completed!");
    console.log("💡 Check verification status later with:");
    console.log(`   await client.getVerificationStatus(verifier, "${apiUrl}");`);

  } catch (error) {
    console.error("\n💥 Demo failed:", error);
    throw error;
  }
};

// Execute main function
main().catch((err) => {
  console.error(err);
  process.exit(1);
});

// Export for use as module
export { ProofOfPostClient };
