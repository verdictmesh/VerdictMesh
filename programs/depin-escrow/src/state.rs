use anchor_lang::prelude::*;

/// A share of the epoch's reward, in basis points of `Epoch.reward`, that goes
/// to the operator. The network keeps the rest.
pub const FULL_SHARE: u16 = 10_000;

/// Where the epoch stands.
///
/// **`Disputed` carries both offers and the address of its review**, not a
/// flag. The offers are what the verdict chooses between — final-offer
/// arbitration, so the escrow must remember exactly what each side put on the
/// table. The address is what ties the payout to this review and no other: a
/// second dispute over the same epoch cannot exist, but a forged one laid out
/// with the same `escrow_ref` would match on that field alone (`FR-013`).
#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, PartialEq, Eq, Debug)]
pub enum EpochState {
    /// The reward is locked; the network has not reported usage yet.
    Funded,
    /// The network has published the split. Unchallenged by `challenge_deadline`,
    /// it is what gets paid.
    Reported {
        operator_bps: u16,
        challenge_deadline: i64,
    },
    /// The operator challenged the report with an offer of their own.
    Disputed {
        dispute: Pubkey,
        reported_bps: u16,
        claimed_bps: u16,
    },
    /// Paid out. Terminal: there is nothing left in the vault to move.
    Settled { operator_bps: u16 },
}

/// One reward epoch between a DePIN network and one of its operators.
///
/// The network locks the reward up front, serves the epoch, then reports how
/// much of it the operator earned. Unlike a milestone deal the payout is never
/// all-or-nothing: whichever split wins, **both** parties are paid a share —
/// and that share of the loser is what reimburses the review deposit
/// (`FR-026a`), so this escrow needs no bond vault (`FR-026e`).
#[account]
#[derive(InitSpace)]
pub struct Epoch {
    /// Locks the reward and reports usage.
    pub network: Pubkey,
    /// Earns a share of the reward and may challenge the report.
    pub operator: Pubkey,
    /// The reward asset. Always the protocol's settlement asset: the deposit is
    /// reimbursed out of the loser's share, which only works in the asset the
    /// deposit was paid in (`FR-011a`).
    pub mint: Pubkey,
    /// PDA `Integrator` in VerdictMesh — the policy both parties signed up to.
    pub integrator: Pubkey,
    pub reward: u64,
    /// How long the operator has to challenge a report, in seconds. Fixed when
    /// the epoch is funded, so the network cannot shorten it after the fact.
    pub challenge_window: i64,
    /// The network's own number for the epoch; part of the address.
    pub epoch_id: u64,
    pub state: EpochState,
    pub bump: u8,
}
