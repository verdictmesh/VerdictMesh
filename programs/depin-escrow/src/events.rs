use anchor_lang::prelude::*;

/// Events are what fact-finding rebuilds the **subject** of a dispute from:
/// what was locked, what the network reported and when, what the operator
/// claimed (`FR-015`, `FR-016`).
#[event]
pub struct EpochFunded {
    pub epoch: Pubkey,
    pub network: Pubkey,
    pub operator: Pubkey,
    pub integrator: Pubkey,
    pub reward: u64,
    pub challenge_window: i64,
}

#[event]
pub struct UsageReported {
    pub epoch: Pubkey,
    pub operator_bps: u16,
    pub challenge_deadline: i64,
}

#[event]
pub struct EpochDisputed {
    pub epoch: Pubkey,
    pub dispute: Pubkey,
    pub reported_bps: u16,
    pub claimed_bps: u16,
    /// The part of the reward the two offers disagree on — the dispute's
    /// `amount`.
    pub contested: u64,
}

/// The reward is paid out. `dispute` is empty when the report stood
/// unchallenged. The verdict itself is not repeated: VerdictMesh announced it
/// (`DisputeTallied`), and a second record of the same fact would sooner or
/// later disagree with the first.
#[event]
pub struct EpochSettled {
    pub epoch: Pubkey,
    pub dispute: Option<Pubkey>,
    pub operator_bps: u16,
    pub operator_amount: u64,
    pub network_amount: u64,
    /// How much of the network's share reimbursed the operator's review
    /// deposit — `FR-026a`. Zero when the network won, when the escalation
    /// failed, or when there was no dispute.
    pub reimbursed: u64,
}
