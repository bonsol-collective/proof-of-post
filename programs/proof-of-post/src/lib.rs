use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar;
use anchor_lang::solana_program::sysvar::Sysvar;
use bonsol_anchor_interface::instructions::{
    execute_v1, CallbackConfig, ExecutionConfig, InputRef,
};
use bonsol_anchor_interface::Bonsol;

use anchor_lang::solana_program::program::invoke;
use bonsol_anchor_interface::callback::handle_callback;

// Change this ID and make your own if you want to deploy to devnet
declare_id!("5MQLTq2D5ZhUAc6TDoAMXfnMeA32bo5DUxYco5LDMKAA");
const POST_VERIFICATION_IMAGE_ID: &str =
    "4de2a43da6e788efef9837b71e055b2bfd83d18ca1c32b93cf5bfff58662aaa5";

#[error_code]
pub enum PostProofError {
    #[msg("Post verification request failed")]
    PostVerificationRequestFailed,
    #[msg("Verification too fast")]
    VerificationTooFast,
    #[msg("Invalid callback")]
    InvalidCallback,
    #[msg("Invalid output")]
    InvalidOutput,
    #[msg("Callback error")]
    CallbackError,
    #[msg("Config not active")]
    ConfigNotActive,
    #[msg("Max claimers reached")]
    MaxClaimersReached,
    #[msg("Insufficient funds")]
    InsufficientFunds,
}

#[program]
pub mod proof_of_post {
    use super::*;

    pub fn create_config(ctx: Context<CreateConfig>, args: CreateConfigArgs) -> Result<()> {
        msg!("Creating PostProofConfig");

        ctx.accounts.post_proof_config.creator = ctx.accounts.creator.key();
        ctx.accounts.post_proof_config.seeds = args.seeds;
        ctx.accounts.post_proof_config.keywords = args.keywords;
        ctx.accounts.post_proof_config.claimers_count = 0;
        ctx.accounts.post_proof_config.reward_amount = args.reward_amount;
        ctx.accounts.post_proof_config.max_claimers = args.max_claimers;
        ctx.accounts.post_proof_config.active = true;
        ctx.accounts.post_proof_config.created_slot = sysvar::clock::Clock::get()?.slot;

        // transfer initial funds to config account
        let rent = Rent::get()?;
        let min_balance =
            rent.minimum_balance(ctx.accounts.post_proof_config.to_account_info().data_len());
        let total_required = min_balance + args.reward_amount * args.max_claimers;

        if total_required > 0 {
            anchor_lang::system_program::transfer(
                CpiContext::new(
                    ctx.accounts.system_program.to_account_info(),
                    anchor_lang::system_program::Transfer {
                        from: ctx.accounts.creator.to_account_info(),
                        to: ctx.accounts.post_proof_config.to_account_info(),
                    },
                ),
                total_required,
            )?;
        }

        Ok(())
    }

    pub fn update_config(ctx: Context<UpdateConfig>, args: UpdateConfigArgs) -> Result<()> {
        msg!("Updating PostProofConfig");

        if let Some(active) = args.active {
            ctx.accounts.post_proof_config.active = active;
        }
        if let Some(max_claimers) = args.max_claimers {
            ctx.accounts.post_proof_config.max_claimers = max_claimers;
        }
        if let Some(reward_amount) = args.reward_amount {
            ctx.accounts.post_proof_config.reward_amount = reward_amount;
        }

        Ok(())
    }

    pub fn verify_post(ctx: Context<VerifyPost>, args: VerifyPostArgs) -> Result<()> {
        msg!("Processing verify_post for post_url: {}", args.post_url);

        // Check if config is active
        if !ctx.accounts.post_proof_config.active {
            return Err(PostProofError::ConfigNotActive.into());
        }

        // Check if max claimers reached
        if ctx.accounts.post_proof_config.claimers_count
            >= ctx.accounts.post_proof_config.max_claimers
        {
            return Err(PostProofError::MaxClaimersReached.into());
        }

        // Check if config has sufficient funds for reward
        if ctx.accounts.post_proof_config.to_account_info().lamports()
            < ctx.accounts.post_proof_config.reward_amount
        {
            return Err(PostProofError::InsufficientFunds.into());
        }

        // Expected requester PDA
        let (expected_requester, _bump) =
            Pubkey::find_program_address(&[args.current_req_id.as_bytes()], &crate::id());
        if ctx.accounts.requester.key() != expected_requester {
            return Err(PostProofError::PostVerificationRequestFailed.into());
        }

        let slot = sysvar::clock::Clock::get()?.slot;

        // Rate limiting: prevent spam verifications
        if slot - ctx.accounts.post_verification_log.slot < 100 {
            return Err(PostProofError::VerificationTooFast.into());
        }

        // Initialize requester account if it doesn't exist
        if ctx.accounts.requester.lamports() == 0 {
            let space = ExecutionTracker::INIT_SPACE + 8;
            let rent = Rent::get()?;
            let lamports = rent.minimum_balance(space);

            anchor_lang::system_program::create_account(
                CpiContext::new_with_signer(
                    ctx.accounts.system_program.to_account_info(),
                    anchor_lang::system_program::CreateAccount {
                        from: ctx.accounts.verifier.to_account_info(),
                        to: ctx.accounts.requester.to_account_info(),
                    },
                    &[&[args.current_req_id.as_bytes(), &[ctx.bumps.requester]]],
                ),
                lamports,
                space as u64,
                &crate::id(),
            )?;
        }

        // Create comma-separated keywords string
        let keywords_string = ctx.accounts.post_proof_config.keywords.join(",");
        let keywords_bytes = keywords_string.as_bytes();

        msg!(
            "satyam123, post_size: {}, post_url: {}, keyword_string: {:?}",
            args.post_size,
            args.post_url,
            keywords_string
        );

        // Build public input: [post_size(8)][keywords_size(8)][keywords_string]
        let mut public_input = Vec::new();
        public_input.extend_from_slice(&args.post_size.to_be_bytes());
        public_input.extend_from_slice(&(keywords_bytes.len() as u64).to_be_bytes());
        public_input.extend_from_slice(keywords_bytes);

        // Prepare Bonsol execution
        let bonsol_ix = execute_v1(
            &ctx.accounts.verifier.key(),
            &ctx.accounts.verifier.key(),
            POST_VERIFICATION_IMAGE_ID,
            &args.current_req_id,
            vec![
                InputRef::public(&public_input),
                InputRef::url(args.post_url.as_bytes()),
            ],
            args.tip,
            slot + 50000,
            ExecutionConfig {
                verify_input_hash: false,
                input_hash: None,
                forward_output: true,
            },
            Some(CallbackConfig {
                program_id: crate::id(),
                instruction_prefix: vec![181, 16, 138, 77, 227, 78, 167, 151], // bonsol_callback instruction discriminator
                extra_accounts: vec![
                    AccountMeta::new_readonly(ctx.accounts.requester.key(), false),
                    AccountMeta::new(ctx.accounts.post_proof_config.key(), false),
                    AccountMeta::new(ctx.accounts.post_verification_log.key(), false),
                    AccountMeta::new(ctx.accounts.verifier.key(), false),
                ],
            }),
            None,
        )
        .map_err(|_| ProgramError::InvalidInstructionData)?;

        msg!("Invoking Bonsol execute_v1 CPI");
        invoke(
            &bonsol_ix,
            &[
                ctx.accounts.verifier.to_account_info().clone(),
                ctx.accounts.system_program.to_account_info().clone(),
                ctx.accounts.execution_request.to_account_info().clone(),
                ctx.accounts.bonsol_program.to_account_info().clone(),
                ctx.accounts.deployment_account.to_account_info().clone(),
                ctx.accounts.requester.to_account_info().clone(),
                ctx.accounts.post_proof_config.to_account_info().clone(),
                ctx.accounts.post_verification_log.to_account_info().clone(),
                ctx.accounts.verifier.to_account_info().clone(),
                ctx.accounts.post_proof_program.to_account_info().clone(),
            ],
        )?;
        msg!("Bonsol execute_v1 CPI invoked");

        // Store execution account reference in requester
        let mut requester_data = ctx.accounts.requester.try_borrow_mut_data()?;
        let tracker = ExecutionTracker {
            execution_account: ctx.accounts.execution_request.key(),
        };

        // Pack the ExecutionTracker data
        let mut data = [0u8; ExecutionTracker::INIT_SPACE];
        tracker.pack(&mut data)?;

        // Write discriminator + data
        requester_data[0..8].copy_from_slice(&ExecutionTracker::DISCRIMINATOR);
        requester_data[8..8 + ExecutionTracker::INIT_SPACE].copy_from_slice(&data);

        ctx.accounts.post_verification_log.current_execution_account =
            Some(ctx.accounts.execution_request.key());
        ctx.accounts.post_verification_log.verifier = ctx.accounts.verifier.key();
        ctx.accounts.post_verification_log.post_url = args.post_url.clone();
        ctx.accounts.post_verification_log.config = ctx.accounts.post_proof_config.key();

        Ok(())
    }

    pub fn bonsol_callback(ctx: Context<BonsolCallback>, data: [u8; 33]) -> Result<()> {
        msg!("Processing bonsol_callback");
        let slot = sysvar::clock::Clock::get()?.slot;

        if let Some(epub) = ctx.accounts.post_verification_log.current_execution_account {
            if ctx.accounts.execution_request.key() != epub {
                msg!("Invalid execution request account");
                return Err(PostProofError::InvalidCallback.into());
            }

            // Get execution account from requester data
            let requester_data = ctx.accounts.requester.try_borrow_data()?;

            // Skip discriminator and get ExecutionTracker data
            if requester_data.len() < 8 + ExecutionTracker::INIT_SPACE {
                msg!("Requester data too short");
                return Err(PostProofError::InvalidCallback.into());
            }

            let tracker_data = &requester_data[8..8 + ExecutionTracker::INIT_SPACE];
            let tracker = ExecutionTracker::unpack(tracker_data)?;
            let execution_account = tracker.execution_account;
            drop(requester_data);

            let ainfos = ctx.accounts.to_account_infos();

            let output = handle_callback(
                POST_VERIFICATION_IMAGE_ID,
                &execution_account,
                &ainfos,
                &data,
            )
            .map_err(|_| PostProofError::CallbackError)?;
            msg!("Callback handled, output received");

            // Extract boolean result from ZK proof output
            let is_valid_post = if output.committed_outputs.len() > 0 {
                output.committed_outputs[0] != 0
            } else {
                false
            };

            msg!("Post verification result: {}", is_valid_post);

            // Update verification log
            ctx.accounts.post_verification_log.slot = slot;
            ctx.accounts.post_verification_log.is_verified = is_valid_post;
            ctx.accounts.post_verification_log.current_execution_account = None;

            // If post is valid, transfer reward and update stats
            if is_valid_post {
                // Transfer SOL reward to verifier
                let reward_amount = ctx.accounts.post_proof_config.reward_amount;

                **ctx
                    .accounts
                    .post_proof_config
                    .to_account_info()
                    .try_borrow_mut_lamports()? -= reward_amount;
                **ctx
                    .accounts
                    .verifier
                    .to_account_info()
                    .try_borrow_mut_lamports()? += reward_amount;

                // Update claimers count
                ctx.accounts.post_proof_config.claimers_count += 1;

                msg!(
                    "Post verified for campaign {:?}! Transferred {} lamports to verifier. Total claimers: {}",
                    ctx.accounts.post_proof_config.seeds,
                    reward_amount,
                    ctx.accounts.post_proof_config.claimers_count
                );

                // Deactivate config if max claimers reached
                if ctx.accounts.post_proof_config.claimers_count
                    >= ctx.accounts.post_proof_config.max_claimers
                {
                    ctx.accounts.post_proof_config.active = false;
                    msg!("Config deactivated - max claimers reached for campaign {:?}", ctx.accounts.post_proof_config.seeds);
                }
            } else {
                msg!("Post verification failed for campaign {:?}", ctx.accounts.post_proof_config.seeds);
            }

            Ok(())
        } else {
            Err(PostProofError::InvalidCallback.into())
        }
    }
}

#[account]
#[derive(InitSpace)]
pub struct PostProofConfig {
    pub creator: Pubkey,
    #[max_len(10)]
    pub seeds: String,
    #[max_len(20, 50)]
    pub keywords: Vec<String>,
    pub claimers_count: u64,
    pub reward_amount: u64,
    pub max_claimers: u64,
    pub active: bool,
    pub created_slot: u64,
}

#[account]
#[derive(InitSpace)]
pub struct PostVerificationLog {
    pub verifier: Pubkey,
    pub config: Pubkey,
    #[max_len(256)]
    pub post_url: String,
    pub slot: u64,
    pub is_verified: bool,
    pub current_execution_account: Option<Pubkey>,
}

#[account]
#[derive(InitSpace)]
pub struct ExecutionTracker {
    pub execution_account: Pubkey,
}

impl ExecutionTracker {
    pub const INIT_SPACE: usize = 32; // Just the Pubkey size

    pub fn pack(&self, dst: &mut [u8]) -> Result<()> {
        if dst.len() < Self::INIT_SPACE {
            return Err(ProgramError::AccountDataTooSmall.into());
        }
        dst[0..32].copy_from_slice(&self.execution_account.to_bytes());
        Ok(())
    }

    pub fn unpack(src: &[u8]) -> Result<Self> {
        if src.len() < Self::INIT_SPACE {
            return Err(ProgramError::AccountDataTooSmall.into());
        }
        let execution_account = Pubkey::new_from_array([
            src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7], src[8], src[9],
            src[10], src[11], src[12], src[13], src[14], src[15], src[16], src[17], src[18],
            src[19], src[20], src[21], src[22], src[23], src[24], src[25], src[26], src[27],
            src[28], src[29], src[30], src[31],
        ]);
        Ok(Self { execution_account })
    }
}

#[derive(AnchorDeserialize, AnchorSerialize, InitSpace)]
pub struct CreateConfigArgs {
    #[max_len(10)]
    pub seeds: String,
    #[max_len(20, 50)]
    pub keywords: Vec<String>,
    pub reward_amount: u64,
    pub max_claimers: u64,
}

#[derive(AnchorDeserialize, AnchorSerialize)]
pub struct UpdateConfigArgs {
    pub active: Option<bool>,
    pub max_claimers: Option<u64>,
    pub reward_amount: Option<u64>,
}

#[derive(AnchorDeserialize, AnchorSerialize)]
pub struct VerifyPostArgs {
    pub current_req_id: String,
    pub post_url: String,
    pub post_size: u64,
    pub tip: u64,
}

#[derive(Accounts)]
#[instruction(args: CreateConfigArgs)]
pub struct CreateConfig<'info> {
    #[account(
        init_if_needed,
        payer = creator,
        seeds = [b"postproofconfig", creator.key().as_ref(), args.seeds.as_bytes()],
        bump,
        space = 8 + PostProofConfig::INIT_SPACE,
    )]
    pub post_proof_config: Account<'info, PostProofConfig>,

    #[account(mut)]
    pub creator: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    #[account(
        mut,
        has_one = creator
    )]
    pub post_proof_config: Account<'info, PostProofConfig>,

    pub creator: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(args: VerifyPostArgs)]
pub struct VerifyPost<'info> {
    #[account(mut)]
    pub post_proof_config: Account<'info, PostProofConfig>,

    #[account(
        init,
        space = 8 + PostVerificationLog::INIT_SPACE,
        payer = verifier,
        seeds = [b"postverificationlog", verifier.key().as_ref(), post_proof_config.key().as_ref()],
        bump,
    )]
    pub post_verification_log: Account<'info, PostVerificationLog>,

    #[account(mut)]
    pub verifier: Signer<'info>,

    pub bonsol_program: Program<'info, Bonsol>,

    #[account(
        mut,
        seeds = [args.current_req_id.as_bytes()],
        bump
    )]
    /// CHECK: This account will be initialized in the instruction if needed
    pub requester: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: This is the execution request account
    pub execution_request: UncheckedAccount<'info>,

    /// CHECK: This is the deployment account
    pub deployment_account: UncheckedAccount<'info>,

    #[account(address = crate::id())]
    /// CHECK: This is the current program address
    pub post_proof_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BonsolCallback<'info> {
    /// CHECK: This is the raw ER account, checked in the callback handler
    pub execution_request: UncheckedAccount<'info>,

    /// CHECK: This is the requester PDA that stores ExecutionTracker data
    pub requester: UncheckedAccount<'info>,

    #[account(mut)]
    pub post_proof_config: Account<'info, PostProofConfig>,

    #[account(
        mut,
        seeds = [b"postverificationlog", verifier.key().as_ref(), post_proof_config.key().as_ref()],
        bump
    )]
    pub post_verification_log: Account<'info, PostVerificationLog>,

    #[account(mut)]
    /// CHECK: Will receive SOL reward if verification succeeds
    pub verifier: UncheckedAccount<'info>,
}
