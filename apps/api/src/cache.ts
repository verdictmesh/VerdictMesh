import type { Db } from '@verdictmesh/db'
import { disputes, evidence } from '@verdictmesh/db'
import type { SQL } from 'drizzle-orm'
import { getTableColumns, sql } from 'drizzle-orm'
import type { PgColumn, PgTable } from 'drizzle-orm/pg-core'
import type { EvidenceRow } from './evidence.js'
import { type Cache, type DisputeRow, latestPerDispute } from './watcher.js'

/**
 * Writing the dispute mirror — and the evidence collected for it — into
 * Postgres.
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
 *
 * The key is passed in rather than read off `column.primary`: that flag is set
 * only for a single-column key, and a composite one — `evidence` — would be
 * rewritten onto itself.
 */
const fromExcluded = (table: PgTable, key: readonly PgColumn[]): Record<string, SQL> =>
  Object.fromEntries(
    Object.entries(getTableColumns(table))
      .filter(([, column]) => !key.includes(column))
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
      set: fromExcluded(disputes, [disputes.pda]),
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

/**
 * The freshest row per source of one dispute. Rows of one collection are
 * unique by construction; this holds the property for any batch, because the
 * alternative is not a duplicate row but an error of the whole statement —
 * the same `ON CONFLICT` limit as in `saveDisputes`.
 */
export function latestPerSource(rows: readonly EvidenceRow[]): EvidenceRow[] {
  const latest = new Map<string, EvidenceRow>()

  for (const row of rows) {
    const key = `${row.disputePda}/${row.source}`
    const known = latest.get(key)
    if (!known || known.slot <= row.slot) latest.set(key, row)
  }

  return [...latest.values()]
}

/**
 * A collected evidence set in one statement.
 *
 * The staleness guard is the same as the mirror's, and for the same reason:
 * the account row is a snapshot at the slot of the read, and a collection that
 * started earlier but finished later must not roll it back. A transaction row
 * never changes — its slot is the slot of the block — so for it the guard is
 * simply "nothing to do".
 *
 * `dispute_pda` references `disputes`: evidence can only be stored for a
 * dispute the mirror already holds, which is always true of a dispute the
 * watcher has just announced.
 */
export function saveEvidence(db: Db, rows: readonly EvidenceRow[]) {
  const key = [evidence.disputePda, evidence.source]

  return db
    .insert(evidence)
    .values(latestPerSource(rows))
    .onConflictDoUpdate({
      target: key,
      set: fromExcluded(evidence, key),
      setWhere: sql`${evidence.slot} < excluded."slot"`,
    })
}

/** Evidence as its consumer needs it. Writes go in sets, not rows. */
export interface EvidenceStore {
  save(rows: readonly EvidenceRow[]): Promise<void>
}

export function postgresEvidenceStore(db: Db): EvidenceStore {
  return {
    async save(rows) {
      // An escrow with no history and no account — a wrong network, most
      // likely — collects nothing, and `values([])` is a syntax error.
      if (rows.length === 0) return
      await saveEvidence(db, rows)
    },
  }
}
