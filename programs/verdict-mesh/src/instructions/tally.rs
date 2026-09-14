use anchor_lang::prelude::*;

use crate::{
    errors::VerdictMeshError,
    events::{DisputeEscalated, DisputeTallied},
    seeds,
    state::{Dispute, DisputeState, Verdict},
};

/// Підрахунок голосів — `FR-010`, `FR-027`, `FR-027a`.
///
/// **Спір мусить закінчитись, і закінчує його ця інструкція.** `SC-011` вимагає
/// результату у 100% розглядів, де частина панелі змовчала. Тому виходів рівно
/// три і всі скінченні: є кворум і більшість — вердикт; немає — один раз
/// розширена панель; не вийшло і там — статус-кво. Третього кола не існує.
///
/// **Кворум і більшість — різні питання.** Кворум відповідає, чи висловилось
/// достатньо присяжних; більшість — чи є з цього відповідь. Рівність при
/// набраному кворумі вердикту не дає і йде на ескалацію нарівні з мовчанням.
///
/// **Ескалація не скидає голоси першого раунду.** Розширена панель — той самий
/// розгляд, продовжений більшою кількістю присяжних. Скидання лічильників
/// зробило б уже розкриті `VoteCommit` нерозкривними назавжди (`AlreadyRevealed`
/// не знімається), а тих, хто чесно проголосував, змусило б голосувати вдруге
/// ні за чим.
///
/// **Інструкція нічия.** Дозвільний кранк без підпису: перехід залежить від
/// часу, а не від чиєїсь волі, і дозволити його комусь одному означало б лише
/// дати можливість не викликати підрахунок узагалі.
///
/// **`active_disputes` тут не зменшується, і це не пропуск.** Спір із винесеним
/// вердиктом ще не фіналізовано — попереду вікно апеляції й розрахунок стейків.
/// Присяжний, звільнений тут, вийшов би з реєстру зі стейком (`FR-007`, T015) за
/// мить до того, як його мали слешити. Лічильник знімає T020, який і так
/// перебирає всю панель поіменно.
impl Tally<'_> {
    pub fn handle(ctx: Context<Tally>) -> Result<()> {
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;
        let dispute_key = ctx.accounts.dispute.key();
        let dispute = &mut ctx.accounts.dispute;

        require!(
            matches!(
                dispute.state,
                DisputeState::Committing | DisputeState::Revealing
            ),
            VerdictMeshError::WrongState
        );
        // Рахувати до кінця вікна розкриття означає рахувати без тих, хто ще
        // встиг би розкритись. Верхньої межі немає: спір, до якого дійшли руки
        // пізно, мусить дорахуватись, а не застрягнути.
        require!(now >= dispute.reveal_deadline, VerdictMeshError::WindowOpen);

        let policy = dispute.policy;
        let quorum = if dispute.escalated {
            policy.extended_quorum
        } else {
            policy.quorum
        };
        let revealed = dispute
            .votes_claimant
            .checked_add(dispute.votes_respondent)
            .ok_or(VerdictMeshError::Overflow)?;

        let verdict = match dispute.votes_claimant.cmp(&dispute.votes_respondent) {
            _ if revealed < quorum => None,
            std::cmp::Ordering::Greater => Some(Verdict::Claimant),
            std::cmp::Ordering::Less => Some(Verdict::Respondent),
            std::cmp::Ordering::Equal => None,
        };

        match verdict {
            Some(verdict) => Self::decide(dispute, dispute_key, verdict, now, policy.appeal_window),
            None if dispute.escalated => {
                // `FR-027a`: розширена панель теж не дала результату. Кошти
                // розподіляються так, ніби спору не було.
                Self::decide(
                    dispute,
                    dispute_key,
                    Verdict::StatusQuo,
                    now,
                    policy.appeal_window,
                )
            }
            None => Self::escalate(dispute, dispute_key, now, clock.slot),
        }
    }

    fn decide(
        dispute: &mut Dispute,
        dispute_key: Pubkey,
        verdict: Verdict,
        now: i64,
        appeal_window: i64,
    ) -> Result<()> {
        let appeal_deadline = now
            .checked_add(appeal_window)
            .ok_or(VerdictMeshError::Overflow)?;

        dispute.verdict = Some(verdict);
        dispute.state = DisputeState::Tallied;
        // Вікно апеляції відлічується від вердикту, а не від відкриття спору:
        // до цієї миті його тривалість нічого не означала.
        dispute.appeal_deadline = appeal_deadline;

        emit!(DisputeTallied {
            dispute: dispute_key,
            verdict,
            votes_claimant: dispute.votes_claimant,
            votes_respondent: dispute.votes_respondent,
            appeal_deadline,
        });

        Ok(())
    }

    /// Перехід на розширену панель — `FR-027`. Саму панель добирає
    /// `select_panel`: тягнути реєстр через підрахунок означало б зробити
    /// дозвільний кранк дорогим рівно настільки, наскільки великий реєстр.
    fn escalate(dispute: &mut Dispute, dispute_key: Pubkey, now: i64, slot: u64) -> Result<()> {
        let commit_deadline = now
            .checked_add(dispute.policy.commit_window)
            .ok_or(VerdictMeshError::Overflow)?;
        let reveal_deadline = commit_deadline
            .checked_add(dispute.policy.reveal_window)
            .ok_or(VerdictMeshError::Overflow)?;

        dispute.escalated = true;
        dispute.state = DisputeState::Committing;
        dispute.commit_deadline = commit_deadline;
        dispute.reveal_deadline = reveal_deadline;
        // **Ентропія перезакріплюється, і це друге відоме слабке місце її
        // джерела.** `SlotHashes` пам'ятає 512 слотів — близько 3.5 хвилини, —
        // а між відкриттям спору і ескалацією проходять обидва вікна, тобто
        // стільки, скільки записав інтегратор у політику. Слот відкриття до
        // цієї миті вже не існує, і добирати присяжних до розширеної панелі
        // було б нічим: ескалація вела б у глухий кут, а `SC-011` не досягався
        // б узагалі.
        //
        // Ціна названа вголос: той, хто викликає підрахунок, обирає, у якому
        // слоті це зробити, тобто може переграти добір, повторюючи спробу.
        // Стримує його лише те, що інструкція нічия — перший виклик закріплює
        // слот, і гріфер мусить випередити всіх, кому спір небайдужий, рівно
        // один раз. Перехід на VRF (`FR-006a`) знімає і це, і вплив лідера
        // слота одним рухом, не зачіпаючи нічого поза `panel.rs`.
        dispute.entropy_slot = slot.saturating_sub(1);

        emit!(DisputeEscalated {
            dispute: dispute_key,
            votes_claimant: dispute.votes_claimant,
            votes_respondent: dispute.votes_respondent,
            commit_deadline,
            reveal_deadline,
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Tally<'info> {
    #[account(
        mut,
        seeds = [
            seeds::DISPUTE,
            dispute.integrator.as_ref(),
            &dispute.dispute_id.to_le_bytes(),
        ],
        bump = dispute.bump,
    )]
    pub dispute: Account<'info, Dispute>,
}
