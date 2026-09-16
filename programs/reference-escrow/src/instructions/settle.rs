use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};
use verdict_mesh::state::{Dispute, DisputeState, Verdict};

use crate::{
    errors::EscrowError,
    events::MilestoneSettled,
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
/// **Депозит за розгляд не відшкодовується, і це не пропуск.** `FR-026a`
/// утримує вартість розгляду «з частки, що належить програвшій стороні, **до
/// виплати їй**» — тобто передбачає, що програвша сторона щось отримує. У спорі
/// над віхою вона не отримує нічого: віху цілком забирає переможець. Джерела,
/// з якого можна було б відшкодувати, в цій угоді не існує, а брати з інших віх
/// означало б переписати умови, на які сторони погоджувались. Тому діє `FR-026c`
/// у чистому вигляді: різниця не стягується ні з кого. Ескроу, що ділить кошти
/// між сторонами, а не віддає їх одній, утримати може — і `T038` мусить сказати
/// інтеграторам, де саме.
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

        ctx.accounts.escrow.entry_mut(milestone)?.state = state;

        emit!(MilestoneSettled {
            escrow: escrow_key,
            milestone,
            dispute: dispute_key,
            winner,
            amount: paid,
        });

        Ok(())
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
    pub escrow: Account<'info, Escrow>,

    /// Розгляд, чий вердикт виконується. Тип із VerdictMesh, тож Anchor звіряє
    /// власника акаунта: виписати собі вердикт, виклавши `Dispute` цією ж
    /// програмою, нічим. **Read-only** — ескроу нічого в чужому стані не міняє.
    pub dispute: Account<'info, Dispute>,

    #[account(address = escrow.mint)]
    pub mint: InterfaceAccount<'info, Mint>,

    /// Обидва токен-акаунти передаються завжди, і кожен прив'язаний до свого
    /// власника з угоди. Передавати лише акаунт переможця означало б дати тому,
    /// хто викликає дозвільну інструкцію, вибирати отримувача.
    #[account(mut, token::mint = mint, token::authority = escrow.buyer)]
    pub buyer_tokens: InterfaceAccount<'info, TokenAccount>,

    #[account(mut, token::mint = mint, token::authority = escrow.seller)]
    pub seller_tokens: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [seeds::ESCROW_VAULT, escrow.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = escrow,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}
