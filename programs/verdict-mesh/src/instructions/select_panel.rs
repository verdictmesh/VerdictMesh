use anchor_lang::prelude::*;

use crate::{
    errors::VerdictMeshError,
    events::PanelSelected,
    panel::{self, SLOT_HASHES_ID},
    seeds,
    state::{Dispute, DisputeState, Juror, JurorIndex, JurorRegistry},
};

/// Juror panel selection — `FR-006`.
///
/// **Why this is a separate instruction rather than part of `open_dispute`.** A dispute is
/// opened by **the integrator's escrow program**, under its own signature. If selection
/// lived inside it, every integrator would have to pass the entire juror registry through
/// its CPI — that is, know exactly the part of VerdictMesh's internals that a
/// "protocol-agnostic layer" is meant to hide. `SC-004` measures integration at forty
/// lines, and passing the registry through would not fit in them on its own.
///
/// `FR-006` requires selection **within the same transaction**, and that still holds: the
/// SDK puts `open_dispute` and `select_panel` into one transaction (T035). The program,
/// however, guarantees something stronger than "in one transaction": entropy is tied to
/// `entropy_slot`, recorded at opening, so the panel is fixed the moment the dispute opens
/// and **does not depend on when, or by whom, selection is invoked**. It cannot be
/// re-rolled by retrying slot after slot.
///
/// **The instruction is permissionless.** No signature is required: the result
/// is deterministic and already fixed, so restricting it to any single party
/// would only give that party the ability to skip selection altogether.
///
/// **It also tops up the panel after an escalation** (`FR-027`, T019). There is no second
/// selection path: it would have to repeat the registry enumeration, the address checks, and
/// the `active_disputes` bookkeeping — and drift from this one at the first edit. The only
/// differences are how many seats need filling and who is no longer a candidate.
impl<'info> SelectPanel<'info> {
    /// The explicit lifetimes in the signature are not cosmetic:
    /// `remaining_accounts` must live as long as the accounts themselves,
    /// otherwise `Account::try_from` cannot be applied to them.
    pub fn handle(ctx: Context<'_, '_, 'info, 'info, SelectPanel<'info>>) -> Result<()> {
        let dispute_key = ctx.accounts.dispute.key();
        let dispute = &mut ctx.accounts.dispute;

        require!(
            dispute.state == DisputeState::Committing,
            VerdictMeshError::WrongState
        );
        // How many jurors the panel must have **now**. After an escalation
        // (`FR-027`, T019) this is the extended size, and the same instruction
        // fills the missing seats — without a second selection path that would
        // have to be kept consistent with this one.
        let target = usize::from(if dispute.escalated {
            dispute.policy.extended_panel_size
        } else {
            dispute.policy.panel_size
        });
        // A panel already filled to its target size is not selected again: a
        // repeat selection would increment `active_disputes` a second time for
        // the same jurors, i.e. an exit lock that is never released.
        require!(
            dispute.panel.len() < target,
            VerdictMeshError::PanelAlreadySelected
        );
        let needed = u8::try_from(target - dispute.panel.len())
            .map_err(|_| VerdictMeshError::InvalidPolicy)?;
        // A panel selected after the commit window closes has no time to vote.
        require!(
            Clock::get()?.unix_timestamp < dispute.commit_deadline,
            VerdictMeshError::WindowClosed
        );

        let policy = dispute.policy;
        let registry_size = ctx.accounts.registry.juror_count;

        // The registry is passed in full and in slot order. Otherwise there is
        // nothing to check eligibility against: selection must see everyone it
        // could pick, not just those the caller chose to show.
        let mut candidates = Vec::with_capacity(registry_size as usize);
        require_eq!(
            ctx.remaining_accounts.len(),
            2 * registry_size as usize,
            VerdictMeshError::InvalidPanelAccounts
        );

        for slot in 0..registry_size {
            let pair = 2 * slot as usize;
            let entry = enumerated_slot(&ctx.remaining_accounts[pair], slot)?;
            let juror = juror_of(&ctx.remaining_accounts[pair + 1], &entry.wallet)?;

            // `FR-011a`: eligibility is measured against **this** dispute's policy. The
            // registry is shared by all integrators, so there is no single threshold — which
            // is exactly why joining the registry does not check it (T014).
            //
            // Anyone already on the panel is not a candidate. This is what makes the
            // post-escalation top-up an extension rather than a re-roll: sitting jurors keep
            // the votes they have already cast, and the draw covers only the added seats.
            // "Continuing the same draw further" is not an option here — the registry may
            // have changed between the two selections, and continuing would put different
            // jurors on seats that are already taken.
            if juror.stake >= policy.juror_stake && !dispute.panel.contains(&entry.wallet) {
                candidates.push((slot, juror));
            }
        }

        require!(
            candidates.len() >= usize::from(needed),
            VerdictMeshError::RegistryTooSmall
        );

        let entropy = panel::entropy_of(
            &ctx.accounts.slot_hashes,
            dispute.entropy_slot,
            &dispute_key,
        )?;
        let drawn = panel::draw(
            &entropy,
            u32::try_from(candidates.len()).map_err(|_| VerdictMeshError::Overflow)?,
            needed,
        )?;

        for position in drawn {
            let (slot, juror) = &mut candidates[position as usize];
            let info = &ctx.remaining_accounts[2 * *slot as usize + 1];

            // The counter that later keeps a juror from leaving the registry
            // until the dispute is finalized (`FR-007`, T015). Without this
            // increment the exit lock has nothing to lock.
            juror.active_disputes = juror
                .active_disputes
                .checked_add(1)
                .ok_or(VerdictMeshError::Overflow)?;

            dispute.panel.push(juror.wallet);
            store(info, juror)?;
        }

        emit!(PanelSelected {
            dispute: dispute_key,
            panel: dispute.panel.clone(),
            entropy_slot: dispute.entropy_slot,
        });

        Ok(())
    }
}

/// The `JurorIndex` that actually sits in slot `slot`.
///
/// This is the very reason `JurorIndex` exists. A slot's address is a function of its number,
/// so checking the address proves two things at once: that the passed account belongs to
/// exactly this slot, and that the registry was enumerated with no gaps or repeats. Without
/// it, the caller could show the selection only the jurors that suit them.
fn enumerated_slot<'info>(info: &'info AccountInfo<'info>, slot: u32) -> Result<JurorIndex> {
    let entry: Account<JurorIndex> = Account::try_from(info)?;
    expect_address(
        info,
        &[seeds::JUROR_INDEX, &slot.to_le_bytes(), &[entry.bump]],
    )?;

    Ok(entry.into_inner())
}

/// The `Juror` of the wallet recorded in the registry slot. The address is
/// checked against the wallet from the **slot**, not from the record itself:
/// otherwise a substituted `Juror` would vouch for itself.
///
/// The account must be writable — selection will increment its `active_disputes`, and finding
/// out there is nowhere to write is better here than halfway through recording the panel.
fn juror_of<'info>(info: &'info AccountInfo<'info>, wallet: &Pubkey) -> Result<Juror> {
    let juror: Account<Juror> = Account::try_from(info)?;
    require!(info.is_writable, VerdictMeshError::InvalidPanelAccounts);
    expect_address(info, &[seeds::JUROR, wallet.as_ref(), &[juror.bump]])?;

    Ok(juror.into_inner())
}

/// Checks the address against the PDA derived with the **stored** bump.
/// Cheaper than `find_program_address` and just as unambiguous: a wrong bump
/// yields a different address, which does not match the one passed in.
fn expect_address(info: &AccountInfo, seeds: &[&[u8]]) -> Result<()> {
    let expected = Pubkey::create_program_address(seeds, &crate::ID)
        .map_err(|_| error!(VerdictMeshError::InvalidPanelAccounts))?;
    require_keys_eq!(*info.key, expected, VerdictMeshError::InvalidPanelAccounts);

    Ok(())
}

/// Writes the juror's updated state back to the account. Anchor does this on its own only for
/// context fields — `remaining_accounts` are the instruction's responsibility.
fn store(info: &AccountInfo, juror: &Juror) -> Result<()> {
    let mut data = info.try_borrow_mut_data()?;
    juror.try_serialize(&mut &mut data[..])?;

    Ok(())
}

#[derive(Accounts)]
pub struct SelectPanel<'info> {
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

    #[account(seeds = [seeds::REGISTRY], bump = registry.bump)]
    pub registry: Account<'info, JurorRegistry>,

    /// Entropy source — `FR-006`. Read as raw bytes: the sysvar's 512 entries
    /// are not deserialized in full (see `panel::entropy_of`).
    ///
    /// CHECK: verified by the sysvar address; the contents are parsed by `panel`.
    #[account(address = SLOT_HASHES_ID)]
    pub slot_hashes: UncheckedAccount<'info>,
    // `remaining_accounts`: (`JurorIndex`, `Juror`) pairs, one per registry
    // slot, in slot order. They cannot be declared as typed fields — their
    // number is known only at call time.
}
