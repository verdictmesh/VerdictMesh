use anchor_lang::prelude::*;

use crate::state::Verdict;

/// FR-029: за подіями зовнішній спостерігач відновлює повну хронологію спору
/// без доступу до офчейн-сервісу. Watcher у apps/api читає саме їх.
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
    pub choice: Verdict,
}

#[event]
pub struct DisputeEscalated {
    pub dispute: Pubkey,
    pub panel: Vec<Pubkey>,
    pub not_revealed: Vec<Pubkey>,
}

#[event]
pub struct DisputeFinalized {
    pub dispute: Pubkey,
    pub verdict: Verdict,
    pub finalized_at: i64,
}

#[event]
pub struct JurorSlashed {
    pub dispute: Pubkey,
    pub juror: Pubkey,
    pub amount: u64,
    pub no_reveal: bool,
}
