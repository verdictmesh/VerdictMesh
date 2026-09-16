//! Оплата розгляду — `FR-026`, `FR-026a`, `FR-026b`, `FR-026c`.
//!
//! **Депозит і є ціна розгляду.** У політиці одне число — `Policy.deposit`, — і
//! воно ж оголошене стороні до відкриття спору (`FR-026d`). Другого числа, з
//! якого можна було б рахувати «вартість розгляду» окремо від внеску, не існує,
//! і вигадувати його означало б дати інтегратору спосіб оголосити одну суму, а
//! списати іншу.
//!
//! **Депозит завжди платить ініціатор, і завжди наперед.** Він лежить у
//! сховищі спору з моменту відкриття, тож розрахунок ділить те, що вже є, і не
//! залежить від того, чи встигла відпрацювати чужа програма. Питання «хто
//! зрештою поніс вартість» (`FR-026a`) вирішує **ескроу** при виплаті: якщо
//! ініціатор виграв, ескроу утримує цю суму з частки програвшої сторони й
//! віддає її ініціатору. Інакше довелось би вимагати, щоб ескроу переказав
//! комісію **до** нашого розрахунку — тобто зав'язати виплату присяжним на
//! порядок виконання в чужій програмі, над якою в нас немає влади (`FR-012`).
//!
//! **Протокол — останній у черзі.** Частка присяжних рахується від оголошеної
//! ціни й обмежується тим, що справді зібрано; протокол забирає залишок. Це
//! єдиний порядок, при якому нестача не падає на присяжного: він рахує свій
//! заробіток наперед і не має як перевірити, скільки дійшло до сховища
//! (`FR-026c`).

use anchor_lang::prelude::*;

use crate::{errors::VerdictMeshError, state::BPS_DENOMINATOR};

/// Частка присяжних в оплаті розгляду — 80%, решта протоколу.
///
/// **Константа протоколу, а не поле `Policy`.** Інтегратор не має підстав
/// вирішувати, як протокол платить власним присяжним: реєстр один на всіх, і
/// політика, що віддає присяжним нуль, зробила б службу в панелі збитковою в
/// **усіх** інтеграторів одразу — досить одного спору за такою політикою, щоб
/// відібрана панель працювала безкоштовно. Розмір самого депозиту інтегратор
/// задає (`FR-026d`); ділення вже зібраного — справа протоколу.
pub const JUROR_FEE_BPS: u16 = 8_000;

/// Як розходиться зібрана оплата розгляду — `FR-026b`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FeeSplit {
    pub jurors: u64,
    pub protocol: u64,
}

/// Ділить сховище спору між присяжними і протоколом.
///
/// `balance` — скільки насправді зібрано, `cost` — оголошена ціна розгляду.
/// Числа збігаються завжди, доки в сховищі лежить рівно депозит, і розходяться
/// рівно там, де щось пішло не так; тоді недоотримує протокол, а не присяжний.
///
/// `has_winners` — чи є кому платити частку присяжних. Панель, що змовчала
/// цілком, переможців не має (T020), і призначена їм частка не має де осісти:
/// сховище спору інструкція закриває, тож лишити суму в ньому означало б
/// знищити її разом з акаунтом.
pub fn split_fee(balance: u64, cost: u64, has_winners: bool) -> Result<FeeSplit> {
    let jurors = if has_winners {
        share_of(cost, JUROR_FEE_BPS)?.min(balance)
    } else {
        0
    };

    Ok(FeeSplit {
        jurors,
        // Відняти можна завжди: частка присяжних обмежена самим балансом.
        protocol: balance
            .checked_sub(jurors)
            .ok_or(VerdictMeshError::Overflow)?,
    })
}

/// Частка від суми в базисних пунктах. Через `u128`, бо добуток `u64` на десять
/// тисяч не вміщається в `u64` — а це гроші, а не лічильник.
///
/// Ділення вниз: залишок дістається тому, хто в своєму розрахунку останній.
/// Округлення вгору роздало б більше, ніж зібрано, і першим це помітив би той,
/// кому не вистачило.
pub fn share_of(amount: u64, bps: u16) -> Result<u64> {
    let share = u128::from(amount)
        .checked_mul(u128::from(bps))
        .ok_or(VerdictMeshError::Overflow)?
        / u128::from(BPS_DENOMINATOR);

    u64::try_from(share).map_err(|_| VerdictMeshError::Overflow.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Демо-конфігурація: депозит 5 USDC, 6 знаків.
    const COST: u64 = 5_000_000;

    #[test]
    fn splits_the_declared_cost_four_to_one() {
        let split = split_fee(COST, COST, true).unwrap();

        assert_eq!(split.jurors, 4_000_000);
        assert_eq!(split.protocol, 1_000_000);
    }

    /// `FR-026c`: недобір падає на протокол, а не на присяжного. Присяжний
    /// рахує свій заробіток наперед і не має як перевірити, скільки дійшло.
    #[test]
    fn a_short_balance_costs_the_protocol_first() {
        let split = split_fee(4_500_000, COST, true).unwrap();

        assert_eq!(split.jurors, 4_000_000);
        assert_eq!(split.protocol, 500_000);
    }

    /// Недобір, глибший за частку протоколу, урізає і присяжних — але лише
    /// після того, як протокол недоотримав усе.
    #[test]
    fn a_balance_below_the_juror_share_leaves_the_protocol_nothing() {
        let split = split_fee(1_000_000, COST, true).unwrap();

        assert_eq!(split.jurors, 1_000_000);
        assert_eq!(split.protocol, 0);
    }

    /// Панель, що змовчала цілком: переможців немає, і частка присяжних не має
    /// де осісти. Вона йде протоколу, а не лишається у сховищі, яке
    /// закривається.
    #[test]
    fn without_winners_the_whole_payment_goes_to_the_protocol() {
        let split = split_fee(COST, COST, false).unwrap();

        assert_eq!(split.jurors, 0);
        assert_eq!(split.protocol, COST);
    }

    #[test]
    fn an_empty_vault_splits_into_nothing() {
        assert_eq!(
            split_fee(0, COST, true).unwrap(),
            FeeSplit {
                jurors: 0,
                protocol: 0
            }
        );
    }

    /// Ділення нічого не створює і нічого не втрачає: сума часток дорівнює
    /// балансу при будь-якому з входів. Це та рівність, через яку сховище можна
    /// закрити одразу після розрахунку.
    #[test]
    fn the_two_shares_always_add_up_to_the_balance() {
        for balance in [0, 1, 2, 3, 7, 999, 1_000_000, COST, u64::MAX] {
            for has_winners in [true, false] {
                let split = split_fee(balance, COST, has_winners).unwrap();

                assert_eq!(
                    split.jurors.checked_add(split.protocol),
                    Some(balance),
                    "balance {balance}, winners {has_winners}"
                );
            }
        }
    }

    /// Копійка з нерівного ділення дістається протоколу: він і є залишковим
    /// отримувачем. Присяжним обіцяна частка від оголошеної ціни, і вона
    /// виплачена повністю.
    #[test]
    fn the_rounding_dust_goes_to_the_protocol() {
        let split = split_fee(3, 3, true).unwrap();

        // 3 * 8000 / 10000 = 2.4 → 2.
        assert_eq!(split.jurors, 2);
        assert_eq!(split.protocol, 1);
    }

    #[test]
    fn a_full_share_of_the_largest_amount_does_not_overflow() {
        assert_eq!(share_of(u64::MAX, BPS_DENOMINATOR).unwrap(), u64::MAX);
    }

    #[test]
    fn a_zero_share_is_zero() {
        assert_eq!(share_of(u64::MAX, 0).unwrap(), 0);
    }
}
