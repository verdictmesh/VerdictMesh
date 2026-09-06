import { describe, expect, it } from 'vitest'
import { disputeView, factFindingReport } from './index.js'

const validReport = {
  summary: 'Milestone was delivered late but within the grace period.',
  timeline: [{ at: 1_700_000_000, what: 'Escrow funded', sourceSignature: 'a'.repeat(88) }],
  facts: [
    {
      statement: 'Escrow held 500 USDC at dispute open',
      verdict: 'confirmed' as const,
      sourceAccount: 'b'.repeat(44),
    },
  ],
  claims: [
    {
      party: 'claimant' as const,
      statement: 'Work was never delivered',
      assessment: 'contradicted' as const,
    },
  ],
  gaps: [],
}

describe('factFindingReport', () => {
  it('accepts a well-formed report', () => {
    expect(factFindingReport.parse(validReport)).toEqual(validReport)
  })

  it('requires gaps to be present even when empty', () => {
    const { gaps: _gaps, ...withoutGaps } = validReport
    expect(factFindingReport.safeParse(withoutGaps).success).toBe(false)
  })

  it('rejects a fact verdict outside the three allowed values', () => {
    const report = { ...validReport, facts: [{ ...validReport.facts[0], verdict: 'probably' }] }
    expect(factFindingReport.safeParse(report).success).toBe(false)
  })

  it('rejects an empty statement rather than silently keeping it', () => {
    const report = { ...validReport, facts: [{ ...validReport.facts[0], statement: '' }] }
    expect(factFindingReport.safeParse(report).success).toBe(false)
  })
})

describe('disputeView', () => {
  const base = {
    pda: 'c'.repeat(44),
    integrator: 'd'.repeat(44),
    escrowRef: 'e'.repeat(44),
    claimant: 'f'.repeat(44),
    respondent: 'g'.repeat(44),
    amount: '500000000',
    state: 'Committing' as const,
    panel: ['h'.repeat(44)],
    reportHash: null,
    commitDeadline: 1_700_000_060,
    revealDeadline: 1_700_000_120,
    appealDeadline: 1_700_000_210,
    escalated: false,
    verdict: null,
    settled: false,
  }

  it('accepts a dispute with no report attested yet', () => {
    expect(disputeView.parse(base)).toEqual(base)
  })

  it('keeps amount as a string so u64 survives the boundary', () => {
    const parsed = disputeView.parse({ ...base, amount: '18446744073709551615' })
    expect(parsed.amount).toBe('18446744073709551615')
  })

  it('rejects a report hash that is not 32 bytes hex', () => {
    expect(disputeView.safeParse({ ...base, reportHash: 'deadbeef' }).success).toBe(false)
  })
})
