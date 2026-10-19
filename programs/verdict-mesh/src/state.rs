use anchor_lang::prelude::*;

use crate::errors::VerdictMeshError;

/// Denominator for slashing shares: 10 000 bps = 100% of the stake.
pub const BPS_DENOMINATOR: u16 = 10_000;

/// Upper bound on panel size. It bounds two things at once: the `panel` vector in `Dispute`
/// itself and the length of the loops that iterate over it in selection, tallying and stake
/// settlement. An unbounded panel is an instruction that will one day run out of compute
/// budget, and that will surface on a live dispute.
pub const MAX_PANEL_SIZE: u8 = 32;

/// Upper bound on a single window's duration — 30 days. Windows add up into deadlines
/// (`opened_at + commit + reveal + appeal`), and without an upper bound the sum overflows `i64`
/// and the dispute gets a deadline in the past.
pub const MAX_WINDOW: i64 = 30 * 24 * 60 * 60;

/// Copied into every dispute when it is opened. An integrator changing its policy does not
/// affect disputes that are already open — FR-003.
#[derive(
    AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, Default, PartialEq, Eq, Debug,
)]
pub struct Policy {
    pub panel_size: u8,
    pub extended_panel_size: u8,
    pub quorum: u8,
    pub extended_quorum: u8,
    pub juror_stake: u64,
    pub slash_bps_wrong: u16,
    pub slash_bps_no_reveal: u16,
    pub commit_window: i64,
    pub reveal_window: i64,
    pub appeal_window: i64,
    pub optimistic_window: i64,
    pub deposit: u64,
    pub optimistic_threshold: u64,
}

impl Policy {
    /// Validation lives next to the type rather than inside the registration instruction: the
    /// policy reaches funds through `Dispute`, and any future path that creates a `Policy` must
    /// pass through here. A check added to one instruction is simply not called by the next
    /// one.
    ///
    /// Every violation returns the same `InvalidPolicy`. A separate code per rule would add
    /// nothing: the client cannot "partially register" anyway, and Anchor writes the line with
    /// the failing rule to the log.
    pub fn validate(&self) -> Result<()> {
        // Panel and quorum.
        require!(self.panel_size > 0, VerdictMeshError::InvalidPolicy);
        require!(
            self.extended_panel_size > self.panel_size,
            VerdictMeshError::InvalidPolicy
        );
        require!(
            self.extended_panel_size <= MAX_PANEL_SIZE,
            VerdictMeshError::InvalidPolicy
        );

        // Quorum is a majority of the panel, not "however many managed to reveal". Otherwise
        // the verdict is decided by a minority that simply happened to be faster.
        require!(
            is_majority(self.quorum, self.panel_size),
            VerdictMeshError::InvalidPolicy
        );
        require!(
            is_majority(self.extended_quorum, self.extended_panel_size),
            VerdictMeshError::InvalidPolicy
        );

        // Escalation must not lower the bar: the extended review cannot close with fewer votes
        // than the initial one required (FR-027).
        require!(
            self.extended_quorum >= self.quorum,
            VerdictMeshError::InvalidPolicy
        );

        // Juror economics. A zero stake or zero slashing leaves the mechanism in place but makes
        // a wrong vote free.
        require!(self.juror_stake > 0, VerdictMeshError::InvalidPolicy);
        require!(self.slash_bps_wrong > 0, VerdictMeshError::InvalidPolicy);
        require!(
            self.slash_bps_no_reveal <= BPS_DENOMINATOR,
            VerdictMeshError::InvalidPolicy
        );
        // FR-008b: silence must cost more than a losing vote, otherwise not revealing is the
        // cheapest way not to lose.
        require!(
            self.slash_bps_no_reveal > self.slash_bps_wrong,
            VerdictMeshError::InvalidPolicy
        );

        // FR-026b: the review fee is split between the jurors and the protocol. Zero means the
        // panel works for free.
        require!(self.deposit > 0, VerdictMeshError::InvalidPolicy);

        for window in [
            self.commit_window,
            self.reveal_window,
            self.appeal_window,
            self.optimistic_window,
        ] {
            require!(
                (1..=MAX_WINDOW).contains(&window),
                VerdictMeshError::InvalidPolicy
            );
        }

        // `optimistic_threshold` is deliberately unchecked: zero is a valid policy that simply
        // disables the optimistic track, since no amount can fall below zero
        // (SPEC.md → US5).

        Ok(())
    }
}

/// Strict majority: `2 * quorum > panel`, computed in `u16` so the product of two `u8`s does not
/// overflow on a large panel.
fn is_majority(quorum: u8, panel: u8) -> bool {
    quorum > 0 && quorum <= panel && u16::from(quorum) * 2 > u16::from(panel)
}

#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisputeState {
    OptimisticPending,
    Committing,
    Revealing,
    Tallied,
    Appealed,
    Finalized,
}

/// StatusQuo means "as if there had been no dispute" (FR-027a). The escrow must be able to
/// distribute funds under this outcome, otherwise auto-escalation leads nowhere.
#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Claimant,
    Respondent,
    StatusQuo,
}

/// A juror's ballot — **two options, not three**.
///
/// `StatusQuo` is how a failed escalation ends (`FR-027a`), not an answer that can be submitted.
/// The difference is not a formality: on a panel of three with a quorum of two, three different
/// answers never form a majority, so a third ballot option would let a single juror send any
/// dispute to re-review — at someone else's expense, since the protocol pays for escalation
/// (`FR-027`).
///
/// A separate type rather than a runtime check, because the submission cannot be checked:
/// `commit_vote` sees only a hash. A juror who committed a `StatusQuo` hash would learn about the
/// restriction only at reveal — when nothing can be changed any more, and silence costs them a
/// larger share of their stake (`FR-008b`).
/// Such a commitment simply cannot be expressed in the type,
/// so the trap does not exist.
#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ballot {
    Claimant,
    Respondent,
}

impl Ballot {
    /// The ballot as an outcome. The tally (T019) compares ballots against the verdict, and the
    /// conversion must live in one place.
    pub fn verdict(self) -> Verdict {
        match self {
            Ballot::Claimant => Verdict::Claimant,
            Ballot::Respondent => Verdict::Respondent,
        }
    }
}

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub settlement_mint: Pubkey,
    /// The only privileged key in the system. It can only record a report hash — FR-017a. There
    /// are no instructions through which it can change a verdict or move funds.
    pub reporter: Pubkey,
    /// Owner of the token account that receives the protocol's share of the review fee
    /// (`FR-026b`). This is **a destination address, not a role**: no instruction asks it for a
    /// signature or grants it anything beyond receiving a transfer. That is why it does not
    /// conflict with `FR-014` — a key that cannot sign anything has no power over a verdict.
    ///
    /// Written together with the rest of `Config` and likewise never updated: a key able to
    /// change the fee recipient could also change it in the middle
    /// of a review.
    pub treasury: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Integrator {
    pub authority: Pubkey,
    pub escrow_program: Pubkey,
    pub policy: Policy,
    pub dispute_count: u64,
    pub bump: u8,
}

/// One registry per protocol, not per integrator: a juror stakes once and becomes eligible for
/// the panels of every integrator. That is why there is no stake threshold here, and there
/// cannot be one — `juror_stake` lives in `Policy`, so each integrator has its own. A juror's
/// eligibility for a specific dispute is checked by panel selection (`FR-006`, T016), not by
/// joining the registry.
#[account]
#[derive(InitSpace)]
pub struct JurorRegistry {
    pub juror_count: u32,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Juror {
    pub wallet: Pubkey,
    /// The amount deposited, not a "sufficient" amount: sufficiency is determined by the policy
    /// of whichever dispute's panel the juror lands on.
    pub stake: u64,
    /// How many non-finalized disputes hold this juror. While it is non-zero, the stake cannot be
    /// withdrawn (`FR-007`, T015).
    pub active_disputes: u16,
    /// Position in the registry. Together with `JurorIndex` it makes the registry enumerable for
    /// selection: `index` maps a juror to its slot, `JurorIndex` maps back.
    pub index: u32,
    pub bump: u8,
}

/// Makes the registry enumerable by index — without it, deterministic selection (FR-006) cannot
/// pick N out of M without reading the whole registry off-chain.
/// A juror leaving is a swap-remove: the last index moves into the freed one.
#[account]
#[derive(InitSpace)]
pub struct JurorIndex {
    pub wallet: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Dispute {
    pub integrator: Pubkey,
    pub dispute_id: u64,
    /// A snapshot, not a reference — FR-003.
    pub policy: Policy,
    /// PDA of the escrow that opened the dispute. The escrow checks this field against its own
    /// address before executing the verdict — otherwise someone else's dispute could dispose of
    /// its funds.
    pub escrow_ref: Pubkey,
    pub claimant: Pubkey,
    pub respondent: Pubkey,
    pub amount: u64,
    pub state: DisputeState,
    /// `max_len(0)` is neither a mistake nor "a panel of zero jurors". The vector grows up to
    /// `policy.extended_panel_size`, which is known only at opening time, so `InitSpace` counts
    /// just the 4-byte length prefix here and `Dispute::space` adds the rest. This way the macro
    /// still computes the fixed part, and a new field cannot silently drop out of the size
    /// calculation.
    #[max_len(0)]
    pub panel: Vec<Pubkey>,
    pub report_hash: [u8; 32],
    pub claimant_claim_hash: [u8; 32],
    pub respondent_claim_hash: [u8; 32],
    pub opened_at: i64,
    /// The slot whose hash provides entropy for panel selection — `FR-006`. Written at opening
    /// and never changed afterwards: if selection took its entropy from the moment of **its own**
    /// execution, it could be gamed by retrying slot after slot until a favorable panel came up.
    ///
    /// This is the slot **before** the one in which the dispute was opened: the current slot's
    /// hash does not exist yet, so selection in the same transaction would
    /// not find it.
    pub entropy_slot: u64,
    pub commit_deadline: i64,
    pub reveal_deadline: i64,
    pub appeal_deadline: i64,
    /// Revealed votes, one counter per ballot option. There is no separate `revealed_count` for
    /// the same reason `VoteCommit` has no `revealed`: it would always equal the sum of these
    /// two, and two sources of the same value sooner or later diverge. To get how many revealed,
    /// take the sum.
    pub votes_claimant: u8,
    pub votes_respondent: u8,
    /// Auto-escalation applies only once — FR-027a.
    pub escalated: bool,
    pub verdict: Option<Verdict>,
    pub bump: u8,
}

impl Dispute {
    /// Review round: 0 is the initial panel, 1 the extended one. Derived from `escalated` rather
    /// than stored separately, because there is exactly one escalation (`FR-027a`) — a separate
    /// field would be a second source of the same fact.
    ///
    /// The number is needed because a vote commitment belongs to its round: one submitted in the
    /// first round and not revealed before the tally is already late, and revealing it in the
    /// second would let the juror choose whether to disclose their vote **after seeing the first
    /// tally**.
    pub fn round(&self) -> u8 {
        u8::from(self.escalated)
    }

    /// Space for the dispute account: the fixed part from `InitSpace` plus the panel itself. The
    /// size is taken from the **extended** panel, not the initial one: auto-escalation
    /// (`FR-027`) extends the same vector, and there is no way to grow the account
    /// after creation.
    pub fn space(extended_panel_size: u8) -> usize {
        Self::DISCRIMINATOR.len()
            + Self::INIT_SPACE
            + std::mem::size_of::<Pubkey>() * extended_panel_size as usize
    }
}

/// A juror's vote commitment — `FR-008`. Created during the commit window, and until reveal it
/// contains nothing from which the vote itself can be derived: `commitment` is a hash
/// (`crate::vote`), `choice` is `None`.
///
/// **There is deliberately no `revealed` flag here**, even though the data model in `PLAN.md`
/// lists one. It would always duplicate `choice.is_some()`, and two fields that must agree
/// sooner or later diverge: stake settlement (T020) distinguishes an unrevealed vote from a
/// revealed one and must do so from a single source.
/// That source is `choice`.
#[account]
#[derive(InitSpace)]
pub struct VoteCommit {
    pub dispute: Pubkey,
    pub juror: Pubkey,
    pub commitment: [u8; 32],
    /// `None` until the vote is revealed (T018). An empty value is exactly what `FR-008b` means
    /// by "the juror committed but did not reveal".
    pub choice: Option<Ballot>,
    /// The review round in which the commitment was submitted — `Dispute::round`. It can only be
    /// revealed in the same round: otherwise a juror who stayed silent in the first round would
    /// decide in the second whether to disclose their vote, already knowing how the first tally
    /// ended. `FR-027b` slashes them precisely for that silence, and a late reveal must not undo
    /// it.
    pub round: u8,
    pub bump: u8,
}
