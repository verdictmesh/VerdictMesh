//! T024 — `FR-014`: доказ відсутності.
//!
//! **Цей файл нічого не перевіряє — він доводить, що чогось немає.** `FR-014`
//! каже, що жоден ключ адміністратора не може змінити вердикт, перевизначити
//! голос чи вилучити кошти з ескроу інтегратора. Вимогу такого роду не можна
//! показати вдалим прогоном: успішний тест доводить, що щось працює, а тут
//! треба довести, що чогось **не існує**. Єдиний спосіб — перелічити всю
//! поверхню програми і зафіксувати її переліком, який червоніє від будь-якого
//! додавання.
//!
//! **Реєстром служить IDL, а не список у голові.** Він генерується зі
//! справжнього коду при `anchor build`, тож інструкція, дописана й забута, у
//! ньому з'явиться, а в переліку нижче — ні. Тест, який просто викликав би
//! десять відомих інструкцій і переконався, що вони поводяться добре, про
//! одинадцяту не сказав би нічого — і саме одинадцята була б тією, заради якої
//! `FR-014` написана.
//!
//! **Друга половина — поведінкова.** Перелік показує, що інструкції для обходу
//! немає; прогін показує, що й наявні нічого не дають тому, хто спробує. Ключем
//! нападника скрізь виступає `reporter` — єдиний привілейований ключ у системі
//! (`FR-017a`), тобто найкращий кандидат на «адміністратора», якого `FR-014`
//! згадує.

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
    panel::SLOT_HASHES_ID,
    state::{Ballot, Dispute, DisputeState, JurorRegistry, Verdict, VoteCommit},
    VerdictMeshError,
};

// ── поверхня програми ───────────────────────────────────────────────────────

/// IDL зі збірки — той самий артефакт, що йде інтеграторам. Читається як файл,
/// а не як вбудований рядок: вбудований довелося б оновлювати руками, тобто
/// саме тією дією, яку цей файл і має ловити.
fn idl() -> serde_json::Value {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/idl/verdict_mesh.json");

    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}. Run `anchor build` (WSL) before the program tests.",
            path.display()
        )
    });

    serde_json::from_str(&text).expect("the IDL must be valid JSON")
}

fn instructions() -> Vec<serde_json::Value> {
    idl()["instructions"]
        .as_array()
        .expect("the IDL must list its instructions")
        .clone()
}

fn instruction(name: &str) -> serde_json::Value {
    instructions()
        .into_iter()
        .find(|entry| entry["name"] == name)
        .unwrap_or_else(|| panic!("the program no longer has an instruction named {name}"))
}

/// Акаунти інструкції, які мають названу ознаку — `signer` або `writable`.
fn accounts_marked(name: &str, mark: &str) -> Vec<String> {
    instruction(name)["accounts"]
        .as_array()
        .expect("an instruction must list its accounts")
        .iter()
        .filter(|account| account[mark] == serde_json::Value::Bool(true))
        .map(|account| account["name"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// Уся програма, як її бачить той, хто підключається. Перелік навмисно
/// хардкоджений: він і є твердженням.
const SURFACE: [&str; 10] = [
    "commit_vote",
    "initialize",
    "open_dispute",
    "register_integrator",
    "reveal_vote",
    "select_panel",
    "settle_stakes",
    "stake",
    "tally",
    "unstake",
];

/// Одинадцятої інструкції не існує. Якщо цей тест червоний — хтось додав до
/// програми інструкцію, і питання «чи не дає вона комусь влади над вердиктом»
/// має бути поставлене вголос, а не закрите правкою переліку.
#[test]
fn offers_exactly_these_ten_instructions_and_nothing_else() {
    let mut names: Vec<String> = instructions()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    names.sort();

    assert_eq!(names, SURFACE);
}

/// Хто взагалі має право щось підписати — повний перелік на всю програму.
/// Ролі в ньому рівно три: власник власних коштів (`juror`, `depositor`,
/// `authority`), той, хто платить оренду (`payer`, `crank`), і чужа програма,
/// що відкриває спір над **своїм** ескроу (`escrow`). Адміністратора немає
/// жодного — не тому, що йому нічого не дозволено, а тому, що його ніде немає.
const SIGNATURES: [(&str, &[&str]); 10] = [
    ("commit_vote", &["juror"]),
    ("initialize", &["payer"]),
    ("open_dispute", &["payer", "depositor", "escrow"]),
    ("register_integrator", &["authority"]),
    ("reveal_vote", &["juror"]),
    ("select_panel", &[]),
    ("settle_stakes", &["crank"]),
    ("stake", &["juror"]),
    ("tally", &[]),
    ("unstake", &["juror"]),
];

#[test]
fn asks_for_exactly_these_signatures_and_nothing_else() {
    for (name, expected) in SIGNATURES {
        assert_eq!(accounts_marked(name, "signer"), expected, "{name}");
    }
}

/// Вердикт нізвідки не приходить ззовні: його не можна ані передати
/// аргументом, ані назвати акаунтом. `reveal_vote` приймає `Ballot` — але це
/// голос присяжного про себе, а не вердикт спору, і подати його може лише той,
/// чий гаманець уже входить у відбиток (`FR-008a`).
#[test]
fn never_takes_a_verdict_from_whoever_calls_it() {
    for entry in instructions() {
        let args = entry["args"].as_array().expect("args must be a list");
        let name = entry["name"].as_str().unwrap_or_default();

        for arg in args {
            assert_ne!(
                arg["type"]["defined"]["name"], "Verdict",
                "{name} takes a verdict from the caller"
            );
            assert_ne!(arg["name"], "verdict", "{name} takes a verdict argument");
        }
    }
}

/// Спір, у якого вже є вердикт, лишається доступним рівно трьом інструкціям —
/// і жодна з них не приймає **жодного** аргументу. Тобто той, хто викликає,
/// не передає в такий спір нічого: результат є функцією стану, а не того, хто
/// прийшов. `open_dispute` і `commit_vote` аргументи мають, але до вирішеного
/// спору не дістають (див. поведінкову частину нижче).
#[test]
fn leaves_the_caller_nothing_to_say_about_a_dispute_that_is_already_decided() {
    for name in ["select_panel", "tally", "settle_stakes"] {
        let args = instruction(name)["args"]
            .as_array()
            .expect("args must be a list")
            .clone();

        assert!(args.is_empty(), "{name} takes {} arguments", args.len());
    }
}

/// `Config` — єдине місце, де записані привілейовані ключі протоколу
/// (`reporter`, `treasury`) і розрахунковий актив. Він створюється один раз і
/// більше не приймає запису: інструкції, здатної переставити reporter посеред
/// спору чи перевести на себе комісію, у програмі немає.
#[test]
fn writes_the_protocol_config_only_at_the_moment_it_is_created() {
    let writers: Vec<&str> = SURFACE
        .into_iter()
        .filter(|name| {
            accounts_marked(name, "writable")
                .iter()
                .any(|a| a == "config")
        })
        .collect();

    assert_eq!(writers, ["initialize"]);
}

/// Ескроу інтегратора видно програмі рівно один раз — коли він сам відкриває
/// спір власним підписом, — і **тільки на читання**. Права записати в нього
/// програма не просить ніде, тож «вилучити кошти з ескроу» неможливо не за
/// правилом, а за списком акаунтів: інструкції, якій цей акаунт віддали б на
/// запис, не існує.
#[test]
fn never_asks_to_write_into_the_escrow_it_arbitrates_for() {
    let mentions: Vec<&str> = SURFACE
        .into_iter()
        .filter(|name| {
            instruction(name)["accounts"]
                .as_array()
                .expect("accounts must be a list")
                .iter()
                .any(|account| account["name"] == "escrow")
        })
        .collect();

    assert_eq!(mentions, ["open_dispute"]);
    assert!(!accounts_marked("open_dispute", "writable").contains(&"escrow".to_owned()));
}

// ── спроби обійти вердикт ключем ────────────────────────────────────────────

const DISPUTE_ID: u64 = 0;

/// Вердикт, який намагаються перевернути. Береться на боці ініціатора, щоб
/// «перевернути» означало щось конкретне: якби спроби вдались, вердикт став би
/// протилежним або зник.
const DECIDED: Verdict = Verdict::Claimant;

/// Спір, який уже вирішено, і ключ reporter, яким його пробують переграти.
struct Decided {
    reporter: Pubkey,
    /// Ключ без відбитка голосу. Потрібен окремо, бо адреса акаунта голосу
    /// виводиться з гаманця: у reporter він у фікстурі вже є — інакше
    /// розкриватись було б нічим, — а подання відбитка вимагає, щоб акаунта ще
    /// не існувало.
    newcomer: Pubkey,
    dispute: Pubkey,
    accounts: Vec<(Address, Account)>,
}

impl Decided {
    fn new() -> Self {
        let reporter = Pubkey::new_unique();
        let newcomer = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let authority = Pubkey::new_unique();

        let (integrator, _) = integrator_pda(&authority);
        let (dispute, bump) = dispute_pda(&integrator, DISPUTE_ID);
        let (registry, registry_bump) = registry_pda();
        let (vote, vote_bump) = vote_pda(&dispute, &reporter);

        let policy = demo_policy();
        let mut decided = dispute_state(&integrator, DISPUTE_ID, &policy, bump);
        decided.state = DisputeState::Tallied;
        decided.verdict = Some(DECIDED);
        decided.votes_claimant = 2;
        decided.votes_respondent = 1;
        decided.appeal_deadline = NOW + policy.appeal_window;

        let accounts = vec![
            (addr(&reporter), wallet(10_000_000_000)),
            (addr(&newcomer), wallet(10_000_000_000)),
            (addr(&vote_pda(&dispute, &newcomer).0), missing()),
            (addr(&dispute), dispute_account(&decided)),
            (
                addr(&config_pda().0),
                config_account(&Pubkey::new_unique(), &reporter, &treasury),
            ),
            (
                addr(&registry),
                program_account(&JurorRegistry {
                    juror_count: 0,
                    bump: registry_bump,
                }),
            ),
            // Відбиток голосу, виписаний на ключ reporter. На панелі його немає
            // — і саме тому спроба розкритись ним має бути відхилена станом
            // спору, а не відсутністю акаунта.
            (
                addr(&vote),
                program_account(&VoteCommit {
                    dispute,
                    juror: reporter,
                    commitment: [7u8; 32],
                    choice: None,
                    round: 0,
                    bump: vote_bump,
                }),
            ),
            keyed_account_for_slot_hashes(&mollusk()),
            mollusk_svm::program::keyed_account_for_system_program(),
        ];

        Self {
            reporter,
            newcomer,
            dispute,
            accounts,
        }
    }

    fn vote(&self) -> Pubkey {
        vote_pda(&self.dispute, &self.reporter).0
    }

    fn commit_attempt(&self) -> Instruction {
        anchor_ix(
            verdict_mesh::accounts::CommitVote {
                juror: self.newcomer,
                dispute: self.dispute,
                vote: vote_pda(&self.dispute, &self.newcomer).0,
                system_program: SYSTEM_PROGRAM,
            },
            verdict_mesh::instruction::CommitVote {
                commitment: [7u8; 32],
            },
        )
    }

    /// Прогін у момент, коли вікна подання і розкриття ще відкриті. Інакше
    /// відмова прийшла б від годинника, і тест доводив би не те: що спізнився,
    /// а не що спір уже вирішено.
    fn attempt(&self, ix: &Instruction) -> InstructionResult {
        mollusk_at(NOW).process_instruction(ix, &self.accounts)
    }

    fn verdict_after(&self, result: &InstructionResult) -> Option<Verdict> {
        decode::<Dispute>(resulting(result, &self.dispute)).verdict
    }
}

/// Панель, що винесла вердикт, не переграється. Це найтихіша зі спроб: вердикт
/// вона не чіпає взагалі, зате міняє **склад тих, хто його виніс**, — а за ним
/// іде слешинг (T020). Панель, підмінена після підрахунку, покарала б чужих
/// присяжних за голоси, яких вони не подавали.
#[test]
fn refuses_to_redraw_the_panel_that_reached_the_verdict() {
    let decided = Decided::new();

    let ix = anchor_ix(
        verdict_mesh::accounts::SelectPanel {
            dispute: decided.dispute,
            registry: registry_pda().0,
            slot_hashes: SLOT_HASHES_ID,
        },
        verdict_mesh::instruction::SelectPanel {},
    );

    let result = decided.attempt(&ix);
    assert!(failed_with(&result, VerdictMeshError::WrongState));
}

/// Підрахунок не повторюється. `tally` — єдина інструкція, яка взагалі пише
/// `verdict`, і другий її виклик відхиляється станом: переставити оголошений
/// вердикт нема чим і нікому, бо підпису вона не бере в жодного ключа.
#[test]
fn refuses_to_count_the_same_dispute_into_a_second_verdict() {
    let decided = Decided::new();

    let ix = anchor_ix(
        verdict_mesh::accounts::Tally {
            dispute: decided.dispute,
        },
        verdict_mesh::instruction::Tally {},
    );

    let result = decided.attempt(&ix);
    assert!(failed_with(&result, VerdictMeshError::WrongState));
}

/// Голос не дописується заднім числом — і не тим, хто прийшов уже після
/// підрахунку. Відбиток, поданий у цю мить, був би голосом, поданим із уже
/// відомим результатом; саме тому вікно подання перевіряється не тільки
/// годинником, а й станом спору.
#[test]
fn refuses_a_vote_sealed_after_the_verdict_was_announced() {
    let decided = Decided::new();
    let ix = decided.commit_attempt();

    let result = decided.attempt(&ix);
    assert!(failed_with(&result, VerdictMeshError::WrongState));
}

/// Розкриття після підрахунку теж відхиляється. Інакше ключ, який тримає
/// відбиток, вирішував би вже **після** оголошення, оприлюднювати голос чи ні,
/// — і кожен такий голос зсував би лічильники вирішеного спору.
#[test]
fn refuses_a_vote_revealed_after_the_verdict_was_announced() {
    let decided = Decided::new();

    let ix = anchor_ix(
        verdict_mesh::accounts::RevealVote {
            juror: decided.reporter,
            dispute: decided.dispute,
            vote: decided.vote(),
        },
        verdict_mesh::instruction::RevealVote {
            choice: Ballot::Respondent,
            salt: [0u8; verdict_mesh::SALT_LEN],
        },
    );

    let result = decided.attempt(&ix);
    assert!(failed_with(&result, VerdictMeshError::WrongState));
}

/// Підсумок усіх спроб разом: жодна не лишила по собі іншого вердикту. Тест
/// навмисно дублює перевірки вище результатом, а не причиною — відмова з
/// правильним кодом і незмінений стан це дві різні обіцянки, і друга важливіша.
#[test]
fn keeps_the_verdict_it_announced_whatever_is_thrown_at_it() {
    let decided = Decided::new();

    let attempts = [
        anchor_ix(
            verdict_mesh::accounts::Tally {
                dispute: decided.dispute,
            },
            verdict_mesh::instruction::Tally {},
        ),
        anchor_ix(
            verdict_mesh::accounts::SelectPanel {
                dispute: decided.dispute,
                registry: registry_pda().0,
                slot_hashes: SLOT_HASHES_ID,
            },
            verdict_mesh::instruction::SelectPanel {},
        ),
        anchor_ix(
            verdict_mesh::accounts::RevealVote {
                juror: decided.reporter,
                dispute: decided.dispute,
                vote: decided.vote(),
            },
            verdict_mesh::instruction::RevealVote {
                choice: Ballot::Respondent,
                salt: [0u8; verdict_mesh::SALT_LEN],
            },
        ),
        decided.commit_attempt(),
    ];

    for ix in attempts {
        let result = decided.attempt(&ix);

        assert!(result.program_result.is_err());
        assert_eq!(decided.verdict_after(&result), Some(DECIDED));
    }
}
