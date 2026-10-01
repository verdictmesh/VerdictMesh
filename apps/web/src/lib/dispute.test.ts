import type { DisputeView } from '@verdictmesh/shared'
import { describe, expect, it } from 'vitest'
import { formatAgo, formatAmount, recipientOf, type Stage, stageDeadline, stageOf } from './dispute'

const base: Pick<
  DisputeView,
  'state' | 'verdict' | 'commitDeadline' | 'revealDeadline' | 'appealDeadline' | 'settlement'
> = {
  state: 'Committing',
  verdict: null,
  commitDeadline: 1_000,
  revealDeadline: 2_000,
  appealDeadline: 0,
  settlement: { status: 'awaiting' },
}

const decided = {
  ...base,
  state: 'Tallied' as const,
  verdict: 'Claimant' as const,
  appealDeadline: 3_000,
}

describe('stageOf', () => {
  it.each<[string, Parameters<typeof stageOf>[0], number, Stage]>([
    ['before the commit deadline', base, 999, 'commit'],
    // `commit_vote` requires now < commit_deadline; `reveal_vote` now >= it.
    ['at the commit deadline', base, 1_000, 'reveal'],
    ['before the reveal deadline', base, 1_999, 'reveal'],
    ['at the reveal deadline, untallied', base, 2_000, 'tally'],
    // The state on chain lags: a dispute still `Committing` long after its
    // windows takes no votes.
    ['hours later, still Committing on chain', base, 50_000, 'tally'],
    ['with a verdict, inside the appeal window', decided, 2_500, 'appeal'],
    ['with a verdict, after the appeal window', decided, 3_000, 'final'],
    [
      'settled by the escrow, even inside the appeal window',
      { ...decided, settlement: { status: 'settled', signature: 's'.repeat(88), slot: 1 } },
      2_500,
      'settled',
    ],
    ['on the optimistic track', { ...base, state: 'OptimisticPending' }, 0, 'optimistic'],
  ])('%s', (_, dispute, now, stage) => {
    expect(stageOf(dispute, now)).toBe(stage)
  })

  it('runs each open stage to its own deadline', () => {
    expect(stageDeadline(base, 'commit')).toBe(1_000)
    expect(stageDeadline(base, 'reveal')).toBe(2_000)
    expect(stageDeadline(decided, 'appeal')).toBe(3_000)
    expect(stageDeadline(decided, 'final')).toBeNull()
  })
})

describe('formatAmount', () => {
  it('reads base units with six decimals', () => {
    expect(formatAmount('20000000')).toBe('20.00 USDC')
    expect(formatAmount('1234567')).toBe('1.234567 USDC')
    expect(formatAmount('0')).toBe('0.00 USDC')
  })

  it('keeps every digit of the largest u64', () => {
    expect(formatAmount('18446744073709551615')).toBe('18,446,744,073,709.551615 USDC')
  })
})

describe('recipientOf', () => {
  const parties = { claimant: 'C', respondent: 'R' }
  it('pays the side the verdict names, and nobody under the status quo', () => {
    expect(recipientOf({ ...parties, verdict: 'Claimant' })).toBe('C')
    expect(recipientOf({ ...parties, verdict: 'Respondent' })).toBe('R')
    expect(recipientOf({ ...parties, verdict: 'StatusQuo' })).toBeNull()
    expect(recipientOf({ ...parties, verdict: null })).toBeNull()
  })
})

describe('formatAgo', () => {
  it('is coarse', () => {
    expect(formatAgo(0, 30)).toBe('just now')
    expect(formatAgo(0, 61)).toBe('1 minute ago')
    expect(formatAgo(0, 7_300)).toBe('2 hours ago')
    expect(formatAgo(0, 200_000)).toBe('2 days ago')
  })
})
