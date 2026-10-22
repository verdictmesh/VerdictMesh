import { readFileSync } from 'node:fs'
import { BN } from '@coral-xyz/anchor'
import { PublicKey } from '@solana/web3.js'
import { describe, expect, it } from 'vitest'
import { type Dispute, verdictStatus } from './verdict.js'

/**
 * The same table drives `verdict_read.rs`, so the client's prediction and the
 * escrow's decision on chain are checked against one answer.
 */
const table = JSON.parse(
  readFileSync(new URL('../../../fixtures/verdict-read/cases.json', import.meta.url), 'utf8'),
) as {
  appealDeadline: number
  cases: {
    name: string
    state: string
    verdict: string | null
    now: number
    ours: boolean
    expect: string
  }[]
}

/** `Tallied` → `{ tallied: {} }`, the way Anchor decodes an enum. */
const decoded = (name: string) => ({ [name.charAt(0).toLowerCase() + name.slice(1)]: {} })

function dispute(input: { escrowRef: PublicKey; state: string; verdict: string | null }) {
  return {
    escrowRef: input.escrowRef,
    state: decoded(input.state),
    verdict: input.verdict === null ? null : decoded(input.verdict),
    appealDeadline: new BN(table.appealDeadline),
    claimant: PublicKey.unique(),
    respondent: PublicKey.unique(),
    policy: { deposit: new BN(5_000_000) },
  } as unknown as Dispute
}

describe('verdictStatus', () => {
  it('has the cases the program is tested on', () => {
    expect(table.cases.length).toBeGreaterThanOrEqual(10)
  })

  for (const c of table.cases) {
    it(c.name, () => {
      const escrow = PublicKey.unique()
      const subject = dispute({
        escrowRef: c.ours ? escrow : PublicKey.unique(),
        state: c.state,
        verdict: c.verdict,
      })

      const got = verdictStatus(subject, escrow, c.now)

      if (['Claimant', 'Respondent', 'StatusQuo'].includes(c.expect)) {
        expect(got).toMatchObject({ status: 'ready', verdict: Object.keys(decoded(c.expect))[0] })
      } else {
        expect(got.status).toBe(c.expect)
      }
    })
  }

  it('names the party whose position won, and nobody on a status quo', () => {
    const escrow = PublicKey.unique()
    const subject = dispute({ escrowRef: escrow, state: 'Tallied', verdict: 'Claimant' })
    const at = table.appealDeadline

    expect(verdictStatus(subject, escrow, at)).toMatchObject({ winner: subject.claimant })
    subject.verdict = decoded('Respondent') as Dispute['verdict']
    expect(verdictStatus(subject, escrow, at)).toMatchObject({ winner: subject.respondent })
    subject.verdict = decoded('StatusQuo') as Dispute['verdict']
    expect(verdictStatus(subject, escrow, at)).toMatchObject({ winner: null })
  })

  it('gives the deposit of the snapshot and the deadline still to wait for', () => {
    const escrow = PublicKey.unique()
    const subject = dispute({ escrowRef: escrow, state: 'Tallied', verdict: 'Claimant' })

    expect(verdictStatus(subject, escrow, table.appealDeadline)).toMatchObject({
      deposit: 5_000_000n,
    })
    expect(verdictStatus(subject, escrow, 0)).toEqual({
      status: 'AppealWindowOpen',
      until: table.appealDeadline,
    })
  })
})
