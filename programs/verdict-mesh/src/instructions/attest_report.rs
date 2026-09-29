use anchor_lang::prelude::*;

use crate::{
    errors::VerdictMeshError,
    events::ReportAttested,
    seeds,
    state::{Config, Dispute, DisputeState},
};

/// The report fingerprint goes on chain — `FR-017`. The reporter role signs
/// it, once, while the first panel is still sealing its votes.
///
/// **What the fingerprint proves and what it does not.** It fixes *which*
/// report jurors were shown: anyone can hash the published body and compare
/// (`FR-017b`), so a report swapped after the fact no longer matches. It says
/// nothing about whether the report is fair — the program never sees the body,
/// and an honest hash of a biased report is still an honest hash (see
/// `SPEC.md` → out of scope).
///
/// **Why the reporter can do nothing else.** This is the only instruction
/// that asks for the reporter's signature, it names one writable account, and
/// the handler writes one field of it (`FR-017a`). The role's reach is the
/// size of this function, not a policy about it.
///
/// **The window.** A dispute opens straight into `Committing`, so "before the
/// commit window" cannot mean before it opens; it means before it closes, in
/// the first round. After an escalation the first round's votes are public,
/// and a report written then would be written with them in view.
impl AttestReport<'_> {
    pub fn handle(ctx: Context<AttestReport>, report_hash: [u8; 32]) -> Result<()> {
        let dispute = &mut ctx.accounts.dispute;

        // Zero is how the field reads before any report. Accepting it would
        // record nothing and leave the next attempt looking like the first.
        require!(report_hash != [0u8; 32], VerdictMeshError::EmptyReportHash);
        // Once, whatever the second one says: a different fingerprint is a
        // swapped report, the same one is a transaction with nothing to do.
        require!(
            dispute.report_hash == [0u8; 32],
            VerdictMeshError::ReportAlreadyAttested
        );
        require!(
            dispute.state == DisputeState::Committing && !dispute.escalated,
            VerdictMeshError::WrongState
        );
        // Strictly before the deadline, the same bound `commit_vote` uses: at
        // the deadline itself votes are no longer sealed, and neither is the
        // report they were meant to be sealed against.
        require!(
            Clock::get()?.unix_timestamp < dispute.commit_deadline,
            VerdictMeshError::WindowClosed
        );

        dispute.report_hash = report_hash;

        emit!(ReportAttested {
            dispute: dispute.key(),
            report_hash,
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct AttestReport<'info> {
    /// The reporter role named in the protocol config. Not writable: the
    /// transaction fee is paid by whoever pays it, and the role needs no
    /// account of its own.
    pub reporter: Signer<'info>,

    /// The protocol config, read for the reporter key it names.
    #[account(
        seeds = [seeds::CONFIG],
        bump = config.bump,
        has_one = reporter @ VerdictMeshError::NotReporter,
    )]
    pub config: Account<'info, Config>,

    /// The dispute whose report is attested. Only `report_hash` changes.
    #[account(
        mut,
        seeds = [
            seeds::DISPUTE,
            dispute.integrator.as_ref(),
            &dispute.dispute_id.to_le_bytes(),
        ],
        bump = dispute.bump,
    )]
    pub dispute: Account<'info, Dispute>,
}
