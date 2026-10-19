use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{
    errors::VerdictMeshError,
    events::{DepositCollected, DisputeOpened},
    seeds,
    state::{Config, Dispute, DisputeState, Integrator},
};

impl OpenDispute<'_> {
    /// Opens a dispute over a locked balance — `FR-004`.
    ///
    /// The policy is copied, not borrowed: `FR-003` requires that the review
    /// rules do not change until finalization, and a reference to `Integrator`
    /// does not guarantee that.
    ///
    /// The review deposit is collected here and now — `FR-026`. It is also the
    /// price of the review (`FR-026d`), so a dispute without a paid review
    /// cannot exist at all: the vault is created and funded by the same
    /// transaction that opens the dispute, and the panel never works on credit.
    pub fn handle(
        ctx: Context<OpenDispute>,
        claimant: Pubkey,
        respondent: Pubkey,
        amount: u64,
        claimant_claim_hash: [u8; 32],
        respondent_claim_hash: [u8; 32],
    ) -> Result<()> {
        require_keys_neq!(claimant, respondent, VerdictMeshError::InvalidParties);
        require!(amount > 0, VerdictMeshError::InvalidAmount);
        // FR-005: a zero fingerprint is not an "empty claim" but a missing one.
        // A juror would see only one side's position and have no way to
        // notice.
        require!(
            claimant_claim_hash != [0u8; 32] && respondent_claim_hash != [0u8; 32],
            VerdictMeshError::MissingClaim
        );

        let policy = ctx.accounts.integrator.policy;
        let opened_at = Clock::get()?.unix_timestamp;
        let commit_deadline = opened_at
            .checked_add(policy.commit_window)
            .ok_or(VerdictMeshError::Overflow)?;
        let reveal_deadline = commit_deadline
            .checked_add(policy.reveal_window)
            .ok_or(VerdictMeshError::Overflow)?;

        let integrator = &mut ctx.accounts.integrator;
        let dispute_id = integrator.dispute_count;
        integrator.dispute_count = dispute_id
            .checked_add(1)
            .ok_or(VerdictMeshError::Overflow)?;

        let dispute = &mut ctx.accounts.dispute;
        dispute.integrator = integrator.key();
        dispute.dispute_id = dispute_id;
        dispute.policy = policy;
        dispute.escrow_ref = ctx.accounts.escrow.key();
        dispute.claimant = claimant;
        dispute.respondent = respondent;
        dispute.amount = amount;
        dispute.state = DisputeState::Committing;
        dispute.panel = Vec::new();
        dispute.report_hash = [0u8; 32];
        dispute.claimant_claim_hash = claimant_claim_hash;
        dispute.respondent_claim_hash = respondent_claim_hash;
        dispute.opened_at = opened_at;
        // Selection entropy is fixed here, not where the selection itself
        // happens (T016): otherwise it could be re-rolled by retrying slot
        // after slot until the panel is to one's liking. The slot before the
        // current one is used — the current slot's hash does not exist yet.
        dispute.entropy_slot = Clock::get()?.slot.saturating_sub(1);
        dispute.commit_deadline = commit_deadline;
        dispute.reveal_deadline = reveal_deadline;
        // The appeal window starts at the verdict, not at dispute opening.
        dispute.appeal_deadline = 0;
        dispute.votes_claimant = 0;
        dispute.votes_respondent = 0;
        dispute.escalated = false;
        dispute.verdict = None;
        dispute.bump = ctx.bumps.dispute;

        // The transfer comes **after** the state is written: the vault is derived from the
        // dispute's address, and until that exists there is nowhere to transfer to. The order
        // is safe — both halves are in one transaction, so no dispute is left without a deposit.
        let deposit = policy.deposit;
        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.depositor_tokens.to_account_info(),
                    mint: ctx.accounts.settlement_mint.to_account_info(),
                    to: ctx.accounts.dispute_vault.to_account_info(),
                    authority: ctx.accounts.depositor.to_account_info(),
                },
            ),
            deposit,
            ctx.accounts.settlement_mint.decimals,
        )?;

        emit!(DepositCollected {
            dispute: dispute.key(),
            depositor: claimant,
            amount: deposit,
        });

        // FR-029: from events, an outside observer reconstructs the dispute's timeline without
        // access to the off-chain service. The watcher (T027) reads this one.
        emit!(DisputeOpened {
            dispute: dispute.key(),
            integrator: dispute.integrator,
            escrow_ref: dispute.escrow_ref,
            claimant,
            respondent,
            amount,
            optimistic: false,
            opened_at,
        });

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(claimant: Pubkey, respondent: Pubkey)]
pub struct OpenDispute<'info> {
    /// Rent for the dispute accounts is paid by whoever initiates the transaction, not by the
    /// escrow PDA: the PDA may hold no lamports, and requiring them from it would mean
    /// requiring the integrator to keep a balance in someone else's program.
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(seeds = [seeds::CONFIG], bump = config.bump)]
    pub config: Account<'info, Config>,

    /// The deposit is paid in the protocol's settlement asset, not in the
    /// disputed asset: `FR-011a` keeps juror economics independent of what
    /// exactly the parties are disputing.
    #[account(address = config.settlement_mint)]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    /// The deposit is made by **the party opening the dispute** — `FR-026` — and that is exactly
    /// whoever the dispute records as `claimant`. The equality is not a formality: without it
    /// the escrow could open a dispute "on behalf of" the claimant yet pay from someone else's
    /// wallet, and `FR-026a` would have nothing to rest on — the review cost would fall on a
    /// person who never knew about the dispute.
    ///
    /// The signature passes through the CPI: the escrow signs the dispute with
    /// its PDA, the claimant signs the outer transaction.
    #[account(constraint = depositor.key() == claimant @ VerdictMeshError::NotTheDepositor)]
    pub depositor: Signer<'info>,

    #[account(
        mut,
        seeds = [seeds::INTEGRATOR, integrator.authority.as_ref()],
        bump = integrator.bump,
    )]
    pub integrator: Account<'info, Integrator>,

    /// The escrow account whose funds are in dispute. It becomes `escrow_ref` —
    /// the only thing by which the escrow later recognizes "its" dispute before
    /// distributing the funds (`FR-012`).
    ///
    /// Hence two requirements, neither redundant. **Owner** — the program the integrator named
    /// at registration: otherwise `escrow_ref` could point to anything. **Signature** — because
    /// ownership is not enough: anyone can read another program's account, and without a
    /// signature it would be enough to pass in someone else's escrow to make it execute a
    /// fabricated verdict.
    ///
    /// CHECK: the account's contents are unknown to and not needed by the
    /// program; the owner and the signature are checked.
    #[account(signer, owner = integrator.escrow_program)]
    pub escrow: UncheckedAccount<'info>,

    /// The dispute number comes from the integrator's counter, not from the
    /// client: otherwise numbering would stop being dense, and `dispute_count`
    /// would no longer tell the truth about how many disputes exist.
    ///
    /// Space is allocated for the **extended** panel — see `Dispute::space`.
    #[account(
        init,
        payer = payer,
        space = Dispute::space(integrator.policy.extended_panel_size),
        seeds = [
            seeds::DISPUTE,
            integrator.key().as_ref(),
            &integrator.dispute_count.to_le_bytes(),
        ],
        bump,
    )]
    pub dispute: Account<'info, Dispute>,

    #[account(
        mut,
        token::mint = settlement_mint,
        token::authority = depositor,
    )]
    pub depositor_tokens: InterfaceAccount<'info, TokenAccount>,

    /// The vault of this dispute — and only this one. One per dispute, not shared: the appeal
    /// bond (`FR-021`) also lands here and will have to be returned to specific parties, and a
    /// shared vault cannot tell "whose amount this is".
    ///
    /// The authority is `Config`, the same PDA as for the stake vault: no
    /// private key exists for it, so the deposit leaves only by a path the
    /// program signs itself (`FR-014`).
    #[account(
        init,
        payer = payer,
        seeds = [seeds::DISPUTE_VAULT, dispute.key().as_ref()],
        bump,
        token::mint = settlement_mint,
        token::authority = config,
    )]
    pub dispute_vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}
