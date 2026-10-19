use anchor_lang::prelude::*;

use crate::state::{Ballot, Verdict};

/// FR-029: from events alone, an external observer reconstructs the full timeline of a dispute
/// without access to the off-chain service. The watcher in apps/api reads exactly these.
#[event]
pub struct DisputeOpened {
    pub dispute: Pubkey,
    pub integrator: Pubkey,
    pub escrow_ref: Pubkey,
    pub claimant: Pubkey,
    pub respondent: Pubkey,
    pub amount: u64,
    pub optimistic: bool,
    pub opened_at: i64,
}

/// The review deposit has been paid — `FR-026`. The amount is in the event because it is a
/// snapshot of the policy at opening time: the integrator may change `Policy` tomorrow, but the
/// party must see how much it was charged today (`FR-026d`).
#[event]
pub struct DepositCollected {
    pub dispute: Pubkey,
    pub depositor: Pubkey,
    pub amount: u64,
}

#[event]
pub struct PanelSelected {
    pub dispute: Pubkey,
    pub panel: Vec<Pubkey>,
    pub entropy_slot: u64,
}

#[event]
pub struct ReportAttested {
    pub dispute: Pubkey,
    pub report_hash: [u8; 32],
}

#[event]
pub struct VoteCommitted {
    pub dispute: Pubkey,
    pub juror: Pubkey,
}

#[event]
pub struct VoteRevealed {
    pub dispute: Pubkey,
    pub juror: Pubkey,
    pub choice: Ballot,
}

/// The dispute has moved to the extended panel — `FR-027`. The votes that caused the escalation
/// are in the event itself: otherwise the party sees that the review was extended but does not
/// see why.
///
/// The list of jurors who did not reveal is deliberately absent, even though `FR-027b` slashes
/// exactly them. The tally does not know them — it sees two numbers, not vote accounts — and
/// passing the whole panel's accounts just for an event would hit the transaction limit, while
/// the observer derives that list on its own anyway: `FR-029` gives it every `VoteCommitted`
/// without a matching `VoteRevealed`.
#[event]
pub struct DisputeEscalated {
    pub dispute: Pubkey,
    pub votes_claimant: u8,
    pub votes_respondent: u8,
    pub commit_deadline: i64,
    pub reveal_deadline: i64,
}

/// A verdict has been reached — `FR-010`. The counts sit next to the outcome because a party
/// cannot verify a verdict without its basis.
#[event]
pub struct DisputeTallied {
    pub dispute: Pubkey,
    pub verdict: Verdict,
    pub votes_claimant: u8,
    pub votes_respondent: u8,
    pub appeal_deadline: i64,
}

#[event]
pub struct DisputeFinalized {
    pub dispute: Pubkey,
    pub verdict: Verdict,
    pub finalized_at: i64,
}

/// Who received the slashed stake — `FR-011`. Together with `JurorSlashed` this gives the full
/// settlement balance: the observer sees what was taken and where it went without reading
/// accounts.
#[event]
pub struct JurorRewarded {
    pub dispute: Pubkey,
    pub juror: Pubkey,
    pub amount: u64,
}

/// How the review fee was distributed — `FR-026b`. Two numbers are enough to reconcile the
/// dispute vault: together they equal what it held, and the account itself is closed
/// afterwards.
#[event]
pub struct DisputeFeeSettled {
    pub dispute: Pubkey,
    pub jurors: u64,
    pub protocol: u64,
}

#[event]
pub struct JurorSlashed {
    pub dispute: Pubkey,
    pub juror: Pubkey,
    pub amount: u64,
    pub no_reveal: bool,
}

/// A juror joined the registry. The watcher (T027) builds the list of eligible jurors from
/// these events without reading every program account: `getProgramAccounts` on every selection
/// is exactly what `JurorIndex` exists to avoid.
#[event]
pub struct JurorStaked {
    pub juror: Pubkey,
    pub stake: u64,
    pub index: u32,
    pub juror_count: u32,
}

/// A juror left the registry, with swap-remove. `index` is the freed slot,
/// `moved` is the juror moved into it from the tail.
/// The `JurorStaked` / `JurorUnstaked` pair of events is enough to rebuild
/// the full registry membership — `FR-029`.
#[event]
pub struct JurorUnstaked {
    pub juror: Pubkey,
    pub stake: u64,
    pub index: u32,
    pub moved: Option<Pubkey>,
    pub juror_count: u32,
}
