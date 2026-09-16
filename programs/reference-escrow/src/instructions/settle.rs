use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};
use verdict_mesh::state::{Dispute, DisputeState, Verdict};

use crate::{
    errors::EscrowError,
    events::MilestoneSettled,
    instructions::bond::{BondSplit, Bonds},
    seeds,
    state::{Escrow, MilestoneState},
};

/// Виконання вердикту — `FR-012`, `FR-013`.
///
/// **Вердикт витягують, а не проштовхують.** Ескроу читає `Dispute` як
/// звичайний ончейн-стан і розподіляє кошти сам. VerdictMesh не викликає сюди
/// нічого і не має тут жодного повноваження — `FR-014` тримається тим, що
/// такої інструкції не існує, а не тим, що нею не користуються.
///
/// **Перевірок чотири, а не три.** Спір наш (`escrow_ref`), вердикт винесено,
/// вікно апеляції минуло і спір не під апеляцією — і четверта, найважливіша:
/// адреса розгляду мусить збігтися з тією, що лежить **у стані віхи**. Без неї
/// будь-який інший спір цієї ж угоди — навіть давно виконаний — зійшовся б за
/// `escrow_ref` і розпорядився б віхою, якої не стосувався. Вона ж і робить
/// повторне виконання неможливим (`FR-013`): після виплати адреси в стані вже
/// немає, і другий виклик не має за що зачепитись. Окремого прапорця
/// «виконано» тому й не заведено — він був би другим джерелом того самого
/// факту.
///
/// **Стан `Finalized` тут не потрібен.** Розрахунок стейків панелі — внутрішня
/// справа VerdictMesh; чекати на його дозвільний кранк означало б тримати чужі
/// кошти замкненими доти, доки комусь не стане цікаво його викликати.
///
/// **Інструкція нічия — і без підпису взагалі.** `SC-005` вимагає розглядів без
/// ручного втручання, тобто без привілейованої людини в контурі. Закривати тут
/// нічого, тож і отримувача оренди, заради якого `settle_stakes` бере підпис
/// кранка, немає. Напрямок виплати ззовні не підказати: обидва токен-акаунти
/// прив'язані до сторін угоди констрейнтами.
///
/// **Депозит відшкодовується із застави, а не з предмета спору** — `FR-026a`,
/// `FR-026e`. Утримувати з частки програвшого нічим: віху цілком забирає
/// переможець, і програвша сторона не отримує **нічого**. Тому джерело лежить
/// поза предметом спору — застава, замкнена з обох боків при укладанні угоди
/// (`instructions::bond`). Переможець-ініціатор отримує назад свою заставу і
/// депозит із застави програвшого; програв сам ініціатор — обидві застави
/// повертаються, бо розгляд уже оплачений його депозитом.
///
/// **Статус-кво застав не рухає.** Віха повертається в `Pending`, тобто лишається
/// предметом, який ще може стати спором, і застава мусить лишатись за нею —
/// інакше наступний розгляд над тією ж віхою не мав би з чого відшкодовувати.
/// Ніхто при цьому застави не втрачає: вона повернеться разом із віхою, коли ту
/// нарешті закриють. Ціна невдалої ескалації лишається рівно одна — депозит
/// ініціатора (`FR-027a`).
impl<'info> SettleMilestone<'info> {
    pub fn handle(
        ctx: Context<'_, '_, '_, 'info, SettleMilestone<'info>>,
        milestone: u8,
    ) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let escrow_key = ctx.accounts.escrow.key();
        let dispute_key = ctx.accounts.dispute.key();
        let dispute = &ctx.accounts.dispute;

        require_keys_eq!(dispute.escrow_ref, escrow_key, EscrowError::NotOurDispute);
        let verdict = dispute.verdict.ok_or(EscrowError::VerdictPending)?;
        // Окремою перевіркою, бо обидві часові умови спір під апеляцією
        // проходить: вердикт першого кола в нього вже є, а дедлайн уже минув.
        require!(
            dispute.state != DisputeState::Appealed,
            EscrowError::VerdictUnderAppeal
        );
        require!(
            now >= dispute.appeal_deadline,
            EscrowError::AppealWindowOpen
        );

        let amount = {
            let entry = ctx.accounts.escrow.entry(milestone)?;
            require!(
                entry.state
                    == MilestoneState::Disputed {
                        dispute: dispute_key
                    },
                EscrowError::MilestoneNotUnderThisDispute
            );
            entry.amount
        };

        // `Verdict::Claimant` — «позиція ініціатора перемогла», а не «платіть
        // позивачу»: позиція виводиться з ролі (див. `claims`), тож напрямок
        // виплати залежить від того, хто відкривав спір.
        let winner = match verdict {
            Verdict::Claimant => Some(dispute.claimant),
            Verdict::Respondent => Some(dispute.respondent),
            // `FR-027a`: «як ніби спору не було». Віха повертається туди, звідки
            // її взяли, і сторони лишаються там, де були.
            Verdict::StatusQuo => None,
        };

        let (state, paid) = match winner {
            Some(winner) if winner == ctx.accounts.escrow.seller => {
                Self::pay(&ctx, &ctx.accounts.seller_tokens, amount)?;
                (MilestoneState::Released, amount)
            }
            Some(winner) if winner == ctx.accounts.escrow.buyer => {
                Self::pay(&ctx, &ctx.accounts.buyer_tokens, amount)?;
                (MilestoneState::Refunded, amount)
            }
            // Недосяжно за побудовою: адреса розгляду в стані віхи доводить, що
            // спір відкрила ця ж програма, а сторонами вона ставить лише свої.
            // Мовчазна гілка тут вгадувала б отримувача чужих коштів.
            Some(_) => return Err(EscrowError::VerdictNamesAStranger.into()),
            None => (MilestoneState::Pending, 0),
        };

        // Застави розходяться лише разом із віхою. Статус-кво лишає її
        // `Pending` — і застава лишається за нею.
        let reimbursed = if winner.is_some() {
            let split = BondSplit::on_verdict(
                ctx.accounts.escrow.bond,
                // Депозит із самого спору, а не з політики інтегратора:
                // політика змінна, а спір несе знімок (`FR-003`), за яким
                // ініціатор і платив.
                dispute.policy.deposit,
                verdict == Verdict::Claimant,
            )?;
            ctx.accounts.bonds().settle(
                &ctx.accounts.escrow,
                split,
                dispute.claimant == ctx.accounts.escrow.seller,
            )?;

            split
                .claimant
                .checked_sub(ctx.accounts.escrow.bond)
                .ok_or(EscrowError::Overflow)?
        } else {
            0
        };

        ctx.accounts.escrow.entry_mut(milestone)?.state = state;

        emit!(MilestoneSettled {
            escrow: escrow_key,
            milestone,
            dispute: dispute_key,
            winner,
            amount: paid,
            reimbursed,
        });

        Ok(())
    }

    /// Каса застав і токен-акаунти сторін у розрахунковому активі — рух застави
    /// живе в одному місці на всі три інструкції: `instructions::bond`.
    fn bonds(&self) -> Bonds<'_, 'info> {
        Bonds {
            mint: &self.settlement_mint,
            vault: &self.bond_vault,
            buyer_tokens: &self.buyer_bond_tokens,
            seller_tokens: &self.seller_bond_tokens,
            token_program: &self.settlement_token_program,
        }
    }

    /// Виплата з каси угоди підписом самої угоди. Приватного ключа до її PDA не
    /// існує, тож кошти виходять лише тим шляхом, який програма щойно
    /// перевірила.
    fn pay(
        ctx: &Context<'_, '_, '_, 'info, SettleMilestone<'info>>,
        to: &InterfaceAccount<'info, TokenAccount>,
        amount: u64,
    ) -> Result<()> {
        let buyer = ctx.accounts.escrow.buyer;
        let deal_id = ctx.accounts.escrow.deal_id.to_le_bytes();
        let bump = ctx.accounts.escrow.bump;
        let signer_seeds: &[&[&[u8]]] = &[&[seeds::ESCROW, buyer.as_ref(), &deal_id, &[bump]]];

        transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.vault.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    to: to.to_account_info(),
                    authority: ctx.accounts.escrow.to_account_info(),
                },
                signer_seeds,
            ),
            amount,
            ctx.accounts.mint.decimals,
        )
    }
}

#[derive(Accounts)]
pub struct SettleMilestone<'info> {
    #[account(
        mut,
        seeds = [seeds::ESCROW, escrow.buyer.as_ref(), &escrow.deal_id.to_le_bytes()],
        bump = escrow.bump,
    )]
    pub escrow: Box<Account<'info, Escrow>>,

    /// Розгляд, чий вердикт виконується. Тип із VerdictMesh, тож Anchor звіряє
    /// власника акаунта: виписати собі вердикт, виклавши `Dispute` цією ж
    /// програмою, нічим. **Read-only** — ескроу нічого в чужому стані не міняє.
    pub dispute: Box<Account<'info, Dispute>>,

    #[account(address = escrow.mint)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

    /// Обидва токен-акаунти передаються завжди, і кожен прив'язаний до свого
    /// власника з угоди. Передавати лише акаунт переможця означало б дати тому,
    /// хто викликає дозвільну інструкцію, вибирати отримувача.
    #[account(mut, token::mint = mint, token::authority = escrow.buyer)]
    pub buyer_tokens: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, token::mint = mint, token::authority = escrow.seller)]
    pub seller_tokens: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [seeds::ESCROW_VAULT, escrow.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = escrow,
    )]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(address = escrow.settlement_mint)]
    pub settlement_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Застава розходиться між обома сторонами, і навіть коли одна з часток
    /// нульова, обидва акаунти передаються — інакше той, хто викликає дозвільну
    /// інструкцію, вибирав би, кому дістанеться відшкодування.
    #[account(mut, token::mint = settlement_mint, token::authority = escrow.buyer)]
    pub buyer_bond_tokens: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, token::mint = settlement_mint, token::authority = escrow.seller)]
    pub seller_bond_tokens: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [seeds::BOND_VAULT, escrow.key().as_ref()],
        bump,
        token::mint = settlement_mint,
        token::authority = escrow,
    )]
    pub bond_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,

    pub settlement_token_program: Interface<'info, TokenInterface>,
}
