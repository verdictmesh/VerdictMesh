//! Застава за розгляд — `FR-026a`, `FR-026e`, `FR-026f`.
//!
//! **Навіщо взагалі застава.** `FR-026a` обіцяє, що вартість розгляду несе
//! сторона, проти якої винесено вердикт. В ескроу «переможець забирає віху»
//! виконати цю обіцянку нічим: програвший не отримує **нічого**, з чого можна
//! було б утримати, а взяти з предмета спору означає, що переможець
//! відшкодовує собі сам — арифметично нуль. Потрібне джерело поза предметом
//! спору, і ним стала застава, яку обидві сторони вносять при укладанні угоди.
//!
//! **Застава на кожну віху, а не на угоду.** Спори над різними віхами йдуть
//! одночасно (T022), і одна застава на всю угоду означала б гонку за неї: перший
//! розгляд, що дійшов до виплати, забрав би джерело в усіх інших. Тому при
//! укладанні замикається `deposit × віх` з кожного боку, і кожна віха несе свою.
//!
//! **Застава — в розрахунковому активі протоколу, а не в активі угоди.**
//! Відшкодовується нею депозит (`FR-011a`), і застава в іншому активі не
//! відшкодувала б нічого. Мінт звіряється з `Config` при укладанні — з тієї ж
//! причини, з якої там перевіряється й програма ескроу: у момент спору гроші
//! вже замкнені, а звернутись нікуди.
//!
//! **Розмір застави фіксується при укладанні** (`Escrow.bond`), а не читається
//! з політики під час виплати. Політика інтегратора змінна, і застава, що росла
//! б разом із нею, вимагала б від сторін доносити кошти в угоду, яку вони вже
//! підписали. Наслідок — `FR-026f`: якщо депозит подорожчав після укладання,
//! відшкодовується стільки, скільки замкнено, а різниця не стягується ні з кого.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{errors::EscrowError, seeds, state::Escrow};

/// Скільки застав повертається кожній стороні розгляду, коли віха закрита.
///
/// Сума завжди дорівнює двом заставам: застава — це кошти сторін, і закриття
/// віхи їх перерозподіляє, а не створює й не спалює. Рівність перевіряється
/// тестом, бо саме на ній тримається твердження «ескроу нічого собі не лишає».
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BondSplit {
    pub claimant: u64,
    pub respondent: u64,
}

impl BondSplit {
    /// Розгляд закінчився — куди йдуть дві застави віхи.
    ///
    /// `bond` — застава однієї сторони, `deposit` — депозит, який ініціатор
    /// справді вніс за цей розгляд (знімок політики в самому спорі, `FR-003`).
    ///
    /// **Відшкодування настає рівно тоді, коли виграв ініціатор.** Якщо програв
    /// він сам — обидві застави повертаються: розгляд уже оплачений його
    /// депозитом, і стягнути ще й заставу означало б покарати за ту саму
    /// поразку двічі, подарувавши різницю відповідачу. Так «оплачується рівно
    /// один раз» (`FR-026a`) лишається правдою в обидва боки.
    ///
    /// **Відшкодування обмежене заставою, а не депозитом** — `FR-026f`.
    /// Політика могла подорожчати після укладання угоди; тоді ініціатор
    /// отримує назад скільки замкнено, а решта не стягується ні з кого.
    pub fn on_verdict(bond: u64, deposit: u64, claimant_won: bool) -> Result<Self> {
        let reimbursed = if claimant_won { bond.min(deposit) } else { 0 };

        Ok(Self {
            claimant: bond.checked_add(reimbursed).ok_or(EscrowError::Overflow)?,
            respondent: bond.checked_sub(reimbursed).ok_or(EscrowError::Overflow)?,
        })
    }
}

/// Каса застав і токен-акаунти обох сторін — усе, чим рухається застава.
///
/// Зібрані в одну структуру, бо звертань троє — укладання, закриття віхи без
/// спору і виконання вердикту, — і кожне з них рухає ті самі кошти між тими
/// самими акаунтами. Три копії цього переказу розійшлись би при першій зміні,
/// і розійшлись би мовчки: помилки компіляції розбіжність у сумі не дає.
///
/// Акаунти сторін передаються **завжди обидва** і прив'язані констрейнтами до
/// ролей з угоди — з тієї ж причини, що у виплаті самої віхи: інакше той, хто
/// викликає інструкцію, вибирав би отримувача.
pub struct Bonds<'a, 'info> {
    pub mint: &'a InterfaceAccount<'info, Mint>,
    pub vault: &'a InterfaceAccount<'info, TokenAccount>,
    pub buyer_tokens: &'a InterfaceAccount<'info, TokenAccount>,
    pub seller_tokens: &'a InterfaceAccount<'info, TokenAccount>,
    pub token_program: &'a Interface<'info, TokenInterface>,
}

impl<'info> Bonds<'_, 'info> {
    /// Замикає заставу обох сторін при укладанні угоди: по `bond` на кожну віху
    /// з кожного боку. Підписують самі сторони — це їхні кошти, і жодного
    /// повноваження над ними ескроу до цього моменту не має.
    pub fn hold(
        &self,
        buyer: &AccountInfo<'info>,
        seller: &AccountInfo<'info>,
        bond: u64,
        milestones: u64,
    ) -> Result<()> {
        let per_side = bond.checked_mul(milestones).ok_or(EscrowError::Overflow)?;

        for (from, authority) in [(self.buyer_tokens, buyer), (self.seller_tokens, seller)] {
            self.transfer(from.to_account_info(), authority.clone(), None, per_side)?;
        }

        Ok(())
    }

    /// Повертає обом сторонам заставу однієї віхи — віха закрилась без спору.
    pub fn refund(&self, escrow: &Account<'info, Escrow>, bond: u64) -> Result<()> {
        self.pay(escrow, bond, bond)
    }

    /// Розводить дві застави віхи за наслідком розгляду. Позиції ініціатора й
    /// відповідача переводяться в ролі угоди тут, а не в розрахунку: `BondSplit`
    /// не має знати, хто з них замовник.
    pub fn settle(
        &self,
        escrow: &Account<'info, Escrow>,
        split: BondSplit,
        claimant_is_seller: bool,
    ) -> Result<()> {
        let (buyer, seller) = if claimant_is_seller {
            (split.respondent, split.claimant)
        } else {
            (split.claimant, split.respondent)
        };

        self.pay(escrow, buyer, seller)
    }

    /// Виплата з каси застав підписом самої угоди. Нульова частка не платиться
    /// взагалі: переказ нічого нікуди не переносить, а обчислювальний бюджет
    /// витрачає.
    fn pay(&self, escrow: &Account<'info, Escrow>, buyer: u64, seller: u64) -> Result<()> {
        let deal_id = escrow.deal_id.to_le_bytes();
        let seeds: &[&[u8]] = &[
            seeds::ESCROW,
            escrow.buyer.as_ref(),
            &deal_id,
            &[escrow.bump],
        ];
        let signer_seeds: &[&[&[u8]]] = &[seeds];

        for (to, amount) in [(self.buyer_tokens, buyer), (self.seller_tokens, seller)] {
            if amount == 0 {
                continue;
            }

            self.transfer(
                to.to_account_info(),
                escrow.to_account_info(),
                Some(signer_seeds),
                amount,
            )?;
        }

        Ok(())
    }

    /// Один переказ застави. `signer_seeds` заданий — кошти йдуть **з** каси й
    /// підписує їх сама угода; порожній — кошти йдуть **у** касу від сторони.
    fn transfer(
        &self,
        counterparty: AccountInfo<'info>,
        authority: AccountInfo<'info>,
        signer_seeds: Option<&[&[&[u8]]]>,
        amount: u64,
    ) -> Result<()> {
        let (from, to) = match signer_seeds {
            Some(_) => (self.vault.to_account_info(), counterparty),
            None => (counterparty, self.vault.to_account_info()),
        };

        let accounts = TransferChecked {
            from,
            mint: self.mint.to_account_info(),
            to,
            authority,
        };
        let program = self.token_program.to_account_info();

        let cpi = match signer_seeds {
            Some(seeds) => CpiContext::new_with_signer(program, accounts, seeds),
            None => CpiContext::new(program, accounts),
        };

        transfer_checked(cpi, amount, self.mint.decimals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Демо-конфігурація: депозит і застава по 5 USDC, 6 знаків.
    const BOND: u64 = 5_000_000;

    /// Ініціатор виграв: його застава повертається, застава програвшого йде
    /// йому ж — і рівно покриває депозит, який він вніс, відкриваючи розгляд.
    #[test]
    fn the_loser_pays_the_hearing_and_the_winner_gets_his_deposit_back() {
        let split = BondSplit::on_verdict(BOND, BOND, true).unwrap();

        assert_eq!(split.claimant, 2 * BOND);
        assert_eq!(split.respondent, 0);
    }

    /// Програв сам ініціатор — обидві застави повертаються. Розгляд уже
    /// оплачений його депозитом, і друге стягнення було б покаранням за ту саму
    /// поразку.
    #[test]
    fn a_losing_initiator_pays_once_and_only_with_his_deposit() {
        let split = BondSplit::on_verdict(BOND, BOND, false).unwrap();

        assert_eq!(split.claimant, BOND);
        assert_eq!(split.respondent, BOND);
    }

    /// `FR-026f`: політика подорожчала після укладання угоди. Відшкодовується
    /// стільки, скільки замкнено, і різниця не стягується ні з кого — застава
    /// відповідача не йде в мінус.
    #[test]
    fn a_bond_smaller_than_the_deposit_reimburses_what_there_is() {
        let split = BondSplit::on_verdict(BOND, 3 * BOND, true).unwrap();

        assert_eq!(split.claimant, 2 * BOND);
        assert_eq!(split.respondent, 0);
    }

    /// Політика подешевшала: відшкодовується депозит, а не застава. Решта —
    /// кошти відповідача, і забирати їх немає підстав.
    #[test]
    fn a_bond_larger_than_the_deposit_reimburses_only_the_deposit() {
        let split = BondSplit::on_verdict(BOND, BOND / 5, true).unwrap();

        assert_eq!(split.claimant, BOND + BOND / 5);
        assert_eq!(split.respondent, BOND - BOND / 5);
    }

    /// Ескроу нічого собі не лишає й нічого не створює: скільки застав замкнено
    /// на віху, стільки й вийде — хай яким був вердикт і як розійшлись політики.
    #[test]
    fn the_two_bonds_always_add_up_to_what_was_locked() {
        for bond in [0, 1, 2, 7, 999, BOND, u64::MAX / 2] {
            for deposit in [0, 1, BOND, u64::MAX] {
                for claimant_won in [true, false] {
                    let split = BondSplit::on_verdict(bond, deposit, claimant_won).unwrap();

                    assert_eq!(
                        split.claimant.checked_add(split.respondent),
                        Some(2 * bond),
                        "bond {bond}, deposit {deposit}, won {claimant_won}"
                    );
                }
            }
        }
    }
}
