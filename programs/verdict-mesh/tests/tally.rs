//! T019 — підрахунок, кворум і одноразова автоескалація (`FR-010`, `FR-027`,
//! `FR-027a`).
//!
//! **Спір мусить закінчитись.** `SC-011` вимагає, щоб розгляд, у якому частина
//! панелі змовчала, доходив до результату без людини. Тому тупика тут немає за
//! побудовою: недобір кворуму або рівність один раз відправляють спір на
//! розширену панель, а вдруге — закривають статус-кво. Третього кола не існує.
//!
//! **Ескалація не скидає вже розкриті голоси.** Розширена панель — це той самий
//! розгляд, продовжений більшою кількістю присяжних, а не новий. Скидання
//! лічильників зробило б `VoteCommit` першого раунду нерозкривними назавжди і
//! змусило б тих, хто вже проголосував, голосувати вдруге ні за чим.
//!
//! **Рівність при досягнутому кворумі — теж ескалація.** Кворум відповідає на
//! питання «чи достатньо людей висловилось», більшість — «чи є відповідь».
//! Перше без другого вердикту не дає.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::result::InstructionResult;
use solana_account::Account;
use solana_address::Address;
use solana_instruction::Instruction;
use verdict_mesh::{
    events::{DisputeEscalated, DisputeTallied},
    state::{Dispute, DisputeState, Policy, Verdict},
    VerdictMeshError,
};

/// Слот, з якого спір відкривався. Глибина `SlotHashes` — 512 слотів, тож до
/// підрахунку цей хеш уже не існує.
const STALE_ENTROPY_SLOT: u64 = SLOT - 10_000;

struct Fixture {
    dispute: Pubkey,
    accounts: Vec<(Address, Account)>,
    policy: Policy,
}

impl Fixture {
    /// Спір, який дожив до кінця вікна розкриття з такими голосами.
    fn new(claimant: u8, respondent: u8) -> Self {
        Self::build(claimant, respondent, false, DisputeState::Revealing)
    }

    /// Той самий спір, але вже після одної ескалації: кворум міряється
    /// розширеним, і другого кола не буде.
    fn escalated(claimant: u8, respondent: u8) -> Self {
        Self::build(claimant, respondent, true, DisputeState::Revealing)
    }

    fn build(claimant: u8, respondent: u8, escalated: bool, state: DisputeState) -> Self {
        let policy = demo_policy();
        let authority = Pubkey::new_unique();
        let (integrator, _) = integrator_pda(&authority);
        let (dispute, bump) = dispute_pda(&integrator, 0);

        let mut opened = dispute_state(&integrator, 0, &policy, bump);
        opened.state = state;
        opened.panel = (0..policy.panel_size)
            .map(|_| Pubkey::new_unique())
            .collect();
        opened.votes_claimant = claimant;
        opened.votes_respondent = respondent;
        opened.escalated = escalated;
        // Слот відкриття давно випав із `SlotHashes` — рівно те, як виглядає
        // будь-який спір, що дожив до кінця обох вікон.
        opened.entropy_slot = STALE_ENTROPY_SLOT;

        Self {
            dispute,
            accounts: vec![(addr(&dispute), dispute_account(&opened))],
            policy,
        }
    }

    /// Перша мить, коли підрахунок дозволений.
    fn due(&self) -> i64 {
        NOW + self.policy.commit_window + self.policy.reveal_window
    }

    fn ix(&self) -> Instruction {
        anchor_ix(
            verdict_mesh::accounts::Tally {
                dispute: self.dispute,
            },
            verdict_mesh::instruction::Tally {},
        )
    }

    fn tally(&self) -> InstructionResult {
        self.tally_at(self.due())
    }

    fn tally_at(&self, now: i64) -> InstructionResult {
        mollusk_at(now).process_instruction(&self.ix(), &self.accounts)
    }

    fn after(&self, result: &InstructionResult) -> Dispute {
        decode(resulting(result, &self.dispute))
    }
}

/// Успішний підрахунок і стан, що з нього вийшов.
fn tallied(fixture: &Fixture) -> Dispute {
    let result = fixture.tally();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
    fixture.after(&result)
}

// ── вердикт ─────────────────────────────────────────────────────────────────

/// `FR-010` дослівно: кворум набрано, більшість є — вердикт за нею.
#[test]
fn gives_the_verdict_to_the_majority() {
    for (claimant, respondent, verdict) in [
        (2u8, 1u8, Verdict::Claimant),
        (1, 2, Verdict::Respondent),
        (3, 0, Verdict::Claimant),
    ] {
        let dispute = tallied(&Fixture::new(claimant, respondent));
        assert_eq!(dispute.verdict, Some(verdict), "{claimant}:{respondent}");
        assert_eq!(dispute.state, DisputeState::Tallied);
    }
}

/// Кворум демо-політики — два з трьох. Рівно на межі вердикт уже є.
#[test]
fn counts_a_panel_that_reached_exactly_the_quorum() {
    let dispute = tallied(&Fixture::new(2, 0));
    assert_eq!(dispute.verdict, Some(Verdict::Claimant));
}

/// Вікно апеляції відкривається від вердикту, а не від відкриття спору — доти
/// воно нуль, і виконувати вердикт нікому не можна.
#[test]
fn opens_the_appeal_window_from_the_verdict() {
    let fixture = Fixture::new(2, 1);
    let dispute = tallied(&fixture);
    assert_eq!(
        dispute.appeal_deadline,
        fixture.due() + fixture.policy.appeal_window
    );
}

/// `FR-029`: подія несе не лише вердикт, а й числа, з яких він вийшов —
/// інакше сторона бачить результат і не бачить підстави.
#[test]
fn announces_the_verdict_with_the_votes_behind_it() {
    let fixture = Fixture::new(2, 1);

    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = fixture.due();
    let result = mollusk.process_instruction(&fixture.ix(), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<DisputeTallied> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].dispute, fixture.dispute);
    assert_eq!(events[0].verdict, Verdict::Claimant);
    assert_eq!(events[0].votes_claimant, 2);
    assert_eq!(events[0].votes_respondent, 1);
}

// ── ескалація ───────────────────────────────────────────────────────────────

/// Недобір кворуму — `FR-027`. Один розкритий голос із трьох не є розглядом,
/// хай навіть він одноголосний.
#[test]
fn escalates_when_the_panel_did_not_reach_the_quorum() {
    let dispute = tallied(&Fixture::new(1, 0));
    assert!(dispute.escalated);
    assert_eq!(dispute.state, DisputeState::Committing);
    assert_eq!(dispute.verdict, None);
}

/// Ніхто не розкрився — той самий випадок, і саме він робить `SC-011`
/// досяжним: спір рухається далі без людини.
#[test]
fn escalates_a_panel_that_stayed_silent() {
    let dispute = tallied(&Fixture::new(0, 0));
    assert!(dispute.escalated);
    assert_eq!(dispute.state, DisputeState::Committing);
}

/// Рівність при набраному кворумі — теж ескалація. Кворум каже, що висловились
/// достатньо; вердикту з цього не виходить.
#[test]
fn escalates_a_tie_even_when_the_quorum_was_reached() {
    let fixture = Fixture::new(1, 1);
    let dispute = tallied(&fixture);
    assert!(dispute.escalated);
    assert_eq!(dispute.verdict, None);
}

/// Голоси першого раунду переносяться. Скидання зробило б `VoteCommit` тих, хто
/// вже розкрився, нерозкривними назавжди, а тих, хто розкрився, — змусило б
/// голосувати вдруге ні за чим.
#[test]
fn keeps_the_votes_already_revealed() {
    let dispute = tallied(&Fixture::new(1, 1));
    assert_eq!(dispute.votes_claimant, 1);
    assert_eq!(dispute.votes_respondent, 1);
}

/// Розширений розгляд дістає власні вікна, відлічені від моменту ескалації.
#[test]
fn gives_the_extended_round_its_own_windows() {
    let fixture = Fixture::new(1, 0);
    let dispute = tallied(&fixture);

    assert_eq!(
        dispute.commit_deadline,
        fixture.due() + fixture.policy.commit_window
    );
    assert_eq!(
        dispute.reveal_deadline,
        fixture.due() + fixture.policy.commit_window + fixture.policy.reveal_window
    );
}

/// Ентропія перезакріплюється: слот відкриття спору давно випав із `SlotHashes`,
/// і без цього добрати присяжних до розширеної панелі було б нічим — ескалація
/// вела б у глухий кут, а `SC-011` не досягався б узагалі.
#[test]
fn re_anchors_the_entropy_for_the_extended_draw() {
    let dispute = tallied(&Fixture::new(1, 0));
    assert_eq!(dispute.entropy_slot, ENTROPY_SLOT);
    assert_ne!(dispute.entropy_slot, STALE_ENTROPY_SLOT);
}

/// Вердикт ентропії не чіпає: перезакріплення потрібне лише тому, хто добирає
/// присяжних, і зайвий запис у стан спору тут був би шумом у хронології.
#[test]
fn leaves_the_entropy_alone_when_the_dispute_is_decided() {
    let dispute = tallied(&Fixture::new(2, 1));
    assert_eq!(dispute.entropy_slot, STALE_ENTROPY_SLOT);
}

#[test]
fn announces_the_escalation_with_the_votes_that_caused_it() {
    let fixture = Fixture::new(1, 1);

    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = fixture.due();
    let result = mollusk.process_instruction(&fixture.ix(), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<DisputeEscalated> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].dispute, fixture.dispute);
    assert_eq!(events[0].votes_claimant, 1);
    assert_eq!(events[0].votes_respondent, 1);
}

// ── ескалація рівно одна ────────────────────────────────────────────────────

/// `FR-027a`: розширена панель теж не дала результату — спір закривається
/// поверненням до статус-кво. Це і є та точка, у якій розгляд гарантовано
/// закінчується.
#[test]
fn closes_a_second_failure_with_the_status_quo() {
    let dispute = tallied(&Fixture::escalated(1, 0));
    assert_eq!(dispute.verdict, Some(Verdict::StatusQuo));
    assert_eq!(dispute.state, DisputeState::Tallied);
}

#[test]
fn closes_a_second_tie_with_the_status_quo() {
    let dispute = tallied(&Fixture::escalated(2, 2));
    assert_eq!(dispute.verdict, Some(Verdict::StatusQuo));
}

/// Кворум розширеного розгляду — свій, і він вищий. Три голоси з п'яти
/// закривають спір, два не закрили б.
#[test]
fn measures_the_extended_round_against_the_extended_quorum() {
    let policy = demo_policy();
    assert_eq!(policy.extended_quorum, 3, "фікстура спирається на це число");

    assert_eq!(
        tallied(&Fixture::escalated(2, 0)).verdict,
        Some(Verdict::StatusQuo),
        "двох голосів для розширеного кворуму замало"
    );
    assert_eq!(
        tallied(&Fixture::escalated(3, 0)).verdict,
        Some(Verdict::Claimant),
    );
}

/// Розширений розгляд, що дійшов до вердикту, — звичайний вердикт, а не
/// статус-кво. Другий раунд не гірший за перший, він лише останній.
#[test]
fn a_successful_extended_round_gives_a_real_verdict() {
    let dispute = tallied(&Fixture::escalated(1, 3));
    assert_eq!(dispute.verdict, Some(Verdict::Respondent));
    assert_eq!(dispute.state, DisputeState::Tallied);
}

// ── вікно ───────────────────────────────────────────────────────────────────

/// Рахувати до кінця вікна розкриття означає рахувати голоси тих, хто ще встиг
/// би розкритись.
#[test]
fn refuses_a_tally_before_the_reveal_window_closes() {
    let fixture = Fixture::new(2, 1);
    let result = fixture.tally_at(fixture.due() - 1);
    assert!(failed_with(&result, VerdictMeshError::WindowOpen));
}

#[test]
fn counts_at_the_very_moment_the_reveal_window_closes() {
    let fixture = Fixture::new(2, 1);
    let result = fixture.tally_at(fixture.due());
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
}

/// Верхньої межі немає навмисно: підрахунок — дозвільний кранк, і спір, до
/// якого дійшли руки пізно, мусить дорахуватись, а не застрягнути.
#[test]
fn counts_however_late_the_crank_arrives() {
    let fixture = Fixture::new(2, 1);
    let result = fixture.tally_at(fixture.due() + 30 * 86_400);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
}

// ── стан спору ──────────────────────────────────────────────────────────────

/// Порахований спір не перераховують: другий підрахунок зсунув би вікно
/// апеляції, а на вже ескальованому — почав би третє коло.
#[test]
fn refuses_to_count_a_dispute_that_is_no_longer_voting() {
    for state in [
        DisputeState::OptimisticPending,
        DisputeState::Tallied,
        DisputeState::Appealed,
        DisputeState::Finalized,
    ] {
        let fixture = Fixture::build(2, 1, false, state);
        let result = fixture.tally();
        assert!(
            failed_with(&result, VerdictMeshError::WrongState),
            "{state:?} accepted a tally"
        );
    }
}

/// Спір, якому так і не зробили відбору панелі, лишається в `Committing` — і
/// мусить дорахуватись звідти, інакше він висить вічно.
#[test]
fn counts_a_dispute_still_sitting_in_committing() {
    let fixture = Fixture::build(0, 0, false, DisputeState::Committing);
    let result = fixture.tally();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
    assert!(fixture.after(&result).escalated);
}
