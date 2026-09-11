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

/// Вступ до реєстру присяжних. Watcher (T027) будує з цих подій список
/// придатних присяжних, не читаючи всі акаунти програми: `getProgramAccounts`
/// на кожному відборі — це те, чого `JurorIndex` і уникає.
#[event]
pub struct JurorStaked {
    pub juror: Pubkey,
    pub stake: u64,
    pub index: u32,
    pub juror_count: u32,
}

/// Вихід із реєстру разом зі swap-remove. `index` — слот, що звільнився,
/// `moved` — присяжний, який на нього переїхав із хвоста. Пари подій
/// `JurorStaked` / `JurorUnstaked` досить, щоб відтворити склад реєстру
/// цілком — `FR-029`.
#[event]
pub struct JurorUnstaked {
    pub juror: Pubkey,
    pub stake: u64,
    pub index: u32,
    pub moved: Option<Pubkey>,
    pub juror_count: u32,
}
