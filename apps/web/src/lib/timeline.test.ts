import type { DisputeView } from '@verdictmesh/shared'
import { describe, expect, it } from 'vitest'
import { buildTimeline } from './timeline'

const dispute: DisputeView = {
  pda: 'P'.repeat(44),
  integrator: 'I'.repeat(44),
  escrowRef: 'E'.repeat(44),
  claimant: 'C'.repeat(44),
  respondent: 'R'.repeat(44),
  amount: '20000000',
  state: 'Committing',
  panel: ['J'.repeat(44)],
  reportHash: null,
  openedAt: 0,
  commitDeadline: 1_000,
  revealDeadline: 2_000,
  appealDeadline: 0,
  votesClaimant: 0,
  votesRespondent: 0,
  escalated: false,
  verdict: null,
  settlement: { status: 'awaiting' },
}

const states = (d: DisputeView, now: number) =>
  Object.fromEntries(buildTimeline(d, now).map((step) => [step.key, step.state]))

const decided: DisputeView = {
  ...dispute,
  state: 'Tallied',
  verdict: 'Respondent',
  appealDeadline: 3_000,
  votesClaimant: 1,
  votesRespondent: 2,
}

describe('buildTimeline', () => {
  it('has exactly one step in progress while the commit window is open', () => {
    expect(states(dispute, 500)).toEqual({
      opened: 'done',
      commit: 'now',
      reveal: 'next',
      verdict: 'next',
      appeal: 'next',
      settled: 'next',
    })
  })

  it('waits for the tally once both windows close', () => {
    expect(states(dispute, 2_500)).toMatchObject({ reveal: 'done', verdict: 'now' })
  })

  it('names the verdict and the revealed votes', () => {
    const verdict = buildTimeline(decided, 2_500).find((step) => step.key === 'verdict')
    expect(verdict?.label).toBe('Verdict: For the respondent')
    expect(verdict?.detail).toContain('1 for the claimant, 2 for the respondent')
  })

  it('links the escrow settlement transaction once known (FR-020)', () => {
    const settled = buildTimeline(
      {
        ...decided,
        state: 'Finalized',
        settlement: { status: 'settled', signature: 'S'.repeat(88), slot: 9 },
      },
      5_000,
    ).at(-1)
    expect(settled).toMatchObject({ state: 'done', signature: 'S'.repeat(88) })
  })

  it('says when the escrow is not one this service reads', () => {
    const last = buildTimeline({ ...decided, settlement: { status: 'untracked' } }, 5_000).at(-1)
    expect(last?.when).toBe('not tracked here')
    expect(last?.signature).toBeUndefined()
  })
})
