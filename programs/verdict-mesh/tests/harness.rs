//! Обв'язка для тестів програми на mollusk.
//!
//! Підключається з інших тестових бінарників так:
//! `#[allow(dead_code)] #[path = "harness.rs"] mod harness;`

use anchor_lang::{
    solana_program::pubkey::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas,
};
use mollusk_svm::{program::loader_keys::LOADER_V3, result::InstructionResult, Mollusk};
use solana_account::Account;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_instruction_error::InstructionError;
use verdict_mesh::{seeds, state::Policy, VerdictMeshError};

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
    mollusk
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

/// Програма-власник розрахункового мінта. `InterfaceAccount<Mint>` приймає і
/// Token-2022, але демо-USDC на devnet — класичний SPL Token.
pub const TOKEN_PROGRAM: Pubkey = anchor_spl::token::ID;

/// Мінт SPL Token викладений вручну, а не через `spl_token::state::Mint::pack`:
/// пакувальник у тестах вимагав би ще одну версію `spl-token` у дереву
/// залежностей заради вісімдесяти двох байтів фіксованого розкладу.
///
/// `[0..4]` тег `COption` авторитету емісії, `[4..36]` сам авторитет,
/// `[36..44]` емісія, `[44]` знаки, `[45]` ознака ініціалізації,
/// `[46..82]` авторитет заморозки.
pub fn spl_mint(decimals: u8) -> Account {
    let mut data = vec![0u8; 82];
    data[44] = decimals;
    data[45] = 1;

    Account {
        lamports: 1_461_600,
        data,
        owner: addr(&TOKEN_PROGRAM),
        executable: false,
        rent_epoch: 0,
    }
}

/// Мінт розрахункового активу демо — 6 знаків, як у devnet-USDC.
pub fn settlement_mint() -> Account {
    spl_mint(DECIMALS as u8)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_program_into_the_cache() {
        let mollusk = mollusk();
        assert!(mollusk
            .program_cache
            .load_program(&addr(&PROGRAM_ID))
            .is_some());
    }

    #[test]
    fn usdc_scales_by_the_mint_decimals() {
        assert_eq!(usdc(1), 1_000_000);
        assert_eq!(usdc(100), 100_000_000);
    }

    #[test]
    fn every_pda_is_derived_from_the_program() {
        let authority = Pubkey::new_unique();
        let juror = Pubkey::new_unique();
        let (integrator, _) = integrator_pda(&authority);
        let (dispute, _) = dispute_pda(&integrator, 0);

        for (pda, _) in [
            config_pda(),
            registry_pda(),
            integrator_pda(&authority),
            juror_pda(&juror),
            juror_index_pda(0),
            dispute_pda(&integrator, 7),
            vote_pda(&dispute, &juror),
        ] {
            assert!(!pda.is_on_curve(), "{pda} must be off-curve to be a PDA");
        }
    }

    /// Індекс присяжного входить у seed у little-endian. Якби порядок байтів
    /// розійшовся між програмою і клієнтом, сусідні індекси мовчки вказували б
    /// на чужі акаунти — тому перевіряємо, що різні індекси дають різні PDA.
    #[test]
    fn juror_index_pdas_are_distinct_per_index() {
        let (zero, _) = juror_index_pda(0);
        let (one, _) = juror_index_pda(1);
        let (big, _) = juror_index_pda(256);
        assert_ne!(zero, one);
        assert_ne!(one, big);
        assert_ne!(zero, big);
    }

    /// Той самий спір у двох різних інтеграторів — різні акаунти. Інакше
    /// нумерація одного інтегратора затирала б спори іншого.
    #[test]
    fn dispute_pda_separates_integrators() {
        let (first, _) = integrator_pda(&Pubkey::new_unique());
        let (second, _) = integrator_pda(&Pubkey::new_unique());
        assert_ne!(dispute_pda(&first, 1).0, dispute_pda(&second, 1).0);
    }

    #[test]
    fn account_fixtures_have_the_shapes_instructions_expect() {
        let signer = wallet(10_000_000);
        assert_eq!(signer.lamports, 10_000_000);
        assert_eq!(signer.owner, addr(&SYSTEM_PROGRAM));
        assert!(signer.data.is_empty());

        let absent = missing();
        assert_eq!(absent.lamports, 0);
        assert!(absent.data.is_empty());
    }

    #[test]
    fn demo_policy_matches_the_documented_configuration() {
        let policy = demo_policy();
        assert_eq!(policy.juror_stake, usdc(100));
        assert_eq!(policy.deposit, usdc(5));
        assert_eq!(policy.optimistic_threshold, usdc(50));
        assert!(policy.extended_panel_size > policy.panel_size);
        assert!(policy.extended_quorum > policy.quorum);
        // Мовчання має коштувати дорожче за програний голос — FR-008b.
        assert!(policy.slash_bps_no_reveal > policy.slash_bps_wrong);
    }
}
