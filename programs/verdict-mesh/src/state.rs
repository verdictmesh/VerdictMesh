use anchor_lang::prelude::*;

/// Копіюється у кожен спір при відкритті. Зміна політики інтегратором не впливає
/// на вже відкриті спори — FR-003.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Policy {
    pub panel_size: u8,
    pub extended_panel_size: u8,
    pub quorum: u8,
    pub extended_quorum: u8,
    pub juror_stake: u64,
    pub slash_bps_wrong: u16,
    pub slash_bps_no_reveal: u16,
    pub commit_window: i64,
    pub reveal_window: i64,
    pub appeal_window: i64,
    pub optimistic_window: i64,
    pub deposit: u64,
    pub optimistic_threshold: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisputeState {
    OptimisticPending,
    Committing,
    Revealing,
    Tallied,
    Appealed,
    Finalized,
}

/// StatusQuo — «як ніби спору не було» (FR-027a). Ескроу зобов'язаний уміти
/// розподілити кошти за цим результатом, інакше автоескалація нікуди не веде.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Claimant,
    Respondent,
    StatusQuo,
}

#[account]
pub struct Config {
    pub settlement_mint: Pubkey,
    /// Єдиний привілейований ключ у системі. Може лише записати відбиток звіту —
    /// FR-017a. Інструкцій, що змінюють вердикт чи рухають кошти, для нього немає.
    pub reporter: Pubkey,
    pub bump: u8,
}

#[account]
pub struct Integrator {
    pub authority: Pubkey,
    pub escrow_program: Pubkey,
    pub policy: Policy,
    pub dispute_count: u64,
    pub bump: u8,
}

#[account]
pub struct JurorRegistry {
    pub juror_count: u32,
    pub bump: u8,
}

#[account]
pub struct Juror {
    pub wallet: Pubkey,
    pub stake: u64,
    pub active_disputes: u16,
    pub index: u32,
    pub bump: u8,
}

/// Дає реєстру перелічуваність за індексом — без цього детермінований відбір
/// (FR-006) не може вибрати N із M, не читаючи весь реєстр офчейн.
/// Вихід присяжного — swap-remove: останній індекс переїжджає на звільнений.
#[account]
pub struct JurorIndex {
    pub wallet: Pubkey,
    pub bump: u8,
}

#[account]
pub struct Dispute {
    pub integrator: Pubkey,
    pub dispute_id: u64,
    /// Знімок, не посилання — FR-003.
    pub policy: Policy,
    /// PDA ескроу, який відкрив спір. Ескроу звіряє це поле зі своїм адресом,
    /// перш ніж виконувати вердикт — інакше чужий спір міг би розпорядитись
    /// його коштами.
    pub escrow_ref: Pubkey,
    pub claimant: Pubkey,
    pub respondent: Pubkey,
    pub amount: u64,
    pub state: DisputeState,
    pub panel: Vec<Pubkey>,
    pub report_hash: [u8; 32],
    pub claimant_claim_hash: [u8; 32],
    pub respondent_claim_hash: [u8; 32],
    pub opened_at: i64,
    pub commit_deadline: i64,
    pub reveal_deadline: i64,
    pub appeal_deadline: i64,
    pub votes_claimant: u8,
    pub votes_respondent: u8,
    pub revealed_count: u8,
    /// Автоескалація застосовується один раз — FR-027a.
    pub escalated: bool,
    pub verdict: Option<Verdict>,
    pub settled: bool,
    pub bump: u8,
}

#[account]
pub struct VoteCommit {
    pub dispute: Pubkey,
    pub juror: Pubkey,
    pub commitment: [u8; 32],
    pub revealed: bool,
    pub choice: Option<Verdict>,
    pub bump: u8,
}
