//! Відбір панелі присяжних — `FR-006`, і межа, якої вимагає `FR-006a`.
//!
//! **Що саме тут ізольовано.** `FR-006a` вимагає, щоб заміна джерела ентропії
//! на зовнішній перевірюваний генератор не зачіпала реєстр присяжних,
//! голосування чи виконання вердикту. Тому в цьому модулі лежать **обидві**
//! половини: звідки береться випадковість (`entropy_of`) і як з неї виходить
//! панель (`draw`). Замінити slot hash на VRF означає переписати перше і не
//! торкнутись другого — і нічого поза цим файлом.
//!
//! **Слабке місце названо вголос.** Хеш слота піддається впливу лідера цього
//! слота, тобто панель теоретично передбачувана для того, хто цей слот
//! виробляє. Це записано в `SPEC.md` → «Поза скоупом» і в `PLAN.md` → ризики;
//! межа існує саме для того, щоб на mainnet сюди став VRF. На devnet-демо
//! ризик приймається свідомо.
//!
//! **Чому ентропія прив'язана до слота відкриття, а не до слота відбору.**
//! Інакше відбір можна було б переграти: повторювати спробу зі слота в слот,
//! доки панель не сподобається. Прив'язка до `Dispute.entropy_slot` робить
//! панель визначеною в момент відкриття спору й незмінною після нього.

use std::cmp::Ordering;

use anchor_lang::prelude::*;
use solana_keccak_hasher::hashv;

use crate::{errors::VerdictMeshError, state::MAX_PANEL_SIZE};

/// Один запис сисвара `SlotHashes`: слот (`u64` LE) і хеш (32 байти).
const SLOT_HASH_ENTRY: usize = 40;

/// Адреса сисвара `SlotHashes`. Оголошена тут, а не взята з реекспорту
/// Anchor: у 0.32.1 весь модуль `solana_program::sysvar` позначений
/// deprecated, а `clippy -D warnings` цього не пробачає.
pub const SLOT_HASHES_ID: Pubkey =
    anchor_lang::solana_program::pubkey::pubkey!("SysvarS1otHashes111111111111111111111111111");

/// Скільки слотів пам'ятає `SlotHashes`. Ентропія спору живе рівно стільки —
/// приблизно 3.5 хвилини, і це верхня межа затримки між відкриттям спору та
/// відбором панелі.
pub const SLOT_HASHES_DEPTH: usize = 512;

/// Ентропія відбору. Тип непрозорий навмисно: усе, що знає решта програми, —
/// що панель виводиться детерміновано з чогось, і звідки саме це «щось»
/// береться, поза цим модулем не видно.
pub struct Entropy([u8; 32]);

impl Entropy {
    /// Потік чисел із одного зерна. Хешування з лічильником замість
    /// послідовного `state = hash(state)` — щоб крок `i` не залежав від того,
    /// скільки кроків зробили до нього: так `draw` лишається чистою функцією
    /// від зерна і номера кроку.
    fn at(&self, step: u8) -> u64 {
        let bytes = hashv(&[self.0.as_slice(), &[step]]).0;
        u64::from_le_bytes(bytes[..8].try_into().expect("keccak gives 32 bytes"))
    }
}

/// Поточне джерело ентропії — `FR-006`: хеш недавнього слота в поєднанні з
/// ідентифікатором спору. **Це та функція, яку замінює перехід на VRF.**
///
/// Ідентифікатор спору тут не для випадковості, а щоб два спори, відкриті в
/// одному слоті, не отримали однакову панель.
pub fn entropy_of(slot_hashes: &AccountInfo, slot: u64, dispute: &Pubkey) -> Result<Entropy> {
    let data = slot_hashes.try_borrow_data()?;
    let slot_hash = find_slot_hash(&data, slot).ok_or(VerdictMeshError::EntropyUnavailable)?;

    Ok(Entropy(hashv(&[slot_hash.as_slice(), dispute.as_ref()]).0))
}

/// Хеш конкретного слота з сирих байтів сисвара.
///
/// `SlotHashes` не десеріалізують цілком: це 512 записів, і Anchor витратив би
/// на них і обчислювальний бюджет, і стек. Розклад фіксований — 8 байтів
/// кількості, далі пари «слот, хеш» у **спадному** порядку слотів, — тож
/// двійковий пошук читає рівно те, що потрібно.
fn find_slot_hash(data: &[u8], slot: u64) -> Option<[u8; 32]> {
    let count = usize::try_from(u64::from_le_bytes(data.get(..8)?.try_into().ok()?)).ok()?;

    let (mut low, mut high) = (0usize, count);
    while low < high {
        let middle = low + (high - low) / 2;
        let at = 8 + middle.checked_mul(SLOT_HASH_ENTRY)?;
        let candidate = u64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?);

        match candidate.cmp(&slot) {
            Ordering::Equal => return data.get(at + 8..at + SLOT_HASH_ENTRY)?.try_into().ok(),
            // Порядок спадний: більший слот означає, що шуканий далі праворуч.
            Ordering::Greater => low = middle + 1,
            Ordering::Less => high = middle,
        }
    }

    None
}

/// Обирає `size` різних позицій із `0..candidates` — детерміновано і без
/// повторів.
///
/// Часткове тасування Фішера—Йейтса: на кроці `i` один із тих, хто ще не
/// вибраний, міняється місцями з позицією `i`. Повторів немає за побудовою, і
/// це важливіше, ніж здається — той самий присяжний двічі в панелі означав би
/// подвійну вагу голосу і кворум, зібраний однією людиною.
///
/// Зсув від залишку по модулю тут є, але при `span ≤ 32` і 64-бітному числі
/// він порядку 2⁻⁵⁹ — на кілька порядків менший за вплив лідера слота на сам
/// хеш, тобто не є вузьким місцем.
pub fn draw(entropy: &Entropy, candidates: u32, size: u8) -> Result<Vec<u32>> {
    require!(size > 0, VerdictMeshError::InvalidPolicy);
    require!(size <= MAX_PANEL_SIZE, VerdictMeshError::InvalidPolicy);
    require!(
        u64::from(size) <= u64::from(candidates),
        VerdictMeshError::RegistryTooSmall
    );

    let mut pool: Vec<u32> = (0..candidates).collect();
    for step in 0..size {
        let taken = u32::from(step);
        let span = candidates - taken;
        let pick = taken + u32::try_from(entropy.at(step) % u64::from(span)).expect("pick < span");
        pool.swap(taken as usize, pick as usize);
    }

    pool.truncate(usize::from(size));
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entropy(seed: u8) -> Entropy {
        Entropy([seed; 32])
    }

    /// Панель без повторів — головна властивість. Той самий присяжний двічі
    /// означав би подвійну вагу голосу і кворум, зібраний однією людиною.
    #[test]
    fn draws_distinct_positions_inside_the_registry() {
        for seed in 0..32u8 {
            let panel = draw(&entropy(seed), 12, 5).expect("a panel of 5 out of 12 exists");

            assert_eq!(panel.len(), 5);
            assert!(panel.iter().all(|position| *position < 12));

            let mut sorted = panel.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), panel.len(), "повтор у панелі: {panel:?}");
        }
    }

    /// Детермінованість — це вимога, а не зручність тесту: без неї відбір
    /// неможливо ані перевірити, ані відтворити зі сторони.
    #[test]
    fn gives_the_same_panel_for_the_same_entropy() {
        let first = draw(&entropy(7), 20, 5).expect("a panel exists");
        let second = draw(&entropy(7), 20, 5).expect("a panel exists");
        assert_eq!(first, second);
    }

    #[test]
    fn gives_different_panels_for_different_entropy() {
        let first = draw(&entropy(1), 20, 5).expect("a panel exists");
        let second = draw(&entropy(2), 20, 5).expect("a panel exists");
        assert_ne!(first, second);
    }

    /// Жодна позиція реєстру не буває структурно недосяжною. Помилка на одиницю
    /// в межах тасування зазвичай саме так і виглядає: відбір працює, панель
    /// без повторів, але останній присяжний не потрапляє в неї ніколи.
    #[test]
    fn can_reach_every_position_in_the_registry() {
        let candidates = 9u32;
        let mut seen = vec![false; candidates as usize];

        for seed in 0..64u8 {
            for position in draw(&entropy(seed), candidates, 3).expect("a panel exists") {
                seen[position as usize] = true;
            }
        }

        assert!(
            seen.iter().all(|hit| *hit),
            "недосяжні позиції: {:?}",
            seen.iter()
                .enumerate()
                .filter(|(_, hit)| !**hit)
                .map(|(position, _)| position)
                .collect::<Vec<_>>()
        );
    }

    /// Панель завбільшки з реєстр — це перестановка, а не «майже всі».
    #[test]
    fn a_panel_the_size_of_the_registry_takes_everyone() {
        let mut panel = draw(&entropy(3), 6, 6).expect("a panel exists");
        panel.sort_unstable();
        assert_eq!(panel, (0..6).collect::<Vec<u32>>());
    }

    /// Реєстр, менший за панель, — не «панель поменше», а відмова: `FR-010`
    /// рахує кворум від розміру панелі з політики.
    #[test]
    fn refuses_a_registry_smaller_than_the_panel() {
        assert!(draw(&entropy(1), 2, 3).is_err());
        assert!(draw(&entropy(1), 0, 1).is_err());
    }

    #[test]
    fn refuses_a_panel_beyond_the_cap() {
        assert!(draw(&entropy(1), 100, MAX_PANEL_SIZE + 1).is_err());
        assert!(draw(&entropy(1), 100, 0).is_err());
    }

    // ── сисвар ──────────────────────────────────────────────────────────────

    /// Байти сисвара у справжньому розкладі: кількість, далі пари «слот, хеш»
    /// у спадному порядку слотів.
    fn slot_hashes(newest: u64, count: u64) -> Vec<u8> {
        let mut data = count.to_le_bytes().to_vec();
        for offset in 0..count {
            let slot = newest - offset;
            data.extend_from_slice(&slot.to_le_bytes());
            data.extend_from_slice(&[slot as u8; 32]);
        }
        data
    }

    #[test]
    fn finds_a_slot_hash_anywhere_in_the_sysvar() {
        let data = slot_hashes(1_000, 512);

        for slot in [1_000u64, 999, 744, 490, 489] {
            assert_eq!(
                find_slot_hash(&data, slot),
                Some([slot as u8; 32]),
                "{slot}"
            );
        }
    }

    /// Слот, що вийшов за глибину сисвара, — це відмова, а не чужий хеш.
    /// Мовчазний промах дав би панель, виведену не з того, з чого обіцяно.
    #[test]
    fn refuses_a_slot_outside_the_sysvar() {
        let data = slot_hashes(1_000, 512);
        assert_eq!(find_slot_hash(&data, 488), None);
        assert_eq!(find_slot_hash(&data, 1_001), None);
    }

    /// Обрізаний або порожній сисвар не має призводити до читання за межами
    /// буфера: помилка тут — це паніка програми на живому спорі.
    #[test]
    fn survives_a_truncated_sysvar() {
        assert_eq!(find_slot_hash(&[], 1), None);
        assert_eq!(find_slot_hash(&0u64.to_le_bytes(), 1), None);

        let mut lying = slot_hashes(1_000, 4);
        lying[..8].copy_from_slice(&512u64.to_le_bytes());
        assert_eq!(find_slot_hash(&lying, 1), None);
    }
}
