import type { Db } from '@verdictmesh/db'
import { disputes } from '@verdictmesh/db'
import type { SQL } from 'drizzle-orm'
import { getTableColumns, sql } from 'drizzle-orm'
import { type Cache, type DisputeRow, latestPerDispute } from './watcher.js'

/**
 * Writing the dispute mirror into Postgres.
 *
 * The whole point is one line — `setWhere`. The watcher has two sources of
 * snapshots moving at different speeds: the subscription brings one dispute
 * immediately, while the periodic rewrite brings all of them at a slot that may
 * already be stale by the time the batch reaches the database. Without a
 * staleness guard, a rewrite that won the race would roll a dispute back to a
 * state it had already left — and that would look like "the panel showed an old
 * deadline for a second", which is to say, like nothing at all.
 */

/**
 * Every column but the key takes its value from the row that lost the
 * conflict — `excluded`. The list is built from the table rather than written
 * out by hand: a new column added to the schema and forgotten here would only
 * ever be set on the first insert and keep its first value forever after.
 */
const fromExcluded = (): Record<string, SQL> =>
  Object.fromEntries(
    Object.entries(getTableColumns(disputes))
      .filter(([, column]) => !column.primary)
      .map(([field, column]) => [field, sql.raw(`excluded."${column.name}"`)]),
  )

/**
 * A batch of snapshots in one statement.
 *
 * `latestPerDispute` is not politeness here: `ON CONFLICT` cannot touch the
 * same row twice within one statement and answers that with an error, and a
 * batch holding two snapshots of one dispute is assembled every time the
 * rewrite and the subscription meet on it.
 */
export function saveDisputes(db: Db, rows: readonly DisputeRow[]) {
  return db
    .insert(disputes)
    .values(latestPerDispute(rows))
    .onConflictDoUpdate({
      target: disputes.pda,
      set: fromExcluded(),
      // Strictly less than: a snapshot from the same slot adds nothing, and a
      // row rewritten onto itself would still wake triggers and replication.
      setWhere: sql`${disputes.syncedSlot} < excluded."synced_slot"`,
    })
}

export function postgresCache(db: Db): Cache {
  return {
    async save(rows) {
      // An empty `values([])` is not "do nothing", it is a syntax error in
      // drizzle. A rewrite of a program with no disputes arrives here with
      // exactly that batch.
      if (rows.length === 0) return
      await saveDisputes(db, rows)
    },
  }
}
