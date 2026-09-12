//! Відбиток голосу присяжного — `FR-008`.
//!
//! Модуль існує окремо від інструкцій з однієї причини: **відбиток рахують
//! двоє**. Присяжний рахує його офчейн, щоб подати (T017), і програма рахує
//! його ще раз при розкритті, щоб звірити (`FR-008a`, T018). Дві копії формули
//! не дають помилки компіляції — вони дають голос, який неможливо розкрити, і
//! присяжного, слешеного за мовчання, якого він не обирав.
//!
//! **Що входить у відбиток і чому саме це.**
//!
//! `juror` — щоб чужий відбиток не можна було скопіювати. Без прив'язки до
//! гаманця повторити чужий `commitment` у своєму акаунті коштує нічого:
//! копіювальник дочекався б, поки автор розкриється, і розкрився б тим самим
//! секретом. Це голос без жодної роботи й без ризику помилитись — тобто рівно
//! те, чого commit-reveal мав не дозволити.
//!
//! `dispute` — щоб відбиток не переносився між спорами. Той самий вибір із тим
//! самим секретом в іншому спорі має давати інше число, інакше присяжний, який
//! голосує в багатьох спорах, розкриває в кожному наступному свій попередній
//! секрет.
//!
//! `DOMAIN` — щоб хеш, порахований деінде в протоколі (відбиток звіту,
//! твердження сторони), не міг випадково збігтися з відбитком голосу. Це
//! копійчана обережність зараз і єдина можлива обережність потім.
//!
//! `salt` — власне те, що ховає голос: варіантів рівно три, і без секрету
//! відбиток перебирається за три спроби.

use anchor_lang::prelude::*;
use solana_keccak_hasher::hashv;

use crate::state::Verdict;

/// Розділювач простору хешів. Версія в рядку — щоб зміна схеми була видимою
/// зміною, а не тихою розбіжністю зі старими, ще не розкритими голосами.
const DOMAIN: &[u8] = b"verdict_mesh/vote/v1";

/// Довжина секрету. 32 байти — не «про запас»: коротший секрет перебирається,
/// а перебраний секрет розкриває голос до вікна розкриття (`FR-009`).
pub const SALT_LEN: usize = 32;

/// Відбиток голосу. **Це та формула, яку повторює SDK** (T035) — інших джерел
/// правди про неї немає.
pub fn commitment_of(
    dispute: &Pubkey,
    juror: &Pubkey,
    choice: Verdict,
    salt: &[u8; SALT_LEN],
) -> [u8; 32] {
    hashv(&[
        DOMAIN,
        dispute.as_ref(),
        juror.as_ref(),
        &[tag(choice)],
        salt,
    ])
    .0
}

/// Байт вибору. Нумерація з одиниці: нуль лишається значенням, якого не існує,
/// тож занулена пам'ять не перетворюється на валідний голос.
///
/// Match без `_` навмисно: новий варіант `Verdict` має зупинити компіляцію тут,
/// а не мовчки отримати чужий байт і зіштовхнути два різні голоси в один
/// відбиток.
fn tag(choice: Verdict) -> u8 {
    match choice {
        Verdict::Claimant => 1,
        Verdict::Respondent => 2,
        Verdict::StatusQuo => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salt(byte: u8) -> [u8; SALT_LEN] {
        [byte; SALT_LEN]
    }

    /// Основа всього: та сама четвірка дає те саме число. Інакше розкрити
    /// власний голос не може навіть той, хто його подав.
    #[test]
    fn the_same_inputs_give_the_same_commitment() {
        let dispute = Pubkey::new_unique();
        let juror = Pubkey::new_unique();

        assert_eq!(
            commitment_of(&dispute, &juror, Verdict::Claimant, &salt(7)),
            commitment_of(&dispute, &juror, Verdict::Claimant, &salt(7)),
        );
    }

    /// Головна перевірка модуля. Скопійований відбиток має бути марним: у
    /// чужому акаунті він не збігається ні з чим, що копіювальник здатен
    /// розкрити, — навіть коли автор уже показав свій секрет усьому світу.
    #[test]
    fn a_copied_commitment_belongs_to_no_one_else() {
        let dispute = Pubkey::new_unique();
        let author = Pubkey::new_unique();
        let copycat = Pubkey::new_unique();

        assert_ne!(
            commitment_of(&dispute, &author, Verdict::Claimant, &salt(7)),
            commitment_of(&dispute, &copycat, Verdict::Claimant, &salt(7)),
        );
    }

    /// Секрет, розкритий в одному спорі, не відмикає голос у іншому.
    #[test]
    fn the_same_secret_gives_a_different_commitment_in_another_dispute() {
        let juror = Pubkey::new_unique();

        assert_ne!(
            commitment_of(&Pubkey::new_unique(), &juror, Verdict::Claimant, &salt(7)),
            commitment_of(&Pubkey::new_unique(), &juror, Verdict::Claimant, &salt(7)),
        );
    }

    /// Вибір мусить бути в хеші: інакше відбиток не зобов'язує ні до чого і
    /// розкрити його можна будь-яким голосом.
    #[test]
    fn every_choice_gives_its_own_commitment() {
        let dispute = Pubkey::new_unique();
        let juror = Pubkey::new_unique();

        let commitments: Vec<[u8; 32]> =
            [Verdict::Claimant, Verdict::Respondent, Verdict::StatusQuo]
                .into_iter()
                .map(|choice| commitment_of(&dispute, &juror, choice, &salt(7)))
                .collect();

        for (position, commitment) in commitments.iter().enumerate() {
            assert!(
                !commitments[position + 1..].contains(commitment),
                "two different choices share a commitment"
            );
        }
    }

    /// Без цього голос ховається лише від тих, хто не здогадався перебрати три
    /// варіанти.
    #[test]
    fn a_different_secret_hides_the_same_choice_differently() {
        let dispute = Pubkey::new_unique();
        let juror = Pubkey::new_unique();

        assert_ne!(
            commitment_of(&dispute, &juror, Verdict::Claimant, &salt(7)),
            commitment_of(&dispute, &juror, Verdict::Claimant, &salt(8)),
        );
    }

    /// Голос не перебирається за три спроби, поки секрет невідомий: жоден із
    /// трьох варіантів із **чужим** секретом не збігається з відбитком.
    #[test]
    fn guessing_the_choice_without_the_secret_matches_nothing() {
        let dispute = Pubkey::new_unique();
        let juror = Pubkey::new_unique();
        let real = commitment_of(&dispute, &juror, Verdict::Respondent, &salt(7));

        for choice in [Verdict::Claimant, Verdict::Respondent, Verdict::StatusQuo] {
            assert_ne!(real, commitment_of(&dispute, &juror, choice, &salt(0)));
        }
    }
}
