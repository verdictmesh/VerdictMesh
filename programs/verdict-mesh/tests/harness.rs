//! Обв'язка для тестів програми на mollusk.
//!
//! Підключається з інших тестових бінарників так:
//! `#[allow(dead_code)] #[path = "harness.rs"] mod harness;`
//!
//! Власних тестів тут немає навмисно — вони лежать у `harness_contract.rs`.
//! `#[path]` втягує файл цілком, тож `#[cfg(test)] mod tests` усередині
//! запускався б заново в кожному тестовому бінарнику: ті самі сім перевірок
//! у звіті стільки разів, скільки в проекті тестових файлів.

use std::{cell::RefCell, rc::Rc};

use anchor_lang::{
    solana_program::pubkey::Pubkey, AccountDeserialize, AnchorDeserialize, AnchorSerialize,
    Discriminator, InstructionData, ToAccountMetas,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use mollusk_svm::{program::loader_keys::LOADER_V3, result::InstructionResult, Mollusk};
use mollusk_svm_programs_token::token;
use solana_account::Account;
use solana_address::Address;
use solana_hash::Hash;
use solana_instruction::{AccountMeta, Instruction};
use solana_instruction_error::InstructionError;
use solana_program_pack::Pack;
use solana_slot_hashes::SlotHashes;
use solana_svm_log_collector::LogCollector;
use spl_token_interface::state::{
    Account as SplTokenAccount, AccountState as SplAccountState, Mint as SplMint,
};
use verdict_mesh::{
    panel::SLOT_HASHES_DEPTH,
    seeds,
    state::{Config, Policy},
    VerdictMeshError,
};

pub const PROGRAM_ID: Pubkey = verdict_mesh::ID;

/// mollusk 0.15 говорить типами нового покоління крейтів Solana (`Address`), а
/// Anchor 0.32.1 — старого (`Pubkey`). Це той самий 32-байтовий ключ у двох
/// незалежних новотипах, і жодна зі сторін не знає про іншу. Переклад тримаємо
/// в одному місці, щоб він не розповз по кожному тесту.
pub fn addr(key: &Pubkey) -> Address {
    Address::from(key.to_bytes())
}

/// Мінт із 6 знаками — стільки має розрахунковий актив, і саме на цьому
/// множнику будуються всі суми нижче.
pub const DECIMALS: u32 = 6;

pub fn usdc(amount: u64) -> u64 {
    amount * 10u64.pow(DECIMALS)
}

/// ELF шукається явним шляхом, а не через пошук mollusk у поточному каталозі:
/// тести запускаються з каталогу крейта, а `anchor build` кладе артефакт у
/// `target/deploy` кореня воркспейсу. Мовчазний промах пошуку виглядав би як
/// «програма не в кеші» — повідомлення, що не натякає на шлях.
pub fn mollusk() -> Mollusk {
    let elf_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy/verdict_mesh.so");

    let elf = std::fs::read(&elf_path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}. Run `anchor build` (WSL) before the program tests.",
            elf_path.display()
        )
    });

    let mut mollusk = Mollusk::default();
    mollusk.add_program_with_loader_and_elf(&addr(&PROGRAM_ID), &LOADER_V3, &elf);
    // Справжній байткод SPL Token. Інструкції зі стейками роблять CPI переказу,
    // і без цього тест перевіряв би не переказ, а власну фікстуру балансу.
    token::add_program(&mut mollusk);
    // Годинник за замовчуванням стоїть на нулі, і тоді `opened_at + window`
    // збігається з самим `window`. Тест, який перевіряє дедлайни, проходив би
    // й тоді, коли програма забула додати час відкриття.
    mollusk.sysvars.clock.unix_timestamp = NOW;
    mollusk.sysvars.clock.slot = SLOT;
    install_slot_hashes(&mut mollusk);
    mollusk
}

/// Фіксований «поточний слот» у тестах.
pub const SLOT: u64 = 372_000_000;

/// Слот, з хеша якого виводиться ентропія відбору: попередній щодо поточного —
/// хеш поточного ще не існує. Рівно те, що записує `open_dispute`.
pub const ENTROPY_SLOT: u64 = SLOT - 1;

/// `SlotHashes` із **різними** хешами. Mollusk за замовчуванням кладе туди нулі,
/// і тоді ентропія однакова для всіх слотів: тест «інший слот — інша панель»
/// проходив би, нічого не перевіряючи.
fn install_slot_hashes(mollusk: &mut Mollusk) {
    let entries: Vec<(u64, Hash)> = (0..SLOT_HASHES_DEPTH as u64)
        .map(|offset| {
            let slot = SLOT - 1 - offset;
            let mut hash = [0u8; 32];
            hash[..8].copy_from_slice(&slot.to_le_bytes());
            hash[8] = 0xA5;
            (slot, Hash::new_from_array(hash))
        })
        .collect();

    mollusk.sysvars.slot_hashes = SlotHashes::new(&entries);
}

/// Акаунт сисвара `SlotHashes` у тому вигляді, у якому його бачить програма.
pub fn keyed_account_for_slot_hashes(mollusk: &Mollusk) -> (Address, Account) {
    mollusk.sysvars.keyed_account_for_slot_hashes_sysvar()
}

/// Фіксований «зараз» у тестах — 2027-01-15, довільна, але не нульова мить.
pub const NOW: i64 = 1_800_000_000;

/// Mollusk зі збирачем логів. Події Anchor не лишають сліду в акаунтах, тож
/// `FR-029` перевіряється єдиним доступним способом — читанням того, що
/// програма справді записала в лог.
pub fn mollusk_with_logs() -> (Mollusk, Rc<RefCell<LogCollector>>) {
    let mut mollusk = mollusk();
    let logs = LogCollector::new_ref();
    mollusk.logger = Some(logs.clone());
    (mollusk, logs)
}

/// Події одного типу з логів, у порядку появи. `emit!` кладе в лог
/// `Program data: <base64(дискримінатор ++ borsh)>`; чужі рядки й події інших
/// типів відсіюються за дискримінатором.
pub fn emitted<E: AnchorDeserialize + Discriminator>(logs: &Rc<RefCell<LogCollector>>) -> Vec<E> {
    logs.borrow()
        .get_recorded_content()
        .iter()
        .filter_map(|line| line.strip_prefix("Program data: "))
        .filter_map(|payload| BASE64.decode(payload).ok())
        .filter_map(|bytes| {
            let discriminator = E::DISCRIMINATOR;
            let mut body = bytes.strip_prefix(discriminator)?;
            E::deserialize(&mut body).ok()
        })
        .collect()
}

/// Складає інструкцію з двох згенерованих Anchor структур — `accounts::*` і
/// `instruction::*`. Дискримінатор і порядок акаунтів беруться з тієї самої
/// програми, що їх потім читає, тож тест не може розійтися з нею вручну
/// написаним байтом.
pub fn anchor_ix<A: ToAccountMetas, D: InstructionData>(accounts: A, args: D) -> Instruction {
    Instruction {
        program_id: addr(&PROGRAM_ID),
        accounts: accounts
            .to_account_metas(None)
            .iter()
            .map(|meta| AccountMeta {
                pubkey: addr(&meta.pubkey),
                is_signer: meta.is_signer,
                is_writable: meta.is_writable,
            })
            .collect(),
        data: args.data(),
    }
}

/// Акаунт із результату виконання за ключем. Mollusk повертає всі передані
/// акаунти в тому ж порядку, але шукати за позицією означає ламати тест при
/// кожній зміні порядку в `#[derive(Accounts)]`.
pub fn resulting<'a>(result: &'a InstructionResult, key: &Pubkey) -> &'a Account {
    let key = addr(key);
    result
        .resulting_accounts
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, account)| account)
        .unwrap_or_else(|| panic!("{key} is not among the resulting accounts"))
}

/// Читає стан Anchor-акаунта разом із перевіркою дискримінатора: якщо за PDA
/// лежить акаунт іншого типу, тест має впасти тут, а не на розбіжності полів.
pub fn decode<T: AccountDeserialize>(account: &Account) -> T {
    T::try_deserialize(&mut account.data.as_slice())
        .unwrap_or_else(|error| panic!("cannot deserialize the account: {error}"))
}

/// Провал саме з очікуваною помилкою програми, а не «якийсь провал». Тест, що
/// приймає будь-яку помилку, зеленіє й тоді, коли інструкція падає з зовсім
/// іншої причини — наприклад на відсутньому акаунті замість перевірки, яку він
/// нібито доводить.
pub fn failed_with(result: &InstructionResult, expected: VerdictMeshError) -> bool {
    matches!(&result.raw_result, Err(InstructionError::Custom(code)) if *code == u32::from(expected))
}

/// Порожній акаунт, що належить системній програмі — типовий підписант.
pub fn wallet(lamports: u64) -> Account {
    Account::new(lamports, 0, &addr(&SYSTEM_PROGRAM))
}

pub const SYSTEM_PROGRAM: Pubkey = anchor_lang::solana_program::system_program::ID;

/// Акаунт, який ще не існує. Mollusk вимагає, щоб кожен ключ в інструкції мав
/// запис, тож «немає акаунта» доводиться передавати явно нулями.
pub fn missing() -> Account {
    Account::default()
}

/// Акаунт, який програма вже колись створила: дискримінатор плюс стан. Потрібен
/// там, де тест перевіряє інструкцію, що читає вже наявний стан, і проганяти
/// заради нього попередню інструкцію означало б зав'язати один тест на дві.
pub fn program_account<T: AnchorSerialize + Discriminator>(state: &T) -> Account {
    let mut data = T::DISCRIMINATOR.to_vec();
    state
        .serialize(&mut data)
        .expect("account state must serialize");

    Account {
        // Rent-exempt із запасом. Anchor не звіряє баланс наявного акаунта з
        // рентою, тож точне число тут нічого не доводить.
        lamports: 10_000_000,
        data,
        owner: addr(&PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }
}

/// Готовий `Config` — те, що лишає по собі `initialize`.
pub fn config_account(settlement_mint: &Pubkey, reporter: &Pubkey) -> Account {
    program_account(&Config {
        settlement_mint: *settlement_mint,
        reporter: *reporter,
        bump: config_pda().1,
    })
}

/// Чужа програма — ескроу інтегратора. Тіло не потрібне: жоден тест її не
/// викликає, перевіряється лише ознака executable.
pub fn executable_program() -> Account {
    Account {
        lamports: 1_000_000_000,
        data: vec![0u8; 36],
        owner: LOADER_V3,
        executable: true,
        rent_epoch: 0,
    }
}

/// Програма-власник розрахункового мінта. `InterfaceAccount<Mint>` приймає і
/// Token-2022, але демо-USDC на devnet — класичний SPL Token.
pub const TOKEN_PROGRAM: Pubkey = anchor_spl::token::ID;

/// Мінт SPL Token. Пакується тим самим розкладом, що читає сама програма
/// токена, — вручну викладені вісімдесят два байти перевіряли б лише те, чи
/// правильно тест пам'ятає зміщення.
///
/// `mintAuthority` порожній навмисно: емісія в тестах не потрібна, а мінт без
/// авторитету — рівно те, чим розрахунковий актив стане після демо.
/// `freezeAuthority` порожній із тієї ж причини, що й на devnet: ключ, здатний
/// заморозити токен-акаунт, заморожує і стейки, і виплату (`FR-014`).
pub fn spl_mint(decimals: u8) -> Account {
    token::create_account_for_mint(SplMint {
        decimals,
        is_initialized: true,
        ..SplMint::default()
    })
}

/// Мінт розрахункового активу демо — 6 знаків, як у devnet-USDC.
pub fn settlement_mint() -> Account {
    spl_mint(DECIMALS as u8)
}

/// Готовий токен-акаунт із балансом. Власник — звичайний гаманець: акаунти під
/// владою PDA створює сама програма, і підробляти їх тест не повинен.
pub fn token_account(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Account {
    token::create_account_for_token_account(SplTokenAccount {
        mint: addr(mint),
        owner: addr(owner),
        amount,
        state: SplAccountState::Initialized,
        ..SplTokenAccount::default()
    })
}

/// Розпакований токен-акаунт із результату виконання. Баланс читається з тих
/// самих байтів, які лишила по собі програма токена, а не з очікувань тесту.
pub fn token_state(account: &Account) -> SplTokenAccount {
    SplTokenAccount::unpack(&account.data)
        .unwrap_or_else(|error| panic!("cannot unpack the token account: {error}"))
}

/// Ключ і акаунт програми SPL Token — у списку акаунтів кожної інструкції, що
/// переказує розрахунковий актив.
pub fn keyed_account_for_token_program() -> (Address, Account) {
    token::keyed_account()
}

pub fn config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[seeds::CONFIG], &PROGRAM_ID)
}

pub fn registry_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[seeds::REGISTRY], &PROGRAM_ID)
}

pub fn integrator_pda(authority: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[seeds::INTEGRATOR, authority.as_ref()], &PROGRAM_ID)
}

pub fn juror_pda(wallet: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[seeds::JUROR, wallet.as_ref()], &PROGRAM_ID)
}

pub fn juror_index_pda(index: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[seeds::JUROR_INDEX, &index.to_le_bytes()], &PROGRAM_ID)
}

pub fn dispute_pda(integrator: &Pubkey, dispute_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            seeds::DISPUTE,
            integrator.as_ref(),
            &dispute_id.to_le_bytes(),
        ],
        &PROGRAM_ID,
    )
}

pub fn vote_pda(dispute: &Pubkey, juror: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[seeds::VOTE, dispute.as_ref(), juror.as_ref()],
        &PROGRAM_ID,
    )
}

/// Демо-політика з docs/PLAN.md → «Демо-конфігурація». Вікна стиснуті під
/// бюджет SC-001; у тестах вони ще й дозволяють проганяти цикл без warp на
/// реальні хвилини.
pub fn demo_policy() -> Policy {
    Policy {
        panel_size: 3,
        extended_panel_size: 5,
        quorum: 2,
        extended_quorum: 3,
        juror_stake: usdc(100),
        slash_bps_wrong: 1_000,
        slash_bps_no_reveal: 2_000,
        commit_window: 60,
        reveal_window: 60,
        appeal_window: 90,
        optimistic_window: 60,
        deposit: usdc(5),
        optimistic_threshold: usdc(50),
    }
}

pub fn stake_vault_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[seeds::STAKE_VAULT], &PROGRAM_ID)
}

/// Акаунт самої програми. Потрібен там, де інструкція має необов'язкові
/// акаунти: Anchor позначає «немає» ключем програми, тож він потрапляє у список
/// акаунтів інструкції нарівні з рештою.
pub fn keyed_account_for_this_program() -> (Address, Account) {
    (
        addr(&PROGRAM_ID),
        mollusk_svm::program::create_program_account_loader_v3(&addr(&PROGRAM_ID)),
    )
}

/// Підміна одного акаунта за ключем. Заміна за позицією ламалася б при кожній
/// зміні порядку в `#[derive(Accounts)]` — і мовчки, бо тест на відмову
/// однаково лишався б зеленим.
pub fn replace(accounts: &mut [(Address, Account)], key: &Pubkey, account: Account) {
    let key = addr(key);
    let entry = accounts
        .iter_mut()
        .find(|(candidate, _)| *candidate == key)
        .unwrap_or_else(|| panic!("{key} is not among the accounts"));
    entry.1 = account;
}
