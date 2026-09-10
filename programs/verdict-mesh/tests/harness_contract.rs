//! Тести самої обв'язки — окремою ціллю, щоб виконатись один раз.
//!
//! Обв'язка не є частиною програми, але кожен тест програми спирається на її
//! PDA, фікстури й переклад типів. Помилка тут виглядала б як помилка
//! програми, тому вона перевіряється, а не вважається очевидною.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;

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
