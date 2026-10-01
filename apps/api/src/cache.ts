import type { Db } from '@verdictmesh/db'
import { disputes, evidence, reports, settlements } from '@verdictmesh/db'
import type { SQL } from 'drizzle-orm'
import { and, arrayContains, asc, desc, eq, getTableColumns, inArray, sql } from 'drizzle-orm'
import type { PgColumn, PgTable } from 'drizzle-orm/pg-core'
import type { EvidenceRow } from './evidence.js'
import type { ReportRow, ReportStore } from './reporter.js'
import type { DisputeFilter, DisputeMirror } from './routes/disputes.js'
import type { MirroredDisputes, PublishedReports } from './routes/reports.js'
import type { SettlementRow, SettlementStore } from './settlement.js'
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

/**
 * A report is inserted, never updated. `FR-017` makes the fingerprint on chain
 * final, and a second generation racing the first — the event and a rewrite
 * offering the same dispute at once — must not replace the body the
 * fingerprint was taken from. The loser of that race simply does nothing.
 */
export function saveReport(db: Db, row: ReportRow) {
  return db.insert(reports).values(row).onConflictDoNothing()
}

/**
 * The fingerprint of the first stored version — the one `attest_report` puts
 * on chain. Read back after every save rather than taken from the row just
 * written: when two generations race, the stored body is the winner's.
 */
export function storedReportHash(db: Db, disputePda: string) {
  return db
    .select({ contentHash: reports.contentHash })
    .from(reports)
    .where(eq(reports.disputePda, disputePda))
    .orderBy(asc(reports.version))
    .limit(1)
}

export function postgresReportStore(db: Db): ReportStore {
  return {
    async storedHash(disputePda) {
      const [row] = await storedReportHash(db, disputePda)
      return row?.contentHash ?? null
    },
    async save(row) {
      await saveReport(db, row)
    },
  }
}

/**
 * The body of the first stored version — the same row `storedReportHash`
 * reads, so what is served is what was attested. Only the body: its stored
 * hash is exactly what `GET /disputes/:pda/report` must not trust.
 */
export function firstStoredReport(db: Db, disputePda: string) {
  return db
    .select({ content: reports.content })
    .from(reports)
    .where(eq(reports.disputePda, disputePda))
    .orderBy(asc(reports.version))
    .limit(1)
}

export function postgresPublishedReports(db: Db): PublishedReports {
  return {
    async first(disputePda) {
      const [row] = await firstStoredReport(db, disputePda)
      return row?.content ?? null
    },
  }
}

/** What the report route needs of a mirror row when there is no report. */
export function mirroredDispute(db: Db, pda: string) {
  return db
    .select({
      state: disputes.state,
      escalated: disputes.escalated,
      reportHash: disputes.reportHash,
      commitDeadline: disputes.commitDeadline,
    })
    .from(disputes)
    .where(eq(disputes.pda, pda))
}

export function postgresMirroredDisputes(db: Db): MirroredDisputes {
  return {
    async find(pda) {
      const [row] = await mirroredDispute(db, pda)
      return row ?? null
    },
  }
}

/** Inserted once: a second sighting of the same settlement changes nothing. */
export function saveSettlement(db: Db, row: SettlementRow) {
  return db.insert(settlements).values(row).onConflictDoNothing()
}

export function postgresSettlementStore(db: Db): SettlementStore {
  return {
    async save(row) {
      await saveSettlement(db, row)
    },
    async settled(disputePdas) {
      if (disputePdas.length === 0) return new Set()
      const rows = await db
        .select({ pda: settlements.disputePda })
        .from(settlements)
        .where(inArray(settlements.disputePda, [...disputePdas]))
      return new Set(rows.map((row) => row.pda))
    },
  }
}

/**
 * Mirror rows with their settlement, newest first. The mirror answers alone:
 * `SC-010` gives the first screen two seconds, and an RPC call per row would
 * spend them.
 */
export function listDisputes(db: Db, filter: DisputeFilter, limit: number) {
  const conditions = [
    filter.state === undefined ? undefined : eq(disputes.state, filter.state),
    filter.integrator === undefined ? undefined : eq(disputes.integrator, filter.integrator),
    // `@>` rather than `= any(...)`: it is what the GIN index on `panel` serves.
    filter.juror === undefined ? undefined : arrayContains(disputes.panel, [filter.juror]),
    filter.pda === undefined ? undefined : eq(disputes.pda, filter.pda),
  ]

  return db
    .select({ dispute: disputes, settlement: settlements })
    .from(disputes)
    .leftJoin(settlements, eq(settlements.disputePda, disputes.pda))
    .where(and(...conditions))
    .orderBy(desc(disputes.openedAt))
    .limit(limit)
}

export function postgresDisputeMirror(db: Db): DisputeMirror {
  return {
    async list(filter, limit) {
      const rows = await listDisputes(db, filter, limit)
      return rows.map(({ dispute, settlement }) => ({
        dispute,
        settlement: settlement && { signature: settlement.signature, slot: settlement.slot },
      }))
    },
  }
}
