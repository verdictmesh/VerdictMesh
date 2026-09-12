//! T017 — подання відбитка голосу (`FR-008`, `FR-009`).
//!
//! Інструкція коротка, і саме тому її легко зробити нешкідливою на вигляд.
//! Перевіряється тут три речі, і кожна ламається тихо.
//!
//! **Голосує панель, а не бажаючі.** Відбиток від того, кого відбір не вибрав,
//! не падає ні на чому іншому: акаунт голосу виводиться з його ж гаманця й
//! чудово створюється. Порахувався б він аж у `tally` — тобто вердикт виніс би
//! хтось, кого до спору не допускали.
//!
//! **Вікно подання закривається.** Відбиток, поданий після дедлайну, — це голос,
//! поданий уже після того, як інші почали розкриватись. Механізм тримається на
//! тому, що всі фіксуються наосліп.
//!
//! **У стані немає голосу.** До розкриття акаунт не містить нічого, що
//! відрізняє один вибір від іншого — `FR-009` перевіряється тим, що два
//! протилежні голоси лишають записи, які різняться лише всередині відбитка.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::{program::keyed_account_for_system_program, result::InstructionResult, Mollusk};
use solana_account::Account;
use solana_address::Address;
use solana_instruction::Instruction;
use verdict_mesh::{
    events::VoteCommitted,
    state::{DisputeState, Verdict, VoteCommit},
    vote::{commitment_of, SALT_LEN},
    VerdictMeshError,
};

/// Панель демо-політики — троє. Четвертий гаманець тримається поруч навмисно:
/// це той, кого відбір не вибрав.
const PANEL: usize = 3;

struct Fixture {
    dispute: Pubkey,
    panel: Vec<Pubkey>,
    outsider: Pubkey,
    accounts: Vec<(Address, Account)>,
}

impl Fixture {
    fn new() -> Self {
        Self::build(DisputeState::Committing, PANEL)
    }

    /// Спір збирається зі стану, а не прогоном `open_dispute` + `select_panel`:
    /// тест про подання голосу не має падати через щось у відборі.
    fn build(state: DisputeState, panel_size: usize) -> Self {
        let authority = Pubkey::new_unique();
        let (integrator, _) = integrator_pda(&authority);
        let (dispute, bump) = dispute_pda(&integrator, 0);

        let panel: Vec<Pubkey> = (0..panel_size).map(|_| Pubkey::new_unique()).collect();
        let outsider = Pubkey::new_unique();

        let mut opened = dispute_state(&integrator, 0, &demo_policy(), bump);
        opened.state = state;
        opened.panel = panel.clone();

        let mut accounts = vec![
            (addr(&dispute), dispute_account(&opened)),
            keyed_account_for_system_program(),
        ];

        for key in panel.iter().chain(std::iter::once(&outsider)) {
            accounts.push((addr(key), wallet(10_000_000)));
            accounts.push((addr(&vote_pda(&dispute, key).0), missing()));
        }

        Self {
            dispute,
            panel,
            outsider,
            accounts,
        }
    }

    fn commitment_for(&self, juror: &Pubkey, choice: Verdict) -> [u8; 32] {
        commitment_of(&self.dispute, juror, choice, &[7u8; SALT_LEN])
    }

    fn ix(&self, juror: &Pubkey, commitment: [u8; 32]) -> Instruction {
        self.ix_with_vote(juror, &vote_pda(&self.dispute, juror).0, commitment)
    }

    fn ix_with_vote(&self, juror: &Pubkey, vote: &Pubkey, commitment: [u8; 32]) -> Instruction {
        anchor_ix(
            verdict_mesh::accounts::CommitVote {
                juror: *juror,
                dispute: self.dispute,
                vote: *vote,
                system_program: SYSTEM_PROGRAM,
            },
            verdict_mesh::instruction::CommitVote { commitment },
        )
    }

    fn commit(&self, juror: &Pubkey, choice: Verdict) -> InstructionResult {
        self.commit_at(juror, choice, NOW)
    }

    fn commit_at(&self, juror: &Pubkey, choice: Verdict, now: i64) -> InstructionResult {
        at(now).process_instruction(
            &self.ix(juror, self.commitment_for(juror, choice)),
            &self.accounts,
        )
    }

    fn deadline(&self) -> i64 {
        NOW + demo_policy().commit_window
    }
}

/// Mollusk із заданим «зараз». Годинник обв'язки стоїть на `NOW`, і тести вікна
/// мусять рухати його явно, а не покладатись на замовчування.
fn at(now: i64) -> Mollusk {
    let mut mollusk = mollusk();
    mollusk.sysvars.clock.unix_timestamp = now;
    mollusk
}

/// Стан голосу з результату виконання.
fn committed(result: &InstructionResult, dispute: &Pubkey, juror: &Pubkey) -> VoteCommit {
    decode(resulting(result, &vote_pda(dispute, juror).0))
}

// ── що записується ──────────────────────────────────────────────────────────

/// Відбиток лягає в акаунт рівно тим числом, яким його подали. Спотворення тут
/// не падає — воно робить голос нерозкривним, і присяжний втрачає частку стейку
/// за мовчання, якого не обирав (`FR-008b`).
#[test]
fn stores_the_commitment_of_a_panel_member_unchanged() {
    let fixture = Fixture::new();
    let juror = fixture.panel[0];
    let commitment = fixture.commitment_for(&juror, Verdict::Claimant);

    let result = fixture.commit(&juror, Verdict::Claimant);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let vote = committed(&result, &fixture.dispute, &juror);
    assert_eq!(vote.commitment, commitment);
    assert_eq!(vote.dispute, fixture.dispute);
    assert_eq!(vote.juror, juror);
}

/// `FR-009` у стані: до розкриття вибору в акаунті немає. Порожній `choice` —
/// це ще й єдина ознака, за якою T020 відрізнить того, хто змовчав.
#[test]
fn leaves_the_choice_unset_until_the_reveal() {
    let fixture = Fixture::new();
    let result = fixture.commit(&fixture.panel[0], Verdict::Claimant);

    assert!(committed(&result, &fixture.dispute, &fixture.panel[0])
        .choice
        .is_none());
}

/// `FR-009` як його бачить сторонній спостерігач: два протилежні голоси
/// лишають записи однакової довжини, що різняться **лише** всередині відбитка.
/// Якби вибір десь просочився в стан — у прапорець, у довжину, у порядок полів —
/// цей тест побачив би різницю поза межами хеша.
#[test]
fn the_stored_record_looks_the_same_whatever_the_choice() {
    let fixture = Fixture::new();
    let juror = fixture.panel[0];

    let claimant = fixture.commit(&juror, Verdict::Claimant);
    let respondent = fixture.commit(&juror, Verdict::Respondent);

    let vote = vote_pda(&fixture.dispute, &juror).0;
    let left = &resulting(&claimant, &vote).data;
    let right = &resulting(&respondent, &vote).data;
    assert_eq!(left.len(), right.len());

    let differing: Vec<usize> = left
        .iter()
        .zip(right.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(position, _)| position)
        .collect();

    // Відбиток — 32 байти після дискримінатора, `dispute` і `juror`.
    let commitment = 8 + 32 + 32..8 + 32 + 32 + 32;
    assert!(
        !differing.is_empty(),
        "the commitment did not change at all"
    );
    assert!(
        differing
            .iter()
            .all(|position| commitment.contains(position)),
        "the choice leaks outside the commitment: {differing:?}"
    );
}

/// `FR-029`: подія є, і в ній немає нічого, крім спору й присяжного. Watcher
/// (T027) будує з неї список тих, хто вже зафіксувався, не знаючи їхніх голосів.
#[test]
fn announces_the_commitment_without_saying_anything_about_it() {
    let fixture = Fixture::new();
    let juror = fixture.panel[0];

    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = NOW;
    let result = mollusk.process_instruction(
        &fixture.ix(&juror, fixture.commitment_for(&juror, Verdict::Claimant)),
        &fixture.accounts,
    );
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<VoteCommitted> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].dispute, fixture.dispute);
    assert_eq!(events[0].juror, juror);
}

/// Присяжні фіксуються незалежно один від одного: кожен у свій акаунт, і жоден
/// не заважає іншому.
#[test]
fn every_panel_member_has_a_commitment_of_their_own() {
    let fixture = Fixture::new();

    for juror in &fixture.panel {
        let result = fixture.commit(juror, Verdict::Respondent);
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
        assert_eq!(
            committed(&result, &fixture.dispute, juror).commitment,
            fixture.commitment_for(juror, Verdict::Respondent),
        );
    }
}

// ── хто має право ───────────────────────────────────────────────────────────

/// Головний тест файлу. Той, кого відбір не вибрав, має бути відхилений саме
/// тут: далі по стан-машині його голос уже нічим не відрізнити від законного.
#[test]
fn refuses_a_signer_that_is_not_on_the_panel() {
    let fixture = Fixture::new();
    let result = fixture.commit(&fixture.outsider, Verdict::Claimant);
    assert!(failed_with(&result, VerdictMeshError::NotOnPanel));
}

/// Спір, якому відбір так і не зробили, має порожню панель — і голосувати в
/// ньому не може ніхто. Це та сама деградація, про яку попереджає T016: спір
/// дійде до `tally` без жодного голосу, а не збереться з випадкових охочих.
#[test]
fn refuses_everyone_while_the_panel_is_empty() {
    let fixture = Fixture::build(DisputeState::Committing, 0);
    let result = fixture.commit(&fixture.outsider, Verdict::Claimant);
    assert!(failed_with(&result, VerdictMeshError::NotOnPanel));
}

/// Акаунт голосу прив'язаний до пари «спір + присяжний». Підставивши чужу
/// адресу, можна було б записати відбиток у чужий акаунт — і розкрити його
/// потім своїм секретом не зміг би вже ніхто.
#[test]
fn refuses_a_vote_account_that_belongs_to_another_juror() {
    let fixture = Fixture::new();
    let juror = fixture.panel[0];
    let stolen = vote_pda(&fixture.dispute, &fixture.panel[1]).0;

    let result = mollusk().process_instruction(
        &fixture.ix_with_vote(
            &juror,
            &stolen,
            fixture.commitment_for(&juror, Verdict::Claimant),
        ),
        &fixture.accounts,
    );
    assert!(result.program_result.is_err());
}

/// Відбиток подається один раз. Друге подання не «оновлює голос» — воно
/// створює другий акаунт за тією ж адресою, тобто не створює нічого; впасти має
/// вголос, а не переписати вже подане.
#[test]
fn refuses_a_second_commitment_from_the_same_juror() {
    let fixture = Fixture::new();
    let juror = fixture.panel[0];

    let first = fixture.commit(&juror, Verdict::Claimant);
    assert!(first.program_result.is_ok(), "{:?}", first.raw_result);

    // Акаунт голосу вже існує — рівно те, що побачить друга транзакція.
    let mut accounts = fixture.accounts.clone();
    let vote = vote_pda(&fixture.dispute, &juror).0;
    replace(&mut accounts, &vote, resulting(&first, &vote).clone());

    let result = at(NOW).process_instruction(
        &fixture.ix(&juror, fixture.commitment_for(&juror, Verdict::Respondent)),
        &accounts,
    );
    assert!(result.program_result.is_err());
}

// ── вікно подання ───────────────────────────────────────────────────────────

/// Остання секунда вікна ще належить присяжному.
#[test]
fn accepts_a_commitment_in_the_last_second_of_the_window() {
    let fixture = Fixture::new();
    let result = fixture.commit_at(&fixture.panel[0], Verdict::Claimant, fixture.deadline() - 1);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
}

/// Сам дедлайн — уже за межею. Інакше вікно подання на секунду перекривається з
/// розкриттям, і цієї секунди досить, щоб подати відбиток, побачивши чужий
/// голос.
#[test]
fn refuses_a_commitment_at_the_deadline() {
    let fixture = Fixture::new();
    let result = fixture.commit_at(&fixture.panel[0], Verdict::Claimant, fixture.deadline());
    assert!(failed_with(&result, VerdictMeshError::WindowClosed));
}

#[test]
fn refuses_a_commitment_long_after_the_window() {
    let fixture = Fixture::new();
    let result = fixture.commit_at(
        &fixture.panel[0],
        Verdict::Claimant,
        fixture.deadline() + 86_400,
    );
    assert!(failed_with(&result, VerdictMeshError::WindowClosed));
}

// ── стан спору ──────────────────────────────────────────────────────────────

/// Подання живе рівно в одному стані. Кожен інший — це або спір, який ще не
/// дійшов до панелі, або той, що вже пішов далі; в обох випадках новий відбиток
/// не має куди врахуватись.
#[test]
fn refuses_a_commitment_in_every_state_but_committing() {
    for state in [
        DisputeState::OptimisticPending,
        DisputeState::Revealing,
        DisputeState::Tallied,
        DisputeState::Appealed,
        DisputeState::Finalized,
    ] {
        let fixture = Fixture::build(state, PANEL);
        let result = fixture.commit(&fixture.panel[0], Verdict::Claimant);
        assert!(
            failed_with(&result, VerdictMeshError::WrongState),
            "{state:?} accepted a commitment"
        );
    }
}

/// Перевірка стану не ховається за перевіркою вікна: спір уже в `Revealing`,
/// але час іще всередині вікна подання — відмова все одно приходить за станом.
#[test]
fn a_revealing_dispute_is_refused_even_inside_the_commit_window() {
    let fixture = Fixture::build(DisputeState::Revealing, PANEL);
    let result = fixture.commit_at(&fixture.panel[0], Verdict::Claimant, NOW + 1);
    assert!(failed_with(&result, VerdictMeshError::WrongState));
}
