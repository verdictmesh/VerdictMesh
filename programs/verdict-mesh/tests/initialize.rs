//! T010 — `Config` і `initialize` (`FR-011a`, `FR-017`).
//!
//! `Config` — єдиний глобальний акаунт протоколу і водночас місце, де живуть
//! три прив'язки, які потім ніхто не може посунути: спільний розрахунковий
//! актив, ключ ролі reporter і адресу скарбниці. Тому тести тут перевіряють не стільки успішний
//! запис, скільки те, чого зробити **не можна**: записати вдруге, підсунути не
//! той PDA, оголосити розрахунковим активом акаунт, який не є мінтом.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::program::keyed_account_for_system_program;
use solana_account::Account;
use solana_instruction::Instruction;
use verdict_mesh::{state::Config, VerdictMeshError};

/// Один набір акаунтів на всі тести: відрізняється лише те, що тест ламає.
struct Fixture {
    payer: Pubkey,
    config: Pubkey,
    mint: Pubkey,
    reporter: Pubkey,
    treasury: Pubkey,
    accounts: Vec<(solana_address::Address, Account)>,
}

impl Fixture {
    fn new() -> Self {
        let payer = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let reporter = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let (config, _) = config_pda();

        let accounts = vec![
            (addr(&payer), wallet(10_000_000_000)),
            (addr(&config), missing()),
            (addr(&mint), settlement_mint()),
            keyed_account_for_system_program(),
        ];

        Self {
            payer,
            config,
            mint,
            reporter,
            treasury,
            accounts,
        }
    }

    fn ix(&self) -> Instruction {
        self.ix_with(self.config, self.mint, self.reporter)
    }

    fn ix_with(&self, config: Pubkey, mint: Pubkey, reporter: Pubkey) -> Instruction {
        self.ix_full(config, mint, reporter, self.treasury)
    }

    fn ix_full(
        &self,
        config: Pubkey,
        mint: Pubkey,
        reporter: Pubkey,
        treasury: Pubkey,
    ) -> Instruction {
        anchor_ix(
            verdict_mesh::accounts::Initialize {
                payer: self.payer,
                config,
                settlement_mint: mint,
                system_program: SYSTEM_PROGRAM,
            },
            verdict_mesh::instruction::Initialize { reporter, treasury },
        )
    }
}

#[test]
fn writes_the_settlement_mint_and_the_reporter_key() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.ix(), &fixture.accounts);

    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let config: Config = decode(resulting(&result, &fixture.config));
    assert_eq!(config.settlement_mint, fixture.mint);
    assert_eq!(config.reporter, fixture.reporter);
    assert_eq!(config.treasury, fixture.treasury);
    assert_eq!(config.bump, config_pda().1);
}

/// Bump зберігається, щоб програма далі підписувала PDA без `find_program_address`
/// — 32 ітерації пошуку на кожному виклику коштують обчислювального бюджету.
#[test]
fn stores_the_canonical_bump() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.ix(), &fixture.accounts);

    let config: Config = decode(resulting(&result, &fixture.config));
    let derived =
        Pubkey::create_program_address(&[verdict_mesh::seeds::CONFIG, &[config.bump]], &PROGRAM_ID)
            .expect("stored bump must derive the config PDA");
    assert_eq!(derived, fixture.config);
}

/// `FR-017` тримається на тому, що ключ reporter незмінний. Незмінність тут —
/// не перевірка в коді, а відсутність другого запису: `Config` створюється раз,
/// інструкції оновлення не існує. Тест доводить перше; друге доводить T024.
#[test]
fn refuses_a_second_initialization() {
    let fixture = Fixture::new();
    let mollusk = mollusk();

    let first = mollusk.process_instruction(&fixture.ix(), &fixture.accounts);
    assert!(first.program_result.is_ok(), "{:?}", first.raw_result);

    let mut accounts = fixture.accounts.clone();
    accounts[1] = (
        addr(&fixture.config),
        resulting(&first, &fixture.config).clone(),
    );

    let hijacker = Pubkey::new_unique();
    let second = mollusk.process_instruction(
        &fixture.ix_with(fixture.config, fixture.mint, hijacker),
        &accounts,
    );

    assert!(
        second.program_result.is_err(),
        "a second initialize must not rewrite the reporter key"
    );
}

/// Некононічний PDA прийняли б лише за відсутності `seeds`/`bump` у контексті —
/// і тоді в протоколі існували б два `Config` з різними розрахунковими активами.
#[test]
fn rejects_a_config_account_that_is_not_the_canonical_pda() {
    let fixture = Fixture::new();
    let impostor = Pubkey::new_unique();

    let mut accounts = fixture.accounts.clone();
    accounts[1] = (addr(&impostor), missing());

    let result = mollusk().process_instruction(
        &fixture.ix_with(impostor, fixture.mint, fixture.reporter),
        &accounts,
    );

    assert!(result.program_result.is_err());
}

/// `FR-011a`: розрахунковий актив спільний для всього протоколу. Якщо його не
/// перевірити тут, помилка спливе на першому переказі стейку — за багато
/// інструкцій від місця, де її зробили.
#[test]
fn rejects_a_settlement_mint_that_is_not_a_mint() {
    let fixture = Fixture::new();

    let mut accounts = fixture.accounts.clone();
    accounts[2] = (addr(&fixture.mint), wallet(1_000_000));

    let result = mollusk().process_instruction(&fixture.ix(), &accounts);
    assert!(result.program_result.is_err());
}

/// Мінт, за яким лежить акаунт правильного власника, але неініціалізований,
/// проходить перевірку власника і провалюється лише на розпакуванні.
#[test]
fn rejects_an_uninitialized_mint() {
    let fixture = Fixture::new();

    let mut uninitialized = settlement_mint();
    uninitialized.data[45] = 0;

    let mut accounts = fixture.accounts.clone();
    accounts[2] = (addr(&fixture.mint), uninitialized);

    let result = mollusk().process_instruction(&fixture.ix(), &accounts);
    assert!(result.program_result.is_err());
}

/// Нульовий ключ — не «порожнє значення», а адреса, приватного ключа до якої
/// ніхто не має. `Config` із таким reporter пройшов би ініціалізацію і назавжди
/// лишив би `attest_report` недосяжним, а виправити його нічим — інструкції
/// оновлення немає за задумом.
#[test]
fn rejects_the_default_key_as_reporter() {
    let fixture = Fixture::new();

    let result = mollusk().process_instruction(
        &fixture.ix_with(fixture.config, fixture.mint, Pubkey::default()),
        &fixture.accounts,
    );

    assert!(failed_with(&result, VerdictMeshError::InvalidReporter));
}

/// Мінт передається лише як предмет перевірки. Підпису з нього не вимагається —
/// інакше протокол не міг би розраховуватись у чужому токені без його емітента.
#[test]
fn does_not_require_the_mint_to_sign() {
    let fixture = Fixture::new();
    let metas = fixture.ix().accounts;

    let mint = metas
        .iter()
        .find(|meta| meta.pubkey == addr(&fixture.mint))
        .expect("the mint must be among the instruction accounts");

    assert!(!mint.is_signer);
    assert!(!mint.is_writable);
}

/// Скарбниця з нульовим ключем ламає не комісію, а **фіналізацію**: розрахунок
/// спору переказує її частку (`FR-026b`) і падає на токен-акаунті, якого в
/// нульового ключа немає. Разом із переказом падає весь кранк — слешинг і вихід
/// присяжних із реєстру теж, — тож кожен спір такого протоколу застрягає
/// назавжди. Виправити нічим: інструкції оновлення `Config` не існує.
#[test]
fn rejects_the_default_key_as_treasury() {
    let fixture = Fixture::new();

    let result = mollusk().process_instruction(
        &fixture.ix_full(
            fixture.config,
            fixture.mint,
            fixture.reporter,
            Pubkey::default(),
        ),
        &fixture.accounts,
    );

    assert!(failed_with(&result, VerdictMeshError::InvalidTreasury));
}

/// Скарбниця — адреса призначення, а не роль. Ключ, який нічого не підписує і
/// нічого не викликає, не може ані змінити вердикт, ані дістати чужі кошти:
/// саме тому `FR-026b` не суперечить `FR-014`. Тест дивиться на список акаунтів
/// інструкції — скарбниці в ньому немає взагалі, вона лише аргумент.
#[test]
fn does_not_take_the_treasury_as_an_account() {
    let fixture = Fixture::new();

    assert!(
        !fixture
            .ix()
            .accounts
            .iter()
            .any(|meta| meta.pubkey == addr(&fixture.treasury)),
        "the treasury is a destination address, never a signer or an account"
    );
}
