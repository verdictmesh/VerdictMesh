import { readFileSync } from 'node:fs'
import type { FactFindingReport } from '@verdictmesh/shared'
import { describe, expect, it } from 'vitest'
import { referenceEscrowPositions } from '../claims.js'
import { escrowOwner, recoverPositions } from '../reporter.js'
import { FIXTURES, type Fixture, fixtureIds, loadFixture } from './fixture.js'
import { flagsOf, judge } from './judge.js'
import { build, knownPrograms } from './scenario.js'
import { scenarios } from './scenarios.js'

const programs = knownPrograms(referenceEscrowPositions)
const ids = fixtureIds()
const fixtures = new Map(ids.map((id) => [id, loadFixture(id)]))
const fixture = (id: string): Fixture => {
  const found = fixtures.get(id)
  if (!found) throw new Error(`No fixture ${id}`)
  return found
}

describe('the SC-007 set', () => {
  it('holds twenty disputes: seventeen modelled and three from devnet', () => {
    expect(ids).toHaveLength(20)
    expect(ids.filter((id) => id.includes('-devnet-'))).toHaveLength(3)
    expect(scenarios.map((scenario) => scenario.id)).toEqual(ids.slice(0, 17))
  })

  // The files are what the scenarios describe, byte for byte: a scenario
  // edited without `sc007:build` would measure a case nobody wrote down.
  it.each(scenarios)('$id is on disk as the scenario builds it', async (scenario) => {
    const built = await build(scenario, programs)
    const stored = fixture(scenario.id)
    expect(JSON.parse(JSON.stringify(built.dispute))).toEqual(stored.dispute)
    expect(JSON.parse(JSON.stringify(built.evidence))).toEqual(stored.evidence)
    expect(JSON.parse(JSON.stringify(built.truth))).toEqual(stored.truth)
  })

  it.each(ids)('%s gives the reporter what its answer key says', (id) => {
    const { dispute, evidence, truth } = fixture(id)
    const positions = recoverPositions(dispute, escrowOwner(evidence), programs)

    expect(dispute.pda).toBe(truth.dispute)
    expect(dispute.amount).toBe(truth.disputedAmount)
    expect(dispute.claimant).toBe(truth[truth.claimant])
    expect(dispute.respondent).toBe(truth[truth.respondent])
    if (truth.escrowProgramKnown) {
      const index = truth.disputedMilestone
      expect(positions.claimant.statement).toContain(`Milestone #${index}`)
      expect(positions.respondent.statement).toContain(`Milestone #${index}`)
    } else {
      expect(positions.claimant.statement).toBeNull()
      expect(positions.respondent.statement).toBeNull()
    }
  })

  it('keeps every digit of an amount past 2^53', () => {
    const { evidence } = fixture('11-amounts-beyond-float')
    expect(JSON.stringify(evidence)).toContain('"9007199254740993"')
  })

  it('cuts what the cases say is cut, and nothing else', () => {
    const cut = (id: string) =>
      fixture(id).evidence.rows.filter((row) => row.payload.logsTruncated === true).length
    expect(cut('12-release-logs-cut')).toBe(1)
    expect(fixture('13-history-cut').evidence.truncated).toBe(true)
    expect(ids.filter((id) => cut(id) > 0)).toEqual(['12-release-logs-cut'])
    expect(ids.filter((id) => fixture(id).evidence.truncated)).toEqual(['13-history-cut'])
  })
})

describe('the judge', () => {
  const f = fixture('05-second-dispute-after-status-quo')
  const { truth } = f
  const signature = (index: number) => {
    const found = truth.transactions[index]?.signature
    if (!found) throw new Error(`No transaction ${index} in the answer key`)
    return found
  }
  const create = signature(0)
  const release = signature(1)
  const settle = signature(3)
  const open = signature(4)
  const flags = (statement: string, source: { sourceSignature?: string; sourceAccount?: string }) =>
    flagsOf({ statement, ...source }, f).map((flag) => flag.kind)

  it('raises nothing on true statements about the right source', () => {
    expect(
      flags(`The escrow was opened with buyer ${truth.buyer} and total 100000000.`, {
        sourceSignature: create,
      }),
    ).toEqual([])
    expect(
      flags('Milestone 0 (15000000) was released to the seller.', { sourceSignature: release }),
    ).toEqual([])
    expect(
      flags('The earlier dispute over milestone 1 settled with no winner; 0 was paid.', {
        sourceSignature: settle,
      }),
    ).toEqual([])
    expect(
      flags('The claimant is the seller and paid a deposit of 5000000.', {
        sourceSignature: open,
      }),
    ).toEqual([])
    expect(
      flags('In the snapshot milestone index 2 (50000000) is Pending.', {
        sourceAccount: f.dispute.escrowRef,
      }),
    ).toEqual([])
  })

  it('flags the amount of another milestone', () => {
    expect(flags('Milestone 1 holds 50000000.', { sourceAccount: f.dispute.escrowRef })).toContain(
      'milestone-amount',
    )
  })

  it('reads ordinals as positions, not indices', () => {
    expect(
      flags('The second milestone holds 35000000.', { sourceAccount: f.dispute.escrowRef }),
    ).toEqual([])
    expect(
      flags('The first milestone holds 35000000.', { sourceAccount: f.dispute.escrowRef }),
    ).toContain('milestone-amount')
  })

  it('flags a state the milestone has never been in', () => {
    expect(flags('Milestone 1 was refunded.', { sourceSignature: settle })).toContain(
      'milestone-state',
    )
    expect(flags('Milestone 2 was released.', { sourceAccount: f.dispute.escrowRef })).toContain(
      'milestone-state',
    )
  })

  it('flags a milestone that does not exist', () => {
    expect(flags('Milestone 3 is pending.', { sourceAccount: f.dispute.escrowRef })).toContain(
      'unknown-milestone',
    )
  })

  it('flags the wrong role of a party', () => {
    expect(flags('The claimant is the buyer.', { sourceSignature: open })).toContain('role')
    expect(flags('The buyer is the claimant.', { sourceSignature: open })).toContain('role')
    expect(flags(`The buyer ${truth.seller} opened it.`, { sourceSignature: open })).toContain(
      'role',
    )
  })

  it('flags numbers and addresses the evidence does not hold, or the source does not', () => {
    expect(flags('The escrow total is 100000001.', { sourceSignature: create })).toContain(
      'unknown-number',
    )
    expect(flags('The escrow total is 100,000,001.', { sourceSignature: create })).toContain(
      'unknown-number',
    )
    expect(
      flags('The escrow total is 100,000,000 with a bond of 5,000,000.', {
        sourceSignature: create,
      }),
    ).toEqual([])
    expect(flags('Milestone 0 holds 15000000.', { sourceSignature: open })).toContain(
      'number-not-in-source',
    )
    expect(
      flags(`Funds went to ${'1'.repeat(10)}${truth.buyer.slice(10)}.`, {
        sourceSignature: create,
      }),
    ).toContain('unknown-address')
  })

  it('counts only labelled verdicts and refuses a flag overruled without a note', () => {
    const report: FactFindingReport = {
      summary: 'x',
      timeline: [],
      claims: [],
      gaps: [],
      facts: [
        {
          statement: 'Milestone 1 holds 50000000.',
          verdict: 'confirmed',
          sourceAccount: f.dispute.escrowRef,
        },
        {
          statement: 'Milestone 0 holds 15000000.',
          verdict: 'confirmed',
          sourceAccount: f.dispute.escrowRef,
        },
        { statement: 'Delivery happened.', verdict: 'unconfirmed' },
      ],
    }
    const unlabelled = judge(f, report)
    expect(unlabelled.confirmed).toHaveLength(2)
    expect(unlabelled.unlabelled).toBe(2)

    const [wrong, right] = unlabelled.confirmed
    const labelled = judge(
      {
        ...f,
        labels: {
          [wrong?.key ?? '']: { statement: '', label: 'consistent', note: '' },
          [right?.key ?? '']: { statement: '', label: 'consistent', note: '' },
        },
      },
      report,
    )
    expect(labelled.unlabelled).toBe(0)
    expect(labelled.overruledSilently).toBe(1)
  })
})

/**
 * The measured run, frozen: the reports the model wrote, the labels a person
 * gave every confirmed fact, and the totals in `sc007.json`. Re-judging the
 * same reports must give the same totals — a judge or a label changed since
 * the measurement shows up here, not in the next release note.
 */
describe('the SC-007 measurement', () => {
  const recorded = JSON.parse(readFileSync(`${FIXTURES}sc007.json`, 'utf8')) as {
    confirmed: number
    contradicted: number
    unverifiable: number
    consistent: number
  }
  const judged = ids.map((id) => {
    const { report } = fixture(id)
    if (!report) throw new Error(`${id} has no measured report`)
    return judge(fixture(id), report.report)
  })
  const total = (pick: (judgement: (typeof judged)[number]) => number) =>
    judged.reduce((sum, judgement) => sum + pick(judgement), 0)

  it('has a label on every confirmed fact, and no flag overruled without a note', () => {
    expect(judged.filter((judgement) => judgement.unlabelled > 0).map((j) => j.id)).toEqual([])
    expect(judged.filter((judgement) => judgement.overruledSilently > 0).map((j) => j.id)).toEqual(
      [],
    )
  })

  it('gives the totals recorded in sc007.json', () => {
    expect({
      confirmed: total((judgement) => judgement.confirmed.length),
      contradicted: total((judgement) => judgement.contradicted),
      unverifiable: total((judgement) => judgement.unverifiable),
      consistent: total((judgement) => judgement.consistent),
    }).toEqual({
      confirmed: recorded.confirmed,
      contradicted: recorded.contradicted,
      unverifiable: recorded.unverifiable,
      consistent: recorded.consistent,
    })
  })

  it('stores reports that passed checkReport — what a juror would have read', () => {
    for (const id of ids) {
      const { report } = fixture(id)
      for (const fact of report?.report.facts ?? []) {
        if (fact.verdict !== 'unconfirmed') {
          expect(fact.sourceSignature ?? fact.sourceAccount).toBeDefined()
        }
      }
    }
  })
})
