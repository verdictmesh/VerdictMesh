use anchor_lang::prelude::*;

use crate::state::{Ballot, Verdict};

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
    pub choice: Ballot,
}

/// Спір пішов на розширену панель — `FR-027`. Голоси, що спричинили ескалацію,
/// у самій події: інакше сторона бачить, що розгляд подовжився, і не бачить
/// чому.
///
/// Списку тих, хто не розкрився, тут немає навмисно, хоча `FR-027b` саме їх і
/// слешить. Підрахунок їх не знає — він бачить два числа, а не акаунти голосів,
/// — і тягнути заради події всю панель акаунтами означало б впертись у ліміт
/// транзакції там, де спостерігач однаково виводить цей список сам: `FR-029`
/// дає йому `VoteCommitted` без парного `VoteRevealed`.
#[event]
pub struct DisputeEscalated {
    pub dispute: Pubkey,
    pub votes_claimant: u8,
    pub votes_respondent: u8,
    pub commit_deadline: i64,
    pub reveal_deadline: i64,
}

/// Вердикт винесено — `FR-010`. Числа поруч із результатом, бо вердикт без
/// підстави сторона перевірити не може.
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

/// Кому дісталось злетіле зі стейків — `FR-011`. Разом із `JurorSlashed` дає
/// повний баланс розрахунку: спостерігач бачить, що вийшло і куди пішло, не
/// читаючи акаунтів.
#[event]
pub struct JurorRewarded {
    pub dispute: Pubkey,
    pub juror: Pubkey,
    pub amount: u64,
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
