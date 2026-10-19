use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
    TransferChecked,
};

use crate::{
    errors::VerdictMeshError,
    events::{DisputeFeeSettled, DisputeFinalized, JurorRewarded, JurorSlashed},
    seeds,
    state::{Config, Dispute, DisputeState, Juror, Verdict, VoteCommit},
    vault::{self, FeeSplit},
};

/// Settles the panel's stakes — `FR-011`, `FR-008b`, `FR-027b`.
///
/// **Slashing moves records, not tokens.** The stake vault is shared by the whole protocol, so
/// nothing needs to — or may — be transferred out of it: it is enough to decrease the slashed
/// juror's `Juror.stake` and increase the `Juror.stake` of those who receive it. The sum of the
/// records does not grow, and the vault stays no smaller than that sum — exactly the invariant
/// `unstake` (T015) relies on when it returns precisely `Juror.stake`. Debiting the vault without
/// decreasing the record would let the slashed juror exit with the full amount, i.e. with someone
/// else's stake.
///
/// **Silence is never free, and this decision goes beyond the letter of `FR-008b`.** The
/// requirement speaks of a juror who committed a vote and did not reveal it. But a juror who did
/// not commit **at all** dodged in the same way — and more cheaply, since a literal reading would
/// leave them unpunished. Then the cheapest way to avoid voting against one's interest would be
/// not to vote at all, and `FR-008b` would protect nothing. Hence both cases take the same share,
/// `slash_bps_no_reveal`.
///
/// **Under a status-quo verdict nobody is slashed for a "wrong" vote.** Nobody voted for it, nor
/// could they: it is the result of a failed escalation (`FR-027a`), not a ballot option. Comparing
/// anyone's vote against it makes no sense, and neither does punishing jurors because the panel
/// did not agree. Silence is still punished here: it is what caused the escalation
/// (`FR-027b`).
///
/// **The review fee is paid out here too, and keeping it in one instruction is deliberate.** The
/// deposit (`FR-026`) is split among the same jurors that slashing has just iterated over — the
/// set of "who was right" is computed once and cannot diverge between two cranks. The second
/// reason matters more than the first: a separate instruction would have its own window in which
/// the dispute is already finalized but the fee is not, and the order of two permissionless calls
/// would decide whether a juror gets paid. The split math lives in `vault.rs`.
///
/// **The fee reaches the juror through the same record as the slashed stake.** The only
/// difference is that it requires an actual token transfer: slashed stake already sits in the
/// stake vault, while the deposit sits in the dispute vault. So the jurors' share is transferred
/// `dispute_vault → stake_vault` for exactly the amount by which the sum of records grows, and
/// the invariant "vault ≥ sum of records" holds.
///
/// **The instruction is final and permissionless.** Anyone can sign it; the only thing the
/// signature earns is the rent of the closed dispute vault. This is not a privilege but a reason
/// to call the crank at all; without it a dispute would stay unfinalized until someone happened
/// to care.
/// It also decrements `active_disputes` — by one, not to zero: a juror sits on several panels
/// at once.
impl<'info> SettleStakes<'info> {
    pub fn handle(ctx: Context<'_, '_, 'info, 'info, SettleStakes<'info>>) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let dispute_key = ctx.accounts.dispute.key();
        // A read, not `&mut`: the only change to the dispute itself is the transition to
        // `Finalized` at the end, and an outstanding mutable borrow would prevent passing the
        // remaining accounts into the fee transfer.
        let dispute = &ctx.accounts.dispute;

        require!(
            dispute.state == DisputeState::Tallied,
            VerdictMeshError::WrongState
        );
        // Settling before the appeal window ends would mean settling a verdict that can still be
        // overturned.
        require!(now >= dispute.appeal_deadline, VerdictMeshError::WindowOpen);

        let verdict = dispute.verdict.ok_or(VerdictMeshError::WrongState)?;
        let policy = dispute.policy;

        // The whole panel is passed, as (`Juror`, `VoteCommit`) pairs in panel order. Showing
        // the settlement only convenient jurors would leave the rest unslashed and locked in the
        // registry.
        require_eq!(
            ctx.remaining_accounts.len(),
            2 * dispute.panel.len(),
            VerdictMeshError::InvalidSettlementAccounts
        );

        let mut jurors = Vec::with_capacity(dispute.panel.len());
        let mut pot: u64 = 0;

        for (seat, wallet) in dispute.panel.iter().enumerate() {
            let pair = 2 * seat;
            let mut juror = panel_member(&ctx.remaining_accounts[pair], wallet)?;
            let choice = revealed_choice(&ctx.remaining_accounts[pair + 1], &dispute_key, wallet)?;

            let bps = match choice {
                // Revealed and matched — or revealed where there is nothing to match (status
                // quo). Both did the work.
                Some(choice) if verdict == Verdict::StatusQuo || choice == verdict => 0,
                Some(_) => policy.slash_bps_wrong,
                None => policy.slash_bps_no_reveal,
            };

            let slashed = vault::share_of(juror.stake, bps)?;
            juror.stake = juror
                .stake
                .checked_sub(slashed)
                .ok_or(VerdictMeshError::Overflow)?;
            pot = pot.checked_add(slashed).ok_or(VerdictMeshError::Overflow)?;

            // This is why selection incremented the counter in the first place (T016). By one,
            // not to zero: zeroing it would release the juror from the registry while another,
            // still unresolved dispute holds them (`FR-007`).
            juror.active_disputes = juror.active_disputes.saturating_sub(1);

            if slashed > 0 {
                emit!(JurorSlashed {
                    dispute: dispute_key,
                    juror: *wallet,
                    amount: slashed,
                    no_reveal: choice.is_none(),
                });
            }

            jurors.push((seat, juror, choice.is_some() && bps == 0));
        }

        // Who receives the slashed stake. Under a real verdict — those whose vote matched; under
        // status quo — everyone who revealed: there is nothing to match, but they did the work.
        // If nobody is left, the collected amount simply stays in the vault — nobody can withdraw
        // it by any means, because the program has no instruction that gives anyone power over
        // funds (`FR-014`).
        let winners = jurors.iter().filter(|(_, _, won)| *won).count();

        // Review fee — `FR-026b`. What is split is what actually sits in the dispute vault,
        // while the jurors' share is computed from the announced price: a shortfall, should one
        // ever occur, must fall on the protocol, not on the juror.
        let fee = vault::split_fee(
            ctx.accounts.dispute_vault.amount,
            policy.deposit,
            winners > 0,
        )?;
        Self::pay_fee(&ctx, &fee)?;

        // The fee is added to the same pot as the slashed stake: both amounts go to the same
        // people for the same review, and computing them separately would mean dividing with a
        // remainder twice — i.e. losing dust
        // where it belongs to nobody.
        pot = pot
            .checked_add(fee.jurors)
            .ok_or(VerdictMeshError::Overflow)?;

        if winners > 0 {
            let winners = u64::try_from(winners).map_err(|_| VerdictMeshError::Overflow)?;
            let each = pot / winners;
            // The division remainder goes to the first winner by panel seat rather than being left
            // as dust: over hundreds of reviews the dust becomes an amount that belongs to nobody
            // and does not reconcile with the vault.
            let mut extra = pot % winners;

            for (_, juror, _) in jurors.iter_mut().filter(|(_, _, won)| *won) {
                let reward = each.checked_add(extra).ok_or(VerdictMeshError::Overflow)?;
                extra = 0;
                juror.stake = juror
                    .stake
                    .checked_add(reward)
                    .ok_or(VerdictMeshError::Overflow)?;

                if reward > 0 {
                    emit!(JurorRewarded {
                        dispute: dispute_key,
                        juror: juror.wallet,
                        amount: reward,
                    });
                }
            }
        }

        for (seat, juror, _) in &jurors {
            store(&ctx.remaining_accounts[2 * seat], juror)?;
        }

        ctx.accounts.dispute.state = DisputeState::Finalized;

        emit!(DisputeFeeSettled {
            dispute: dispute_key,
            jurors: fee.jurors,
            protocol: fee.protocol,
        });

        emit!(DisputeFinalized {
            dispute: dispute_key,
            verdict,
            finalized_at: now,
        });

        Ok(())
    }

    /// Pays the review fee out of the dispute vault and closes it.
    ///
    /// The vault is **always** closed, even when there was nothing to split: an empty token
    /// account per dispute is rent locked forever, and it accumulates at exactly the rate at
    /// which the protocol operates.
    fn pay_fee(
        ctx: &Context<'_, '_, 'info, 'info, SettleStakes<'info>>,
        fee: &FeeSplit,
    ) -> Result<()> {
        let config_bump = ctx.accounts.config.bump;
        let signer_seeds: &[&[&[u8]]] = &[&[seeds::CONFIG, &[config_bump]]];
        let decimals = ctx.accounts.settlement_mint.decimals;

        let pay = |to: AccountInfo<'info>, amount: u64| -> Result<()> {
            if amount == 0 {
                return Ok(());
            }

            transfer_checked(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    TransferChecked {
                        from: ctx.accounts.dispute_vault.to_account_info(),
                        mint: ctx.accounts.settlement_mint.to_account_info(),
                        to,
                        authority: ctx.accounts.config.to_account_info(),
                    },
                    signer_seeds,
                ),
                amount,
                decimals,
            )
        };

        // The jurors' share moves into the stake vault because that is where their balances
        // live: `Juror.stake` is a record against that vault, and increasing it without adding
        // tokens would promise more than exists.
        pay(ctx.accounts.stake_vault.to_account_info(), fee.jurors)?;
        pay(ctx.accounts.treasury_tokens.to_account_info(), fee.protocol)?;

        close_account(CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            CloseAccount {
                account: ctx.accounts.dispute_vault.to_account_info(),
                destination: ctx.accounts.crank.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            signer_seeds,
        ))
    }
}

/// The `Juror` of the wallet that actually occupies this panel seat. The check is by address:
/// a substituted record would absorb the slashing instead of the juror
/// it is meant for.
fn panel_member<'info>(info: &'info AccountInfo<'info>, wallet: &Pubkey) -> Result<Juror> {
    let juror: Account<Juror> = Account::try_from(info)?;
    require!(
        info.is_writable,
        VerdictMeshError::InvalidSettlementAccounts
    );
    expect_address(info, &[seeds::JUROR, wallet.as_ref(), &[juror.bump]])?;

    Ok(juror.into_inner())
}

/// The juror's vote, if they revealed it.
///
/// An empty account is not an error: it is a juror who never committed. That is exactly why the
/// address is re-derived here rather than checked against a stored bump — a nonexistent account
/// has none, yet proving that the empty account passed in **is** that very vote is still
/// required.
fn revealed_choice<'info>(
    info: &'info AccountInfo<'info>,
    dispute: &Pubkey,
    wallet: &Pubkey,
) -> Result<Option<Verdict>> {
    let (expected, _) = Pubkey::find_program_address(
        &[seeds::VOTE, dispute.as_ref(), wallet.as_ref()],
        &crate::ID,
    );
    require_keys_eq!(
        *info.key,
        expected,
        VerdictMeshError::InvalidSettlementAccounts
    );

    if info.data_is_empty() {
        return Ok(None);
    }

    let vote: Account<VoteCommit> = Account::try_from(info)?;

    Ok(vote.choice.map(|choice| choice.verdict()))
}

/// Writes the juror's modified state back to the account: Anchor does this automatically only
/// for context fields; `remaining_accounts` are the instruction's own responsibility.
fn store(info: &AccountInfo, juror: &Juror) -> Result<()> {
    let mut data = info.try_borrow_mut_data()?;
    juror.try_serialize(&mut &mut data[..])?;

    Ok(())
}

/// Checks an address against a PDA using the **stored** bump — cheaper than a search and just as
/// unambiguous.
fn expect_address(info: &AccountInfo, seeds: &[&[u8]]) -> Result<()> {
    let expected = Pubkey::create_program_address(seeds, &crate::ID)
        .map_err(|_| error!(VerdictMeshError::InvalidSettlementAccounts))?;
    require_keys_eq!(
        *info.key,
        expected,
        VerdictMeshError::InvalidSettlementAccounts
    );

    Ok(())
}

#[derive(Accounts)]
pub struct SettleStakes<'info> {
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

    /// Anyone. A signature is required only because the rent of the closed dispute vault has to
    /// go to someone, and the fairest recipient is whoever took it upon themselves to call
    /// the crank.
    #[account(mut)]
    pub crank: Signer<'info>,

    #[account(seeds = [seeds::CONFIG], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(address = config.settlement_mint)]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    /// This dispute's vault. It is closed here — hence `mut`.
    #[account(
        mut,
        seeds = [seeds::DISPUTE_VAULT, dispute.key().as_ref()],
        bump,
        token::mint = settlement_mint,
        token::authority = config,
    )]
    pub dispute_vault: InterfaceAccount<'info, TokenAccount>,

    /// The jurors' share moves here: `Juror.stake` is a record against this vault, and it may
    /// only grow together with it.
    #[account(
        mut,
        seeds = [seeds::STAKE_VAULT],
        bump,
        token::mint = settlement_mint,
        token::authority = config,
    )]
    pub stake_vault: InterfaceAccount<'info, TokenAccount>,

    /// The protocol treasury's token account — `FR-026b`. Checked by the owner from `Config`
    /// rather than by the account's own address: the ATA address is derived from the owner and
    /// the mint, both already bound, and storing it separately would be a second source of the
    /// same fact.
    #[account(
        mut,
        token::mint = settlement_mint,
        token::authority = config.treasury,
    )]
    pub treasury_tokens: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    // `remaining_accounts`: (`Juror`, `VoteCommit`) pairs, one per panel seat, in panel order.
    // The vote account may be empty — that is a juror who
    // never committed.
}
