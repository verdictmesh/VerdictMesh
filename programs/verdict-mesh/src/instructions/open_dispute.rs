use anchor_lang::prelude::*;

use crate::{
    errors::VerdictMeshError,
    events::DisputeOpened,
    seeds,
    state::{Dispute, DisputeState, Integrator},
};

impl OpenDispute<'_> {
    /// Відкриває спір над замкненим залишком — `FR-004`.
    ///
    /// Політика копіюється, а не позичається: `FR-003` вимагає, щоб правила
    /// розгляду не змінювались до фіналізації, а посилання на `Integrator`
    /// цього не дає.
    ///
    /// Депозит за розгляд тут ще не переказується — сховище депозитів з'являється
    /// разом із рештою токен-логіки (T021, `FR-026`). Сума, яку винна сторона,
    /// уже зафіксована знімком політики, тож переказ додається, не змінюючи
    /// нічого із записаного тут.
    pub fn handle(
        ctx: Context<OpenDispute>,
        claimant: Pubkey,
        respondent: Pubkey,
        amount: u64,
        claimant_claim_hash: [u8; 32],
        respondent_claim_hash: [u8; 32],
    ) -> Result<()> {
        require_keys_neq!(claimant, respondent, VerdictMeshError::InvalidParties);
        require!(amount > 0, VerdictMeshError::InvalidAmount);
        // FR-005: нульовий відбиток — не «порожнє твердження», а відсутнє.
        // Присяжний побачив би позицію лише однієї сторони і не мав би як це
        // помітити.
        require!(
            claimant_claim_hash != [0u8; 32] && respondent_claim_hash != [0u8; 32],
            VerdictMeshError::MissingClaim
        );

        let policy = ctx.accounts.integrator.policy;
        let opened_at = Clock::get()?.unix_timestamp;
        let commit_deadline = opened_at
            .checked_add(policy.commit_window)
            .ok_or(VerdictMeshError::Overflow)?;
        let reveal_deadline = commit_deadline
            .checked_add(policy.reveal_window)
            .ok_or(VerdictMeshError::Overflow)?;

        let integrator = &mut ctx.accounts.integrator;
        let dispute_id = integrator.dispute_count;
        integrator.dispute_count = dispute_id
            .checked_add(1)
            .ok_or(VerdictMeshError::Overflow)?;

        let dispute = &mut ctx.accounts.dispute;
        dispute.integrator = integrator.key();
        dispute.dispute_id = dispute_id;
        dispute.policy = policy;
        dispute.escrow_ref = ctx.accounts.escrow.key();
        dispute.claimant = claimant;
        dispute.respondent = respondent;
        dispute.amount = amount;
        dispute.state = DisputeState::Committing;
        dispute.panel = Vec::new();
        dispute.report_hash = [0u8; 32];
        dispute.claimant_claim_hash = claimant_claim_hash;
        dispute.respondent_claim_hash = respondent_claim_hash;
        dispute.opened_at = opened_at;
        // Ентропію відбору фіксуємо тут, а не там, де відбувається сам відбір
        // (T016): інакше його можна було б переграти, повторюючи спробу зі
        // слота в слот, доки панель не сподобається. Береться слот перед
        // поточним — хеш поточного ще не існує.
        dispute.entropy_slot = Clock::get()?.slot.saturating_sub(1);
        dispute.commit_deadline = commit_deadline;
        dispute.reveal_deadline = reveal_deadline;
        // Вікно апеляції відкривається від вердикту, а не від відкриття спору.
        dispute.appeal_deadline = 0;
        dispute.votes_claimant = 0;
        dispute.votes_respondent = 0;
        dispute.revealed_count = 0;
        dispute.escalated = false;
        dispute.verdict = None;
        dispute.settled = false;
        dispute.bump = ctx.bumps.dispute;

        // FR-029: за подіями зовнішній спостерігач відновлює хронологію спору
        // без доступу до офчейн-сервісу. Watcher (T027) читає саме цю.
        emit!(DisputeOpened {
            dispute: dispute.key(),
            integrator: dispute.integrator,
            escrow_ref: dispute.escrow_ref,
            claimant,
            respondent,
            amount,
            optimistic: false,
            opened_at,
        });

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(claimant: Pubkey, respondent: Pubkey)]
pub struct OpenDispute<'info> {
    /// Оренду акаунта спору платить той, хто ініціює транзакцію, а не PDA
    /// ескроу: у PDA може не бути лампортів, і вимагати їх від нього означало б
    /// вимагати від інтегратора тримати баланс у чужій програмі.
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds = [seeds::INTEGRATOR, integrator.authority.as_ref()],
        bump = integrator.bump,
    )]
    pub integrator: Account<'info, Integrator>,

    /// Акаунт ескроу, над коштами якого йде спір. Він і стає `escrow_ref` —
    /// єдиним, за чим ескроу згодом упізнає «свій» спір, перш ніж розподілити
    /// кошти (`FR-012`).
    ///
    /// Тому вимог дві, і жодна не зайва. **Власник** — програма, яку інтегратор
    /// вказав при реєстрації: інакше `escrow_ref` вказував би на що завгодно.
    /// **Підпис** — бо власності замало: акаунт чужої програми може прочитати
    /// будь-хто, і без підпису вистачило б підставити чужий ескроу, щоб
    /// змусити його виконати вигаданий вердикт.
    ///
    /// CHECK: вміст акаунта програмі невідомий і не потрібен; перевіряються
    /// власник і підпис.
    #[account(signer, owner = integrator.escrow_program)]
    pub escrow: UncheckedAccount<'info>,

    /// Номер спору бере лічильник інтегратора, а не клієнт: інакше нумерація
    /// перестала б бути щільною, і `dispute_count` не був би правдою про те,
    /// скільки спорів існує.
    ///
    /// Місце виділяється під **розширену** панель — див. `Dispute::space`.
    #[account(
        init,
        payer = payer,
        space = Dispute::space(integrator.policy.extended_panel_size),
        seeds = [
            seeds::DISPUTE,
            integrator.key().as_ref(),
            &integrator.dispute_count.to_le_bytes(),
        ],
        bump,
    )]
    pub dispute: Account<'info, Dispute>,

    pub system_program: Program<'info, System>,
}
