//! T011 — реєстрація інтегратора і валідація політики (`FR-001`, `FR-002`,
//! `FR-026d`).
//!
//! Політика — це набір чисел, за якими потім рахуються чужі кошти: скільки
//! стейку згорає, скільки коштує розгляд, скільки голосів вирішують. Жодна
//! наступна інструкція не перевіряє їх повторно, тому кожне правило нижче — це
//! місце, де несправна політика мала б зупинитись і не зупиниться більше ніде.
//!
//! Кожен тест псує рівно одне поле еталонної демо-політики. Тому одного коду
//! помилки `InvalidPolicy` досить: якби перевірки бракувало, зіпсована політика
//! пройшла б, і впав би саме той тест, що її псує.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::{program::keyed_account_for_system_program, result::InstructionResult};
use solana_account::Account;
use solana_address::Address;
use verdict_mesh::{
    state::{Integrator, Policy, MAX_PANEL_SIZE, MAX_WINDOW},
    VerdictMeshError,
};

struct Fixture {
    authority: Pubkey,
    integrator: Pubkey,
    escrow_program: Pubkey,
    accounts: Vec<(Address, Account)>,
}

impl Fixture {
    fn new() -> Self {
        let authority = Pubkey::new_unique();
        let escrow_program = Pubkey::new_unique();
        let (integrator, _) = integrator_pda(&authority);
        let (config, _) = config_pda();

        let accounts = vec![
            (addr(&authority), wallet(10_000_000_000)),
            (addr(&integrator), missing()),
            (
                addr(&config),
                config_account(
                    &Pubkey::new_unique(),
                    &Pubkey::new_unique(),
                    &Pubkey::new_unique(),
                ),
            ),
            (addr(&escrow_program), executable_program()),
            keyed_account_for_system_program(),
        ];

        Self {
            authority,
            integrator,
            escrow_program,
            accounts,
        }
    }

    /// Реєстрація демо-політикою — еталон, від якого відходять решта тестів.
    fn register(&self) -> InstructionResult {
        self.register_with(demo_policy())
    }

    fn register_with(&self, policy: Policy) -> InstructionResult {
        self.run(policy, &self.accounts)
    }

    fn run(&self, policy: Policy, accounts: &[(Address, Account)]) -> InstructionResult {
        let ix = anchor_ix(
            verdict_mesh::accounts::RegisterIntegrator {
                authority: self.authority,
                integrator: self.integrator,
                config: config_pda().0,
                escrow_program: self.escrow_program,
                system_program: SYSTEM_PROGRAM,
            },
            verdict_mesh::instruction::RegisterIntegrator { policy },
        );
        mollusk().process_instruction(&ix, accounts)
    }
}

/// Політика з одним зміненим полем: тест читається як «те саме, але X».
fn policy_with(mutate: impl FnOnce(&mut Policy)) -> Policy {
    let mut policy = demo_policy();
    mutate(&mut policy);
    policy
}

fn assert_rejected(result: &InstructionResult) {
    assert!(
        failed_with(result, VerdictMeshError::InvalidPolicy),
        "expected InvalidPolicy, got {:?}",
        result.raw_result
    );
}

// ── успішна реєстрація ──────────────────────────────────────────────────────

#[test]
fn stores_the_authority_the_escrow_program_and_the_policy() {
    let fixture = Fixture::new();
    let result = fixture.register();

    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let integrator: Integrator = decode(resulting(&result, &fixture.integrator));
    assert_eq!(integrator.authority, fixture.authority);
    assert_eq!(integrator.escrow_program, fixture.escrow_program);
    assert_eq!(integrator.policy, demo_policy());
    assert_eq!(integrator.bump, integrator_pda(&fixture.authority).1);
}

/// Лічильник — джерело `dispute_id` у seed спору. Починається з нуля, інакше
/// перший спір інтегратора виявився б не першим.
#[test]
fn starts_the_dispute_counter_at_zero() {
    let fixture = Fixture::new();
    let result = fixture.register();

    let integrator: Integrator = decode(resulting(&result, &fixture.integrator));
    assert_eq!(integrator.dispute_count, 0);
}

/// `FR-026d`: розмір депозиту відомий стороні до відкриття спору. Це виконується
/// тим, що депозит лежить у політиці, яку видно в читаному акаунті, а не
/// рахується десь у момент відкриття.
#[test]
fn publishes_the_review_deposit_in_readable_state() {
    let fixture = Fixture::new();
    let result = fixture.register();

    let integrator: Integrator = decode(resulting(&result, &fixture.integrator));
    assert_eq!(integrator.policy.deposit, demo_policy().deposit);
    assert!(integrator.policy.deposit > 0);
}

/// Другий інтегратор із власним ключем не заважає першому: PDA різні.
#[test]
fn separates_integrators_by_authority() {
    let first = Fixture::new();
    let second = Fixture::new();
    assert_ne!(first.integrator, second.integrator);

    assert!(first.register().program_result.is_ok());
    assert!(second.register().program_result.is_ok());
}

/// Політика фіксується на реєстрації, і інструкції оновлення в програмі немає.
/// Повторна реєстрація тим самим ключем — єдиний шлях, яким її можна було б
/// підмінити під уже відкритими спорами.
#[test]
fn refuses_a_second_registration_for_the_same_authority() {
    let fixture = Fixture::new();
    let first = fixture.register();
    assert!(first.program_result.is_ok(), "{:?}", first.raw_result);

    let mut accounts = fixture.accounts.clone();
    accounts[1] = (
        addr(&fixture.integrator),
        resulting(&first, &fixture.integrator).clone(),
    );

    let second = fixture.run(policy_with(|p| p.deposit = usdc(1)), &accounts);
    assert!(
        second.program_result.is_err(),
        "a second registration must not replace the policy"
    );
}

// ── межі реєстрації ─────────────────────────────────────────────────────────

/// Політика не існує без розрахункового активу: усі її суми — у ньому
/// (`FR-011a`). Реєстрація до `initialize` дала б інтегратора з числами в
/// активі, якого протокол ще не має.
#[test]
fn requires_the_protocol_to_be_initialized() {
    let fixture = Fixture::new();

    let mut accounts = fixture.accounts.clone();
    accounts[2] = (addr(&config_pda().0), missing());

    let result = fixture.run(demo_policy(), &accounts);
    assert!(result.program_result.is_err());
}

/// `escrow_program` — ідентифікатор програми, чиї акаунти згодом відкриватимуть
/// спори. Помилка в ньому назавжди: інструкції оновлення немає, і виправити її
/// можна лише реєстрацією під іншим ключем.
#[test]
fn rejects_an_escrow_program_that_is_not_executable() {
    let fixture = Fixture::new();

    let mut accounts = fixture.accounts.clone();
    accounts[3] = (addr(&fixture.escrow_program), wallet(1_000_000));

    let result = fixture.run(demo_policy(), &accounts);
    assert!(result.program_result.is_err());
}

/// Реєструє інтегратора його власник, а не будь-хто за нього: authority і є
/// seed-ом PDA, тож без підпису чужий ключ закріпив би за собою чужий протокол.
#[test]
fn requires_the_authority_to_sign() {
    let fixture = Fixture::new();
    let ix = anchor_ix(
        verdict_mesh::accounts::RegisterIntegrator {
            authority: fixture.authority,
            integrator: fixture.integrator,
            config: config_pda().0,
            escrow_program: fixture.escrow_program,
            system_program: SYSTEM_PROGRAM,
        },
        verdict_mesh::instruction::RegisterIntegrator {
            policy: demo_policy(),
        },
    );

    let authority = ix
        .accounts
        .iter()
        .find(|meta| meta.pubkey == addr(&fixture.authority))
        .expect("the authority must be among the instruction accounts");
    assert!(authority.is_signer);
}

// ── валідація політики (FR-002) ─────────────────────────────────────────────

#[test]
fn accepts_the_demo_policy() {
    assert!(Fixture::new().register().program_result.is_ok());
}

#[test]
fn rejects_an_empty_panel() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| p.panel_size = 0)));
}

/// Кворум — не «скільки завгодно розкритих голосів». Якщо він не є більшістю
/// панелі, вердикт може винести меншість присяжних, які просто розкрились
/// першими, а решта не встигла.
#[test]
fn rejects_a_quorum_that_is_not_a_majority_of_the_panel() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| {
        p.panel_size = 4;
        p.quorum = 2;
    })));
}

#[test]
fn rejects_a_quorum_larger_than_the_panel() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| p.quorum = p.panel_size + 1)));
}

#[test]
fn rejects_a_zero_quorum() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| p.quorum = 0)));
}

/// `FR-027`: ескалація має відчутно змінювати вибірку. Розширена панель, не
/// більша за початкову, робить автоескалацію другим прогоном того самого.
#[test]
fn rejects_an_extended_panel_that_is_not_larger() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| p.extended_panel_size = p.panel_size)));
}

/// Ескалація не має бути способом знизити планку: розширений розгляд не
/// закривається меншою кількістю голосів, ніж вимагав початковий.
#[test]
fn rejects_an_extended_quorum_below_the_initial_one() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| {
        // 4 з 7 — законна більшість розширеної панелі. Єдине, що не так:
        // початкова вимагала 5 голосів, а розширена вимагає менше.
        p.panel_size = 5;
        p.quorum = 5;
        p.extended_panel_size = 7;
        p.extended_quorum = 4;
    })));
}

#[test]
fn rejects_an_extended_quorum_that_is_not_a_majority() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| {
        p.extended_panel_size = 6;
        p.extended_quorum = 3;
    })));
}

/// Панель обмежена, бо її перебирають у циклі на кожному підрахунку і тримають
/// вектором у самому спорі. Необмежене число тут — інструкція, яку одного дня
/// не вистачить бюджету виконати.
#[test]
fn rejects_a_panel_beyond_the_cap() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| {
        // Кворум лишається законною більшістю панелі, щоб єдиним порушенням
        // був її розмір.
        p.extended_panel_size = MAX_PANEL_SIZE + 1;
        p.extended_quorum = MAX_PANEL_SIZE / 2 + 1;
    })));
}

#[test]
fn accepts_a_panel_exactly_at_the_cap() {
    let fixture = Fixture::new();
    let result = fixture.register_with(policy_with(|p| {
        p.panel_size = MAX_PANEL_SIZE - 1;
        p.quorum = MAX_PANEL_SIZE / 2 + 1;
        p.extended_panel_size = MAX_PANEL_SIZE;
        p.extended_quorum = MAX_PANEL_SIZE / 2 + 1;
    }));
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
}

/// `FR-008b`: мовчання має коштувати дорожче за програний голос. Інакше
/// присяжний, який бачить, що програє, просто не розкривається — і робить це
/// дешевше, ніж чесно програти.
#[test]
fn rejects_silence_priced_at_or_below_a_lost_vote() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| {
        p.slash_bps_no_reveal = p.slash_bps_wrong;
    })));
    assert_rejected(&fixture.register_with(policy_with(|p| {
        p.slash_bps_no_reveal = p.slash_bps_wrong - 1;
    })));
}

/// Частка понад 100% списала б зі стейку більше, ніж у ньому є.
#[test]
fn rejects_a_slash_share_above_the_whole_stake() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| p.slash_bps_no_reveal = 10_001)));
}

/// Нульовий слешинг робить програний голос безкоштовним, а разом із ним —
/// і всю економіку присяжних.
#[test]
fn rejects_a_lost_vote_that_costs_nothing() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| p.slash_bps_wrong = 0)));
}

#[test]
fn rejects_a_zero_juror_stake() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| p.juror_stake = 0)));
}

/// `FR-026b`: оплата розгляду розподіляється між присяжними й протоколом.
/// Нульовий депозит означає, що панель працює безкоштовно.
#[test]
fn rejects_a_zero_deposit() {
    let fixture = Fixture::new();
    assert_rejected(&fixture.register_with(policy_with(|p| p.deposit = 0)));
}

#[test]
fn rejects_a_window_that_closes_before_it_opens() {
    let fixture = Fixture::new();
    for mutate in [
        (|p: &mut Policy| p.commit_window = 0) as fn(&mut Policy),
        |p: &mut Policy| p.reveal_window = 0,
        |p: &mut Policy| p.appeal_window = 0,
        |p: &mut Policy| p.optimistic_window = 0,
        |p: &mut Policy| p.commit_window = -1,
    ] {
        assert_rejected(&fixture.register_with(policy_with(mutate)));
    }
}

/// Вікна складаються в дедлайни. Без верхньої межі сума переповнює `i64` уже
/// на другому доданку, і спір отримує дедлайн у минулому.
#[test]
fn rejects_a_window_beyond_the_cap() {
    let fixture = Fixture::new();
    for mutate in [
        (|p: &mut Policy| p.commit_window = MAX_WINDOW + 1) as fn(&mut Policy),
        |p: &mut Policy| p.reveal_window = i64::MAX,
        |p: &mut Policy| p.appeal_window = MAX_WINDOW + 1,
        |p: &mut Policy| p.optimistic_window = i64::MAX,
    ] {
        assert_rejected(&fixture.register_with(policy_with(mutate)));
    }
}

/// Нульовий поріг — валідна політика: вона просто вимикає оптимістичний трек,
/// бо жодна сума не опиниться нижче нуля. `SPEC.md` → US5 передбачає спори, до
/// яких трек не застосовується взагалі.
#[test]
fn accepts_a_zero_optimistic_threshold() {
    let fixture = Fixture::new();
    let result = fixture.register_with(policy_with(|p| p.optimistic_threshold = 0));
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
}
