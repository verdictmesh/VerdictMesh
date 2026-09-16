//! Відбитки тверджень сторін — `FR-005`.
//!
//! **Твердження виводяться з ролі, а не заявляються.** У цій угоді позицій
//! рівно дві й вони структурні: виконавець вважає віху зданою й хоче виплату,
//! замовник вважає її незданою й хоче повернення. Третьої позиції в
//! milestone-ескроу не існує.
//!
//! Тому обидва відбитки рахує програма. Альтернатива — дати ініціатору спору
//! подати **обидва** хеші — означала б, що позицію відповідача формулює той,
//! хто проти нього судиться: присяжний побачив би дві позиції, з яких одна
//! написана супротивником, і не мав би як це помітити.
//!
//! **Модуль окремий, бо відбиток рахують двоє.** Програма — щоб відкрити спір;
//! панель присяжного і генератор звіту (T029) — щоб показати, під чим саме
//! підписалась кожна сторона, і довести, що показане збігається з тим, що
//! записано в спорі (`FR-017b` тримається на тій самій ідеї). Дві копії формули
//! не дають помилки компіляції — вони дають твердження, яке неможливо звірити.

use anchor_lang::prelude::*;
use solana_keccak_hasher::hashv;

/// Розділювач простору хешів. Версія в рядку — щоб зміна схеми була видимою
/// зміною, а не тихою розбіжністю зі старими, ще не розглянутими спорами.
const DOMAIN: &[u8] = b"reference_escrow/claim/v1";

/// Чого сторона хоче від віхи.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Position {
    /// «Віху здано, платіть виконавцю.»
    Release,
    /// «Віху не здано, поверніть замовнику.»
    Refund,
}

impl Position {
    /// Позиція сторони за її роллю в угоді. Виконавець ніколи не вимагає
    /// повернення собі, замовник ніколи не вимагає виплати не собі — саме тому
    /// позицію й можна вивести, а не питати.
    pub fn of_seller() -> Self {
        Position::Release
    }

    pub fn of_buyer() -> Self {
        Position::Refund
    }

    /// Байт позиції. Нумерація з одиниці: нуль лишається значенням, якого не
    /// існує, — а нульовий відбиток `open_dispute` відхиляє як відсутнє
    /// твердження (`FR-005`).
    fn tag(self) -> u8 {
        match self {
            Position::Release => 1,
            Position::Refund => 2,
        }
    }
}

/// Відбиток твердження. **Це та формула, яку повторює офчейн** — інших джерел
/// правди про неї немає.
///
/// У хеш входить віха, а не лише угода: та сама позиція щодо іншої віхи — інше
/// твердження, і присяжний, який бачить два однакові відбитки в різних спорах,
/// не має як зрозуміти, чи це збіг, чи копія.
pub fn claim_of(escrow: &Pubkey, milestone: u8, position: Position) -> [u8; 32] {
    hashv(&[DOMAIN, escrow.as_ref(), &[milestone], &[position.tag()]]).0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escrow() -> Pubkey {
        Pubkey::new_unique()
    }

    /// Нульовий відбиток `open_dispute` відхиляє як відсутнє твердження. Хеш
    /// його не дає ні за яких входів, але перевірити це дешевше, ніж дізнатись
    /// на живому спорі.
    #[test]
    fn never_produces_an_empty_claim() {
        let escrow = escrow();

        for milestone in 0..=u8::MAX {
            for position in [Position::Release, Position::Refund] {
                assert_ne!(claim_of(&escrow, milestone, position), [0u8; 32]);
            }
        }
    }

    /// Дві позиції в одному спорі мусять різнитись: однакові відбитки означали
    /// б, що сторони заявили те саме, і спору немає.
    #[test]
    fn tells_the_two_positions_apart() {
        let escrow = escrow();

        assert_ne!(
            claim_of(&escrow, 0, Position::Release),
            claim_of(&escrow, 0, Position::Refund)
        );
    }

    /// Та сама позиція щодо іншої віхи — інше твердження.
    #[test]
    fn binds_the_claim_to_its_milestone() {
        let escrow = escrow();

        assert_ne!(
            claim_of(&escrow, 0, Position::Release),
            claim_of(&escrow, 1, Position::Release)
        );
    }

    /// І до своєї угоди. Без цього відбиток переносився б між угодами, і два
    /// різні спори виглядали б як один.
    #[test]
    fn binds_the_claim_to_its_escrow() {
        assert_ne!(
            claim_of(&escrow(), 0, Position::Release),
            claim_of(&escrow(), 0, Position::Release)
        );
    }

    /// Формула детермінована: офчейн рахує те саме число, інакше звірити
    /// показане твердження з записаним неможливо.
    #[test]
    fn is_deterministic() {
        let escrow = escrow();

        assert_eq!(
            claim_of(&escrow, 3, Position::Refund),
            claim_of(&escrow, 3, Position::Refund)
        );
    }

    /// Позиція виводиться з ролі, і ролі не збігаються.
    #[test]
    fn gives_the_two_roles_opposite_positions() {
        assert_eq!(Position::of_seller(), Position::Release);
        assert_eq!(Position::of_buyer(), Position::Refund);
        assert_ne!(Position::of_seller(), Position::of_buyer());
    }
}
