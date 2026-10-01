import type { DisputeView } from '@verdictmesh/shared'
import { formatAgo, formatDuration, stageOf, VERDICT_LABEL } from './dispute'

export type StepState = 'done' | 'now' | 'next'

export interface Step {
  key: string
  label: string
  detail: string
  when: string
  state: StepState
  /** The settlement transaction, once known. */
  signature?: string
}

const left = (deadline: number, now: number) => `${formatDuration((deadline - now) * 1000)} left`

/**
 * The hearing as a party follows it (`FR-020`): what has happened, what is
 * happening, how long it has left. Built from the deadlines and the verdict
 * only — the same inputs `stageOf` reads, so the list and the stage chip can
 * never disagree.
 */
export function buildTimeline(dispute: DisputeView, now: number): Step[] {
  const stage = stageOf(dispute, now)
  const decided = dispute.verdict !== null

  const window = (start: number | null, end: number): StepState => {
    if (decided || now >= end) return 'done'
    if (start === null || now >= start) return 'now'
    return 'next'
  }

  const commit = window(null, dispute.commitDeadline)
  const reveal = window(dispute.commitDeadline, dispute.revealDeadline)

  const steps: Step[] = [
    {
      key: 'opened',
      label: dispute.escalated ? 'Hearing opened, then escalated' : 'Hearing opened',
      detail: dispute.escalated
        ? 'The first panel did not reach a quorum; a wider panel was drawn and the windows restarted.'
        : 'The escrow marked the milestone disputed and locked the funds where they were.',
      when: formatAgo(dispute.openedAt, now),
      state: 'done',
    },
    {
      key: 'commit',
      label: 'Commit window',
      detail: 'Each panel member seals a choice. Sealed choices are unreadable to everyone.',
      when: commit === 'now' ? left(dispute.commitDeadline, now) : 'closed',
      state: commit,
    },
    {
      key: 'reveal',
      label: 'Reveal window',
      detail: 'Each panel member opens the sealed choice. A member who stays silent loses stake.',
      when:
        reveal === 'now'
          ? left(dispute.revealDeadline, now)
          : reveal === 'next'
            ? 'starts when the commit window closes'
            : 'closed',
      state: reveal,
    },
    {
      key: 'verdict',
      label: decided && dispute.verdict ? `Verdict: ${VERDICT_LABEL[dispute.verdict]}` : 'Verdict',
      detail: decided
        ? `Recorded on-chain from ${dispute.votesClaimant + dispute.votesRespondent} revealed votes: ${dispute.votesClaimant} for the claimant, ${dispute.votesRespondent} for the respondent.`
        : 'Anyone may record it once the reveal window closes; the majority of revealed votes decides.',
      when: decided
        ? 'recorded'
        : stage === 'tally'
          ? 'waiting for the tally'
          : 'after the reveal window',
      state: decided ? 'done' : stage === 'tally' ? 'now' : 'next',
    },
    {
      key: 'appeal',
      label: 'Appeal window',
      detail: 'The escrow does not pay out until this window closes.',
      when: !decided
        ? 'after the verdict'
        : stage === 'appeal'
          ? left(dispute.appealDeadline, now)
          : 'closed',
      state: !decided ? 'next' : stage === 'appeal' ? 'now' : 'done',
    },
  ]

  const { settlement } = dispute
  if (settlement.status === 'settled') {
    steps.push({
      key: 'settled',
      label: 'Escrow paid out',
      detail: 'The escrow read the recorded verdict and moved the funds itself.',
      when: 'done',
      state: 'done',
      signature: settlement.signature,
    })
  } else {
    steps.push({
      key: 'settled',
      label: 'Escrow pays out',
      detail:
        settlement.status === 'untracked'
          ? 'This escrow is a program whose events this service does not read; check the escrow itself.'
          : 'Anyone may trigger the payout once the appeal window closes. No key can direct the funds elsewhere.',
      when:
        stage === 'final'
          ? settlement.status === 'untracked'
            ? 'not tracked here'
            : 'waiting for the escrow'
          : 'after the appeal window',
      state: stage === 'final' ? 'now' : 'next',
    })
  }

  return steps
}
