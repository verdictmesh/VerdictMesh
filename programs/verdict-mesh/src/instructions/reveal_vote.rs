use anchor_lang::prelude::*;

use crate::{
    errors::VerdictMeshError,
    events::VoteRevealed,
    seeds,
    state::{Ballot, Dispute, DisputeState, VoteCommit},
    vote::{commitment_of, SALT_LEN},
};

/// Другий крок голосування — `FR-008a`: присяжний показує голос і секрет, за
/// якими перевіряється поданий раніше відбиток.
///
/// **Це та інструкція, що робить перший крок небезсенсовним.** Сам по собі
/// відбиток не зобов'язує ні до чого — число як число. Зобов'язанням його
/// робить те, що розкрити його можна рівно однією парою «вибір + секрет»,
/// і рахується голос лише тоді, коли пара збіглась.
///
/// **Розбіжність відхиляє транзакцію, а не «не враховує голос».** Формулювання
/// `FR-008a` («не враховується у підрахунку») допускало б і мовчазне
/// проковтування, але тоді присяжний, що помилився секретом, отримав би
/// підтверджену транзакцію і дізнався б про втрату стейку аж на розрахунку.
/// Відмова лишає йому час у вікні розкриття.
///
/// **Перевіряти нічого, крім хеша, тут не треба, і це властивість адреси.**
/// Акаунт голосу виводиться з пари «спір + присяжний», тож його існування вже
/// доводить, що підписант був у панелі, коли подавав відбиток (`commit_vote`
/// без цього акаунт не створює). Повторна перевірка панелі нічого б не додала.
impl RevealVote<'_> {
    pub fn handle(ctx: Context<RevealVote>, choice: Ballot, salt: [u8; SALT_LEN]) -> Result<()> {
        let dispute_key = ctx.accounts.dispute.key();
        let juror = ctx.accounts.juror.key();
        let now = Clock::get()?.unix_timestamp;

        let dispute = &mut ctx.accounts.dispute;
        require!(
            matches!(
                dispute.state,
                DisputeState::Committing | DisputeState::Revealing
            ),
            VerdictMeshError::WrongState
        );
        // `FR-009` на межі: розкритись до кінця вікна подання означає показати
        // свій голос тим, хто ще не зафіксувався. Вікна стикаються — сам
        // дедлайн подання вже належить розкриттю.
        require!(now >= dispute.commit_deadline, VerdictMeshError::WindowOpen);
        require!(
            now < dispute.reveal_deadline,
            VerdictMeshError::WindowClosed
        );

        let vote = &mut ctx.accounts.vote;
        // Одноразово: друге розкриття порахувало б той самий голос двічі, і
        // панель із трьох дала б кворум силами одного присяжного.
        require!(vote.choice.is_none(), VerdictMeshError::AlreadyRevealed);
        // Відбиток першого кола, не розкритий до підрахунку, спізнився назавжди.
        // Дозволити його зараз означало б дати присяжному вирішувати,
        // оприлюднювати свій голос чи ні, **побачивши перший підрахунок**, — і
        // тим скасувати слешинг, який `FR-027b` призначає саме за те мовчання.
        require!(
            vote.round == dispute.round(),
            VerdictMeshError::StaleCommitment
        );
        require!(
            commitment_of(&dispute_key, &juror, choice, &salt) == vote.commitment,
            VerdictMeshError::CommitmentMismatch
        );

        vote.choice = Some(choice);

        // Лічильники живуть у спорі, а не виводяться перебором акаунтів голосів:
        // підрахунок (T019) інакше мусив би отримати всю панель акаунтами і
        // впирався б у ліміт транзакції там, де достатньо двох чисел. Перебір
        // акаунтів однаково буде — але в розрахунку стейків (T020), якому вони
        // потрібні поіменно.
        let counter = match choice {
            Ballot::Claimant => &mut dispute.votes_claimant,
            Ballot::Respondent => &mut dispute.votes_respondent,
        };
        *counter = counter.checked_add(1).ok_or(VerdictMeshError::Overflow)?;

        // Перше розкриття закриває подання і в стані, а не лише за годинником.
        dispute.state = DisputeState::Revealing;

        emit!(VoteRevealed {
            dispute: dispute_key,
            juror,
            choice,
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct RevealVote<'info> {
    /// Підпис присяжного — те, чим акаунт голосу пов'язується з тим, хто його
    /// подавав: гаманець підписанта входить і в адресу акаунта, і в сам хеш.
    /// Тому розкрити чужий голос своїм підписом не можна — ні за присяжного,
    /// який вирішив змовчати, ні проти нього.
    ///
    /// `mut` не потрібне: розкриття не створює акаунтів і не повертає оренди.
    pub juror: Signer<'info>,

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

    /// Акаунт не закривається після розкриття: слешинг (`FR-011`, `FR-008b`,
    /// T020) читає саме його, щоб відрізнити правильний голос від програного, а
    /// обидва — від мовчання. Оренда повертається присяжному там.
    #[account(
        mut,
        seeds = [seeds::VOTE, dispute.key().as_ref(), juror.key().as_ref()],
        bump = vote.bump,
    )]
    pub vote: Account<'info, VoteCommit>,
}
