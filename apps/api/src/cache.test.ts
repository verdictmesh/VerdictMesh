import { createDb, disputes } from '@verdictmesh/db'
import { getTableColumns } from 'drizzle-orm'
import { describe, expect, it, vi } from 'vitest'
import { postgresCache, saveDisputes } from './cache.js'
import type { DisputeRow } from './watcher.js'

/**
 * `postgres` does not connect until a query is executed, and `.toSQL()` does
 * not execute one — so the SQL is checked without a database. And it is the SQL
 * that matters here: `setWhere` is the only thing standing between a race of
 * two snapshot sources and a silent rollback of a dispute to an older state.
 */
const db = createDb('postgresql://user:pass@localhost:5432/postgres')

const row = (pda: string, syncedSlot: number, state: DisputeRow['state']): DisputeRow => ({
  pda,
  integrator: 'Ea1p2xPRBqNGqPqrRGMnrE1L9fBUgpfoXGCFhZUHh8dz',
  escrowRef: 'HFCNHUwPxRqqW6gaLd3uUjJcEUfjnRptJHYas4ua59fB',
  claimant: '7Np41oeYqPefeNQEHSv1UDhYrehxin3NStELsSKCT4K2',
  respondent: 'GqjGVGrxTPqCwn9x1MTxd1L8qBPhTEXaRAgErVbNsMpH',
  amount: 18_446_744_073_709_551_615n,
  state,
  panel: [],
  reportHash: null,
  claimantClaimHash: '0'.repeat(64),
  respondentClaimHash: '1'.repeat(64),
  openedAt: 1_700_000_000,
  commitDeadline: 1_700_000_060,
  revealDeadline: 1_700_000_120,
  appealDeadline: 0,
  votesClaimant: 0,
  votesRespondent: 0,
  escalated: false,
  verdict: null,
  syncedSlot,
})

describe('storing snapshots', () => {
  it('updates a row only from a snapshot of a later slot', () => {
    const { sql } = saveDisputes(db, [row('a', 10, 'Committing')]).toSQL()

    expect(sql).toContain('on conflict ("pda") do update set')
    expect(sql).toContain('where "disputes"."synced_slot" < excluded."synced_slot"')
  })

  /**
   * The column list in `set` is built from the table rather than written out by
   * hand. This test holds exactly that property: a column added by a future
   * migration would otherwise only ever be set on the first insert.
   */
  it('takes every column but the key from excluded', () => {
    const { sql } = saveDisputes(db, [row('a', 10, 'Committing')]).toSQL()
    const updated = sql.slice(sql.indexOf('do update set'))

    for (const column of Object.values(getTableColumns(disputes))) {
      if (column.name === 'pda') continue
      expect(updated).toContain(`excluded."${column.name}"`)
    }

    expect(updated).not.toContain('excluded."pda"')
  })

  /**
   * `ON CONFLICT` cannot touch the same row twice and answers that with an
   * error. A batch holding two snapshots of one dispute is assembled every time
   * the rewrite and the subscription meet on it.
   */
  it('leaves one snapshot per dispute in a batch — the freshest', () => {
    const { sql, params } = saveDisputes(db, [
      row('a', 10, 'Committing'),
      row('a', 12, 'Revealing'),
      row('b', 3, 'OptimisticPending'),
    ]).toSQL()

    expect(sql.match(/\$1\b/g)).toHaveLength(1)
    expect(params).toContain('Revealing')
    expect(params).not.toContain('Committing')
    expect(params.filter((value) => value === 'a' || value === 'b')).toEqual(['a', 'b'])
  })

  it('builds no statement for an empty batch', async () => {
    const insert = vi.spyOn(db, 'insert')
    await postgresCache(db).save([])
    expect(insert).not.toHaveBeenCalled()
    insert.mockRestore()
  })
})
