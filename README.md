**About Proof of Post**

Proof of Post demonstrates how to use Bonsol to verify social media content on-chain. This example shows how campaigns can reward users for creating posts with specific keywords, verified through zero-knowledge proofs.

**NOTE:** 
This is a demo on devnet. Verification takes approximately 40 seconds - 1 minute (after latest Bonsol upgrades with v0.6.0 release). Check your wallet after this time to see if you received the reward.

**How It Works**

When you submit a BlueSky post URL for verification:
1. Your wallet sends a transaction to the Proof of Post program with the post URL
2. The program calls Bonsol to verify the post content
3. Bonsol's ZK application fetches the post from BlueSky API and checks for required keywords
4. Bonsol generates a zero-knowledge proof of the verification
5. If valid, the program automatically transfers SOL reward to your wallet

**Key Features**
* Verifiable social media content without exposing private data
* Automated reward distribution on-chain
* Keyword-based content verification
* Rate limiting to prevent spam
* Campaign management with claim limits

**Use Cases**

This pattern can be extended to build:
* Marketing campaigns with verifiable engagement
* Content creator reward programs
* Social proof for on-chain reputation systems
* Decentralized social media verification

**Outcome:** 

The zero-knowledge proof ensures that post content is verified without exposing the entire post data on-chain, maintaining privacy while enabling trustless verification.

**Screenshots:**
<img width="803" height="506" alt="image" src="https://github.com/user-attachments/assets/fcfc4c4d-7c59-4c3c-9ccf-e464115a4968" />
<img width="818" height="548" alt="image" src="https://github.com/user-attachments/assets/e9782a96-ac56-49f1-9018-7d05d78515a7" />
