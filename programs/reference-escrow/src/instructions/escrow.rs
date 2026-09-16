use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};
use verdict_mesh::{program::VerdictMesh, state::Integrator};

use crate::{
    claims::{claim_of, Position},
    errors::EscrowError,
    events::{EscrowOpened, MilestoneDisputed, MilestoneReleased},
    seeds,
    state::{Escrow, Milestone, MilestoneState, MAX_MILESTONES},
};

/// Створення угоди — замикання коштів.
///
/// **Уся сума замикається наперед, а не по віхах.** Ескроу, у який доносять,
/// не є ескроу: виконавець дізнався б про порожню касу вже після роботи, і
/// арбітраж не мав би над чим виносити вердикт.
///
/// **Виконавець підписує.** Не тому, що приймати гроші треба з дозволу, а тому
/// що разом з угодою він приймає **політику розгляду**: у спорі за цією віхою
/// він буде стороною, а панель, вікна й слешинг задає той `Integrator`, який
/// вибрав замовник. Угода, підписана однією стороною, зробила б відповідачем
/// того, хто на цей арбітраж не погоджувався.
///
/// **Політика перевіряється тут, а не в момент спору.** `Integrator`, який
/// вказує на іншу програму ескроу, не зміг би відкрити спір з цієї — і
/// з'ясувалося б це в найгіршу мить, коли гроші вже замкнені, а звернутись
/// нікуди.
impl CreateEscrow<'_> {
    pub fn handle(ctx: Context<CreateEscrow>, deal_id: u64, milestones: Vec<u64>) -> Result<()> {
        let buyer = ctx.accounts.buyer.key();
        let seller = ctx.accounts.seller.key();
        require_keys_neq!(buyer, seller, EscrowError::InvalidParties);

        require!(
            !milestones.is_empty() && milestones.len() <= MAX_MILESTONES as usize,
            EscrowError::InvalidMilestones
        );

        let mut total: u64 = 0;
        for amount in &milestones {
            // Нульова віха — крок, який неможливо ані виплатити, ані оспорити
            // з користю: спір над нічим `open_dispute` однаково не прийме.
            require!(*amount > 0, EscrowError::InvalidMilestones);
            total = total.checked_add(*amount).ok_or(EscrowError::Overflow)?;
        }

        require_keys_eq!(
            ctx.accounts.integrator.escrow_program,
            crate::ID,
            EscrowError::WrongArbitrationProgram
        );

        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.buyer_tokens.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.vault.to_account_info(),
                    authority: ctx.accounts.buyer.to_account_info(),
                },
            ),
            total,
            ctx.accounts.mint.decimals,
        )?;

        let count = u8::try_from(milestones.len()).map_err(|_| EscrowError::InvalidMilestones)?;

        let escrow = &mut ctx.accounts.escrow;
        escrow.buyer = buyer;
        escrow.seller = seller;
        escrow.mint = ctx.accounts.mint.key();
        escrow.integrator = ctx.accounts.integrator.key();
        escrow.deal_id = deal_id;
        escrow.milestones = milestones
            .into_iter()
            .map(|amount| Milestone {
                amount,
                state: MilestoneState::Pending,
            })
            .collect();
        escrow.bump = ctx.bumps.escrow;

        emit!(EscrowOpened {
            escrow: escrow.key(),
            buyer,
            seller,
            mint: escrow.mint,
            integrator: escrow.integrator,
            total,
            milestones: count,
        });

        Ok(())
    }
}

/// Закриття віхи без спору — щасливий шлях.
///
/// **Закриває віху замовник, і лише він.** Це не привілей: він єдиний, хто
/// може віддати свої гроші добровільно. Дати таке право виконавцю означало б
/// дати йому платити собі, а дозвільний виклик перетворив би ескроу на кран.
/// Незгода замовника — не безвихідь: у виконавця є спір (`dispute`), і
/// відмовляти без підстав коштує депозиту й програного розгляду.
impl ReleaseMilestone<'_> {
    pub fn handle(ctx: Context<ReleaseMilestone>, milestone: u8) -> Result<()> {
        let amount = {
            let entry = ctx.accounts.escrow.entry(milestone)?;
            require!(
                entry.state == MilestoneState::Pending,
                EscrowError::MilestoneNotPending
            );
            entry.amount
        };

        let escrow_key = ctx.accounts.escrow.key();
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
                    to: ctx.accounts.seller_tokens.to_account_info(),
                    authority: ctx.accounts.escrow.to_account_info(),
                },
                signer_seeds,
            ),
            amount,
            ctx.accounts.mint.decimals,
        )?;

        ctx.accounts.escrow.entry_mut(milestone)?.state = MilestoneState::Released;

        emit!(MilestoneReleased {
            escrow: escrow_key,
            milestone,
            amount,
        });

        Ok(())
    }
}

/// Відкриття спору над віхою — `FR-012` з боку інтегратора.
///
/// **Це і є вся інтеграція**, і саме вона міряється в `SC-004`: один CPI,
/// адреса власного PDA як підпис, і два відбитки тверджень. Жодного
/// повноваження VerdictMesh над цими коштами при цьому не з'являється —
/// вердикт звідси **витягнуть** (T023), а не проштовхнуть сюди.
///
/// **Спір іде над віхою, і її номер лишається в стані разом з адресою
/// розгляду.** Без цього виконання вердикту довелось би прив'язувати до віхи
/// за здогадкою, і будь-який інший спір цієї ж угоди зійшовся б за
/// `escrow_ref`.
///
/// **Позиції сторін не заявляються, а виводяться з ролей** — див. `claims`.
/// Ініціатор, який писав би відбиток твердження супротивника, писав би
/// половину того, що побачить присяжний.
impl<'info> DisputeMilestone<'info> {
    pub fn handle(
        ctx: Context<'_, '_, '_, 'info, DisputeMilestone<'info>>,
        milestone: u8,
    ) -> Result<()> {
        let escrow_key = ctx.accounts.escrow.key();
        let claimant = ctx.accounts.claimant.key();
        let buyer = ctx.accounts.escrow.buyer;
        let seller = ctx.accounts.escrow.seller;

        // Спір відкриває сторона угоди. Сторонній не має чого тут вимагати, а
        // депозит за розгляд однаково стягується з ініціатора (`FR-026`) — тобто
        // без цієї перевірки будь-хто міг би замкнути чужу віху за свої гроші.
        require!(
            claimant == buyer || claimant == seller,
            EscrowError::NotAParty
        );

        let amount = {
            let entry = ctx.accounts.escrow.entry(milestone)?;
            require!(
                entry.state == MilestoneState::Pending,
                EscrowError::MilestoneNotPending
            );
            entry.amount
        };

        let (respondent, claimant_position, respondent_position) = if claimant == seller {
            (buyer, Position::of_seller(), Position::of_buyer())
        } else {
            (seller, Position::of_buyer(), Position::of_seller())
        };

        let deal_id = ctx.accounts.escrow.deal_id.to_le_bytes();
        let bump = ctx.accounts.escrow.bump;
        let signer_seeds: &[&[&[u8]]] = &[&[seeds::ESCROW, buyer.as_ref(), &deal_id, &[bump]]];

        verdict_mesh::cpi::open_dispute(
            CpiContext::new_with_signer(
                ctx.accounts.verdict_mesh_program.to_account_info(),
                verdict_mesh::cpi::accounts::OpenDispute {
                    payer: ctx.accounts.claimant.to_account_info(),
                    config: ctx.accounts.config.to_account_info(),
                    settlement_mint: ctx.accounts.settlement_mint.to_account_info(),
                    depositor: ctx.accounts.claimant.to_account_info(),
                    integrator: ctx.accounts.integrator.to_account_info(),
                    escrow: ctx.accounts.escrow.to_account_info(),
                    dispute: ctx.accounts.dispute.to_account_info(),
                    depositor_tokens: ctx.accounts.claimant_tokens.to_account_info(),
                    dispute_vault: ctx.accounts.dispute_vault.to_account_info(),
                    token_program: ctx.accounts.token_program.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                },
                signer_seeds,
            ),
            claimant,
            respondent,
            amount,
            claim_of(&escrow_key, milestone, claimant_position),
            claim_of(&escrow_key, milestone, respondent_position),
        )?;

        // Запис **після** виклику: адреса розгляду має сенс лише тоді, коли за
        // нею справді з'явився спір, а перевіряв її VerdictMesh, а не ми.
        let dispute = ctx.accounts.dispute.key();
        ctx.accounts.escrow.entry_mut(milestone)?.state = MilestoneState::Disputed { dispute };

        emit!(MilestoneDisputed {
            escrow: escrow_key,
            milestone,
            dispute,
            claimant,
            amount,
        });

        Ok(())
    }
}

impl Escrow {
    /// Віха за номером. Окремий помічник, бо звертань троє, і кожне з них — це
    /// індекс у вектор: `get` замість `[]` тут не стиль, а різниця між
    /// зрозумілою помилкою і панікою в програмі.
    pub fn entry(&self, milestone: u8) -> Result<&Milestone> {
        self.milestones
            .get(milestone as usize)
            .ok_or_else(|| EscrowError::UnknownMilestone.into())
    }

    pub fn entry_mut(&mut self, milestone: u8) -> Result<&mut Milestone> {
        self.milestones
            .get_mut(milestone as usize)
            .ok_or_else(|| EscrowError::UnknownMilestone.into())
    }
}

#[derive(Accounts)]
#[instruction(deal_id: u64, milestones: Vec<u64>)]
pub struct CreateEscrow<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    /// Виконавець підписує угоду разом із замовником: він приймає не гроші, а
    /// політику розгляду, за якою його ж і судитимуть.
    pub seller: Signer<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    /// Політика арбітражу, на яку погоджуються обидві сторони. Тип із
    /// VerdictMesh, тож Anchor звіряє й власника акаунта — підсунути сюди
    /// вигаданий `Integrator` нічим.
    pub integrator: Account<'info, Integrator>,

    #[account(
        init,
        payer = buyer,
        space = Escrow::space(milestones.len()),
        seeds = [seeds::ESCROW, buyer.key().as_ref(), &deal_id.to_le_bytes()],
        bump,
    )]
    pub escrow: Account<'info, Escrow>,

    #[account(mut, token::mint = mint, token::authority = buyer)]
    pub buyer_tokens: InterfaceAccount<'info, TokenAccount>,

    /// Каса угоди. Авторитет — сам `Escrow`: приватного ключа до нього не існує,
    /// тож кошти виходять лише тим шляхом, який програма підписала сама.
    #[account(
        init,
        payer = buyer,
        seeds = [seeds::ESCROW_VAULT, escrow.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = escrow,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ReleaseMilestone<'info> {
    pub buyer: Signer<'info>,

    #[account(
        mut,
        seeds = [seeds::ESCROW, escrow.buyer.as_ref(), &escrow.deal_id.to_le_bytes()],
        bump = escrow.bump,
        has_one = buyer @ EscrowError::NotAParty,
    )]
    pub escrow: Account<'info, Escrow>,

    #[account(address = escrow.mint)]
    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        token::mint = mint,
        token::authority = escrow.seller,
    )]
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

/// Акаунти VerdictMesh тут навмисно **не типізовані**: їх перевіряє той, кому
/// вони належать. Ескроу повторював би чужі констрейнти, розходився б з ними
/// при першій зміні — і кожен інтегратор мусив би повторити те саме.
/// Єдина перевірка, яку інтегратор зобов'язаний зробити сам, — що спір іде під
/// **тією** політикою, на яку сторони погодились.
#[derive(Accounts)]
pub struct DisputeMilestone<'info> {
    #[account(mut)]
    pub claimant: Signer<'info>,

    #[account(
        mut,
        seeds = [seeds::ESCROW, escrow.buyer.as_ref(), &escrow.deal_id.to_le_bytes()],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, Escrow>,

    /// CHECK: перевіряє VerdictMesh; звідси йде єдина вимога — це має бути та
    /// сама політика, під якою укладалась угода.
    #[account(mut, constraint = integrator.key() == escrow.integrator @ EscrowError::WrongIntegrator)]
    pub integrator: UncheckedAccount<'info>,

    /// CHECK: глобальний акаунт VerdictMesh; перевіряє VerdictMesh.
    pub config: UncheckedAccount<'info>,

    /// CHECK: розрахунковий актив протоколу; перевіряє VerdictMesh.
    pub settlement_mint: UncheckedAccount<'info>,

    /// CHECK: акаунт спору; його адресу виводить і перевіряє VerdictMesh.
    #[account(mut)]
    pub dispute: UncheckedAccount<'info>,

    /// CHECK: токени ініціатора під депозит за розгляд; перевіряє VerdictMesh.
    #[account(mut)]
    pub claimant_tokens: UncheckedAccount<'info>,

    /// CHECK: сховище депозиту цього спору; перевіряє VerdictMesh.
    #[account(mut)]
    pub dispute_vault: UncheckedAccount<'info>,

    pub verdict_mesh_program: Program<'info, VerdictMesh>,

    /// Програма токена **розрахункового** активу, а не активу угоди: тут
    /// рухається лише депозит.
    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}
