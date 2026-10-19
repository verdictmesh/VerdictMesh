use anchor_lang::prelude::*;

use crate::{
    seeds,
    state::{Config, Integrator, Policy},
};

/// Binds an arbitration policy to an integrator — `FR-001`, `FR-002`.
///
/// The policy is validated here and nowhere else: it enters a dispute as a
/// snapshot (`FR-003`), and nobody re-validates the snapshot. So
/// `Policy::validate` is the only boundary between the numbers a client sent
/// and arithmetic over other people's funds.
///
/// There is no instruction to update the policy, just as with `initialize`. A policy that could
/// be changed under already open disputes would make the snapshot in `Dispute` mere decoration:
/// the parties would agree to one set of rules while the review followed another. Rules can
/// only be changed by registering a new integrator — that is, in full view of parties that
/// have not yet joined it.
impl RegisterIntegrator<'_> {
    pub fn handle(ctx: Context<RegisterIntegrator>, policy: Policy) -> Result<()> {
        policy.validate()?;

        let integrator = &mut ctx.accounts.integrator;
        integrator.authority = ctx.accounts.authority.key();
        integrator.escrow_program = ctx.accounts.escrow_program.key();
        integrator.policy = policy;
        integrator.dispute_count = 0;
        integrator.bump = ctx.bumps.integrator;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct RegisterIntegrator<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// A PDA keyed by the owner: a protocol can be claimed only with one's own
    /// signature, and someone else's key has nowhere to write its policy.
    #[account(
        init,
        payer = authority,
        space = Integrator::DISCRIMINATOR.len() + Integrator::INIT_SPACE,
        seeds = [seeds::INTEGRATOR, authority.key().as_ref()],
        bump,
    )]
    pub integrator: Account<'info, Integrator>,

    /// There is no policy without a settlement asset: `juror_stake` and
    /// `deposit` are denominated in it (`FR-011a`). Requiring `Config` makes the
    /// `initialize` → `register_integrator` order a check, not a convention.
    #[account(seeds = [seeds::CONFIG], bump = config.bump)]
    pub config: Account<'info, Config>,

    /// The integrator's escrow program: `open_dispute` later uses it to recognize that a dispute
    /// is opened by that program's own account. We check that it really is a program — nothing
    /// can update this field, so a mistake in it would be permanent.
    ///
    /// CHECK: no test and no instruction invokes it; only the executable flag
    /// is checked.
    #[account(executable)]
    pub escrow_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}
