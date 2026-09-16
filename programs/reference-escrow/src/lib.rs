use anchor_lang::prelude::*;

pub mod claims;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod seeds;
pub mod state;

pub use errors::EscrowError;
pub use instructions::*;
pub use state::*;

declare_id!("4iYF4WRdtuSmjTXH5fSa2ow5WrdeonEoeoY3epypfTHo");

/// Milestone-ескроу, який купує арбітраж у VerdictMesh замість того, щоб писати
/// власний. Це і демо-інтеграція для US1/US2/US4/US5, і зразок, за яким міряється
/// SC-004.
///
/// Виконання вердикту працює за pull-моделлю: `settle` читає акаунт `Dispute`,
/// перевіряє власника, стан і те, що спір належить саме цьому ескроу, після чого
/// розподіляє кошти сам. VerdictMesh при цьому не має жодного повноваження над
/// цим ескроу.
#[program]
pub mod reference_escrow {
    use super::*;

    /// Замикає всю суму угоди й розписує її по віхах, а разом з нею — заставу за
    /// розгляд з обох сторін (`FR-026e`). Підписують обидві сторони: разом з
    /// угодою вони приймають політику розгляду. Див. `instructions::escrow`.
    pub fn create_escrow<'info>(
        ctx: Context<'_, '_, '_, 'info, CreateEscrow<'info>>,
        deal_id: u64,
        milestones: Vec<u64>,
    ) -> Result<()> {
        CreateEscrow::handle(ctx, deal_id, milestones)
    }

    /// Закриває віху без спору: замовник віддає свої гроші добровільно, і
    /// більше ніхто цього зробити не може. Застава віхи повертається обом
    /// сторонам разом із нею — див. `instructions::escrow`.
    pub fn release_milestone<'info>(
        ctx: Context<'_, '_, '_, 'info, ReleaseMilestone<'info>>,
        milestone: u8,
    ) -> Result<()> {
        ReleaseMilestone::handle(ctx, milestone)
    }

    /// Відкриває спір над віхою одним CPI у VerdictMesh, підписуючи його
    /// власним PDA. Уся інтеграція — тут; вердикт звідси витягнуть, а не
    /// проштовхнуть сюди. Див. `instructions::escrow`.
    pub fn dispute_milestone<'info>(
        ctx: Context<'_, '_, '_, 'info, DisputeMilestone<'info>>,
        milestone: u8,
    ) -> Result<()> {
        DisputeMilestone::handle(ctx, milestone)
    }

    /// Виконує вердикт над віхою: читає `Dispute`, робить чотири перевірки і
    /// розподіляє кошти сам. Нічия інструкція без жодного підпису — див.
    /// `instructions::settle`.
    pub fn settle_milestone<'info>(
        ctx: Context<'_, '_, '_, 'info, SettleMilestone<'info>>,
        milestone: u8,
    ) -> Result<()> {
        SettleMilestone::handle(ctx, milestone)
    }
}
