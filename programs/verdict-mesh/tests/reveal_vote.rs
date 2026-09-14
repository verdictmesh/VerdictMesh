//! T018 — розкриття голосу (`FR-008a`).
//!
//! Друга половина commit-reveal, і саме та, що робить першу небезсенсовною.
//! Відбиток сам по собі не зобов'язує ні до чого: доводить, що присяжний
//! розкрив **той** голос, який фіксував, лише звірка тут.
//!
//! **Розбіжність відхиляється, а не перезаписується.** Секрет не той, вибір не
//! той, відбиток порахований для іншого спору чи іншого гаманця — усе це один
//! і той самий випадок: показане не є тим, що подавали.
//!
//! **Копія чужого відбитка марна.** Це перевіряється тут наскрізно, а не лише
//! юніт-тестом формули: присяжний, який поклав до себе чужий `commitment`, не
//! розкриє його навіть тоді, коли автор уже показав світові і вибір, і секрет.
//!
//! **Вікно розкриття не перекривається з вікном подання.** Розкритись раніше,
//! ніж закрилось подання, означає показати свій голос тим, хто ще не
//! зафіксувався — рівно те, чого `FR-009` не дозволяє.

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
    events::VoteRevealed,
    state::{Ballot, Dispute, DisputeState, Policy, VoteCommit},
    vote::{commitment_of, SALT_LEN},
    VerdictMeshError,
};

const PANEL: usize = 3;

/// Секрет присяжного зі слота `index`. Різні секрети в різних присяжних — щоб
/// тест на копіювання не проходив випадково.
fn salt(index: usize) -> [u8; SALT_LEN] {
    [index as u8 + 1; SALT_LEN]
}

struct Fixture {
    dispute: Pubkey,
    panel: Vec<Pubkey>,
    accounts: Vec<(Address, Account)>,
    policy: Policy,
}

impl Fixture {
    /// Панель, у якій усі троє вже подали відбіток свого голосу. Спір іще в
    /// `Committing`: перехід у `Revealing` робить перше ж розкриття, і тест про
    /// цей перехід має бачити стан до нього.
    fn new(choices: [Ballot; PANEL]) -> Self {
        Self::build(choices, DisputeState::Committing)
    }

    fn build(choices: [Ballot; PANEL], state: DisputeState) -> Self {
        let policy = demo_policy();
        let authority = Pubkey::new_unique();
        let (integrator, _) = integrator_pda(&authority);
        let (dispute, bump) = dispute_pda(&integrator, 0);

        let panel: Vec<Pubkey> = (0..PANEL).map(|_| Pubkey::new_unique()).collect();

        let mut opened = dispute_state(&integrator, 0, &policy, bump);
        opened.state = state;
        opened.panel = panel.clone();

        let mut accounts = vec![(addr(&dispute), dispute_account(&opened))];

        for (index, juror) in panel.iter().enumerate() {
            accounts.push((addr(juror), wallet(10_000_000)));
            accounts.push((
                addr(&vote_pda(&dispute, juror).0),
                committed_account(&dispute, juror, choices[index], &salt(index)),
            ));
        }

        Self {
            dispute,
            panel,
            accounts,
            policy,
        }
    }

    /// Момент усередині вікна розкриття: подання вже закрилось, розкриття ще ні.
    fn revealing(&self) -> i64 {
        NOW + self.policy.commit_window
    }

    fn reveal_deadline(&self) -> i64 {
        NOW + self.policy.commit_window + self.policy.reveal_window
    }

    fn ix(&self, index: usize, choice: Ballot, secret: [u8; SALT_LEN]) -> Instruction {
        anchor_ix(
            verdict_mesh::accounts::RevealVote {
                juror: self.panel[index],
                dispute: self.dispute,
                vote: vote_pda(&self.dispute, &self.panel[index]).0,
            },
            verdict_mesh::instruction::RevealVote {
                choice,
                salt: secret,
            },
        )
    }

    /// Розкриття «як задумано»: тим вибором і тим секретом, з яких зроблено
    /// відбиток.
    fn reveal(&self, index: usize, choice: Ballot) -> InstructionResult {
        self.reveal_at(index, choice, salt(index), self.revealing())
    }

    fn reveal_at(
        &self,
        index: usize,
        choice: Ballot,
        secret: [u8; SALT_LEN],
        now: i64,
    ) -> InstructionResult {
        mollusk_at(now).process_instruction(&self.ix(index, choice, secret), &self.accounts)
    }

    /// Наслідок виконання лягає назад у список акаунтів — так кілька розкриттів
    /// в одному тесті бачать одне одного. Без цього лічильники щоразу
    /// починалися б з нуля, і тест на накопичення нічого б не накопичував.
    fn apply(&mut self, result: &InstructionResult) {
        let dispute = resulting(result, &self.dispute).clone();
        replace(&mut self.accounts, &self.dispute, dispute);
    }

    fn dispute_after(&self, result: &InstructionResult) -> Dispute {
        decode(resulting(result, &self.dispute))
    }

    fn vote_after(&self, result: &InstructionResult, index: usize) -> VoteCommit {
        decode(resulting(
            result,
            &vote_pda(&self.dispute, &self.panel[index]).0,
        ))
    }
}

/// Акаунт голосу в тому вигляді, у якому його лишає `commit_vote`.
fn committed_account(
    dispute: &Pubkey,
    juror: &Pubkey,
    choice: Ballot,
    secret: &[u8; SALT_LEN],
) -> Account {
    program_account(&VoteCommit {
        dispute: *dispute,
        juror: *juror,
        commitment: commitment_of(dispute, juror, choice, secret),
        choice: None,
        bump: vote_pda(dispute, juror).1,
    })
}

// ── звірка ──────────────────────────────────────────────────────────────────

/// Голос, розкритий тим самим вибором і тим самим секретом, приймається і лягає
/// в акаунт.
#[test]
fn records_a_vote_that_matches_its_commitment() {
    let fixture = Fixture::new([Ballot::Claimant; PANEL]);
    let result = fixture.reveal(0, Ballot::Claimant);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    assert_eq!(
        fixture.vote_after(&result, 0).choice,
        Some(Ballot::Claimant)
    );
}

/// Головна перевірка файлу. Чужий секрет не відмикає власний відбиток — без
/// цього подання не зобов'язувало б ні до чого, і голос можна було б обрати
/// після того, як стали видні чужі.
#[test]
fn refuses_a_vote_revealed_with_the_wrong_secret() {
    let fixture = Fixture::new([Ballot::Claimant; PANEL]);
    let result = fixture.reveal_at(0, Ballot::Claimant, salt(9), fixture.revealing());
    assert!(failed_with(&result, VerdictMeshError::CommitmentMismatch));
}

/// Той самий секрет, інший вибір — теж розбіжність. Це і є `FR-008a` дослівно:
/// «розкритий голос, що не відповідає відбитку, не враховується».
#[test]
fn refuses_a_vote_revealed_as_the_other_choice() {
    let fixture = Fixture::new([Ballot::Claimant; PANEL]);
    let result = fixture.reveal(0, Ballot::Respondent);
    assert!(failed_with(&result, VerdictMeshError::CommitmentMismatch));
}

/// Копія чужого відбитка марна, і це видно наскрізь, а не лише у формулі.
/// Присяжний поклав до себе `commitment` сусіда — і не розкриває його навіть
/// тим самим вибором і тим самим секретом, які сусід оприлюднить хвилиною
/// пізніше. Без прив'язки до гаманця це був би голос без роботи й без ризику.
#[test]
fn refuses_a_commitment_copied_from_another_juror() {
    let mut fixture = Fixture::new([Ballot::Claimant; PANEL]);
    let author = fixture.panel[0];
    let copycat = fixture.panel[1];

    // Копіювальник підклав собі чужий відбиток ще у вікні подання.
    let stolen = program_account(&VoteCommit {
        dispute: fixture.dispute,
        juror: copycat,
        commitment: commitment_of(&fixture.dispute, &author, Ballot::Claimant, &salt(0)),
        choice: None,
        bump: vote_pda(&fixture.dispute, &copycat).1,
    });
    replace(
        &mut fixture.accounts,
        &vote_pda(&fixture.dispute, &copycat).0,
        stolen,
    );

    // Автор розкрився — вибір і секрет тепер публічні.
    assert!(fixture.reveal(0, Ballot::Claimant).program_result.is_ok());

    let result = fixture.reveal_at(1, Ballot::Claimant, salt(0), fixture.revealing());
    assert!(failed_with(&result, VerdictMeshError::CommitmentMismatch));
}

/// Секрет, розкритий в одному спорі, не відмикає голос у іншому: присяжний, що
/// сидить у кількох панелях, не роздає своїм першим розкриттям усі наступні.
#[test]
fn refuses_a_commitment_computed_for_another_dispute() {
    let mut fixture = Fixture::new([Ballot::Claimant; PANEL]);
    let juror = fixture.panel[0];
    let elsewhere = Pubkey::new_unique();

    let foreign = program_account(&VoteCommit {
        dispute: fixture.dispute,
        juror,
        commitment: commitment_of(&elsewhere, &juror, Ballot::Claimant, &salt(0)),
        choice: None,
        bump: vote_pda(&fixture.dispute, &juror).1,
    });
    replace(
        &mut fixture.accounts,
        &vote_pda(&fixture.dispute, &juror).0,
        foreign,
    );

    let result = fixture.reveal(0, Ballot::Claimant);
    assert!(failed_with(&result, VerdictMeshError::CommitmentMismatch));
}

/// Розкриття одноразове. Друге порахувало б той самий голос двічі — і панель із
/// трьох дала б кворум силами одного присяжного.
#[test]
fn refuses_to_reveal_the_same_vote_twice() {
    let mut fixture = Fixture::new([Ballot::Claimant; PANEL]);

    let first = fixture.reveal(0, Ballot::Claimant);
    assert!(first.program_result.is_ok(), "{:?}", first.raw_result);

    let vote = vote_pda(&fixture.dispute, &fixture.panel[0]).0;
    let revealed = resulting(&first, &vote).clone();
    fixture.apply(&first);
    replace(&mut fixture.accounts, &vote, revealed);

    let result = fixture.reveal(0, Ballot::Claimant);
    assert!(failed_with(&result, VerdictMeshError::AlreadyRevealed));
}

/// Акаунт голосу прив'язаний до підписанта. Розкрити чужий голос своїм підписом
/// не можна — інакше сусід оприлюднював би відбиток за присяжного, який хотів
/// змовчати, або навпаки.
#[test]
fn refuses_to_reveal_someone_elses_vote() {
    let fixture = Fixture::new([Ballot::Claimant; PANEL]);

    let hijacked = anchor_ix(
        verdict_mesh::accounts::RevealVote {
            juror: fixture.panel[1],
            dispute: fixture.dispute,
            vote: vote_pda(&fixture.dispute, &fixture.panel[0]).0,
        },
        verdict_mesh::instruction::RevealVote {
            choice: Ballot::Claimant,
            salt: salt(0),
        },
    );

    let result = mollusk_at(fixture.revealing()).process_instruction(&hijacked, &fixture.accounts);
    assert!(result.program_result.is_err());
}

// ── підрахунок ──────────────────────────────────────────────────────────────

/// Голос лягає у свій лічильник. Помилка тут не падає — вона віддає спір іншій
/// стороні.
#[test]
fn counts_the_vote_for_the_side_it_names() {
    for (choice, claimant, respondent) in [(Ballot::Claimant, 1, 0), (Ballot::Respondent, 0, 1)] {
        let fixture = Fixture::new([choice; PANEL]);
        let result = fixture.reveal(0, choice);
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

        let dispute = fixture.dispute_after(&result);
        assert_eq!(dispute.votes_claimant, claimant, "{choice:?}");
        assert_eq!(dispute.votes_respondent, respondent, "{choice:?}");
    }
}

/// Розкриття накопичуються: троє присяжних дають три голоси, розкладені за
/// сторонами. Прогін послідовний — кожне наступне бачить наслідки попереднього.
#[test]
fn accumulates_the_votes_of_the_whole_panel() {
    let choices = [Ballot::Claimant, Ballot::Respondent, Ballot::Claimant];
    let mut fixture = Fixture::new(choices);

    let mut last = None;
    for (index, choice) in choices.iter().enumerate() {
        let result = fixture.reveal(index, *choice);
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
        fixture.apply(&result);
        last = Some(result);
    }

    let dispute = fixture.dispute_after(&last.expect("the panel revealed"));
    assert_eq!(dispute.votes_claimant, 2);
    assert_eq!(dispute.votes_respondent, 1);
}

/// Перше розкриття переводить спір у `Revealing`. Стан — не прикраса: саме він
/// тримає межу, за якою подання відбитків уже неможливе.
#[test]
fn moves_the_dispute_into_revealing_on_the_first_reveal() {
    let fixture = Fixture::new([Ballot::Claimant; PANEL]);
    let result = fixture.reveal(0, Ballot::Claimant);

    assert_eq!(
        fixture.dispute_after(&result).state,
        DisputeState::Revealing
    );
}

/// `FR-029`: подія несе вже розкритий вибір — ховати його після розкриття немає
/// від кого, а watcher (T027) будує з неї хід голосування.
#[test]
fn announces_the_revealed_vote() {
    let fixture = Fixture::new([Ballot::Respondent; PANEL]);

    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = fixture.revealing();
    let result = mollusk.process_instruction(
        &fixture.ix(0, Ballot::Respondent, salt(0)),
        &fixture.accounts,
    );
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<VoteRevealed> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].dispute, fixture.dispute);
    assert_eq!(events[0].juror, fixture.panel[0]);
    assert_eq!(events[0].choice, Ballot::Respondent);
}

// ── вікно розкриття ─────────────────────────────────────────────────────────

/// `FR-009` на межі. Розкритись за секунду до кінця подання означає показати
/// свій голос тим, хто ще не зафіксувався.
#[test]
fn refuses_a_reveal_before_the_commit_window_closes() {
    let fixture = Fixture::new([Ballot::Claimant; PANEL]);
    let result = fixture.reveal_at(0, Ballot::Claimant, salt(0), fixture.revealing() - 1);
    assert!(failed_with(&result, VerdictMeshError::WindowOpen));
}

/// Дедлайн подання — перша мить розкриття. Вікна стикаються, а не
/// перекриваються і не лишають щілини, у якій не можна ні те, ні те.
#[test]
fn accepts_a_reveal_at_the_very_moment_the_commit_window_closes() {
    let fixture = Fixture::new([Ballot::Claimant; PANEL]);
    let result = fixture.reveal_at(0, Ballot::Claimant, salt(0), fixture.revealing());
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
}

#[test]
fn accepts_a_reveal_in_the_last_second_of_the_window() {
    let fixture = Fixture::new([Ballot::Claimant; PANEL]);
    let result = fixture.reveal_at(0, Ballot::Claimant, salt(0), fixture.reveal_deadline() - 1);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
}

/// Після дедлайну голос не враховується, а присяжний вважається таким, що
/// змовчав (`FR-008b`). Приймати запізніле розкриття означало б рахувати голос,
/// поданий уже після того, як підрахунок міг початись.
#[test]
fn refuses_a_reveal_at_the_deadline() {
    let fixture = Fixture::new([Ballot::Claimant; PANEL]);
    let result = fixture.reveal_at(0, Ballot::Claimant, salt(0), fixture.reveal_deadline());
    assert!(failed_with(&result, VerdictMeshError::WindowClosed));
}

// ── стан спору ──────────────────────────────────────────────────────────────

/// Розкриття живе у двох станах: `Committing` (перше розкриття, воно ж і
/// переводить спір далі) і `Revealing`. Решта — це спір, який уже порахували.
#[test]
fn refuses_a_reveal_once_the_dispute_has_moved_on() {
    for state in [
        DisputeState::OptimisticPending,
        DisputeState::Tallied,
        DisputeState::Appealed,
        DisputeState::Finalized,
    ] {
        let fixture = Fixture::build([Ballot::Claimant; PANEL], state);
        let result = fixture.reveal(0, Ballot::Claimant);
        assert!(
            failed_with(&result, VerdictMeshError::WrongState),
            "{state:?} accepted a reveal"
        );
    }
}

#[test]
fn accepts_a_reveal_while_the_dispute_is_already_revealing() {
    let fixture = Fixture::build([Ballot::Claimant; PANEL], DisputeState::Revealing);
    let result = fixture.reveal(0, Ballot::Claimant);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
}
