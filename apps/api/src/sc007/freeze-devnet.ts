/**
 * Freezes disputes that really happened on devnet into `fixtures/disputes/`:
 * the dispute row and the evidence exactly as the reporter stored them, and an
 * answer key read off that evidence. They anchor the modelled cases — if the
 * model behaves differently on these, the modelled input has drifted from the
 * real one.
 *
 * All devnet disputes so far have one shape (`e2e-dispute.ts`: the seller
 * disputes milestone 1 of 3 right after creation), so three are enough.
 *
 *   pnpm --filter @verdictmesh/api sc007:freeze <pda> <pda> <pda>
 */
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { createDb, disputes, evidence as evidenceTable } from '@verdictmesh/db'
import { desc, eq } from 'drizzle-orm'
import type { EvidenceRow, EvidenceSet, Json } from '../evidence.js'
import { saveInputs } from './fixture.js'
import type { Truth, TruthTransaction } from './scenario.js'

const FIRST_ID = 18

const envFile = fileURLToPath(new URL('../../../../.env', import.meta.url))
const env = Object.fromEntries(
  readFileSync(envFile, 'utf8')
    .split(/\r?\n/)
    .filter((line) => /^[A-Z_0-9]+=/.test(line))
    .map((line) => [line.slice(0, line.indexOf('=')), line.slice(line.indexOf('=') + 1)]),
)
const db = createDb(process.env.DATABASE_URL ?? env.DATABASE_URL ?? '')

type Event = { name: string; data: Record<string, Json> }
const eventsOf = (row: EvidenceRow) => (row.payload.events ?? []) as Event[]
const text = (value: Json | undefined) => String(value)

function truthOf(dispute: { pda: string }, evidence: EvidenceSet): Truth {
  const events = evidence.rows.flatMap((row) => eventsOf(row))
  const opened = events.find((event) => event.name === 'EscrowOpened')?.data
  const disputed = events.find(
    (event) => event.name === 'MilestoneDisputed' && event.data.dispute === dispute.pda,
  )?.data
  const deposit = events.find(
    (event) => event.name === 'DepositCollected' && event.data.dispute === dispute.pda,
  )?.data
  const account = evidence.rows.find((row) => row.kind === 'account')?.payload.data as
    | { milestones: { amount: string; state: Record<string, Json> }[] }
    | undefined
  if (!opened || !disputed || !deposit || !account) {
    throw new Error(`${dispute.pda}: not the shape of a devnet e2e dispute`)
  }

  const claimant = disputed.claimant === opened.buyer ? 'buyer' : 'seller'
  const transactions: TruthTransaction[] = evidence.rows
    .filter((row) => row.kind === 'transaction')
    .map((row) => ({
      signature: row.source,
      slot: row.slot,
      blockTime: Number(row.payload.blockTime),
      what: eventsOf(row)
        .map((event) => event.name)
        .join(', '),
    }))
    .sort((a, b) => a.slot - b.slot)

  return {
    title: `Frozen from devnet: the ${claimant} disputes milestone ${text(disputed.milestone)} right after creation.`,
    traps: [
      'Milestone 1 (index) called the first milestone, or given the amount of index 0 or 2.',
      'The deposit and the bond (equal here) confused with each other or with an amount.',
    ],
    buyer: text(opened.buyer),
    seller: text(opened.seller),
    claimant,
    respondent: claimant === 'buyer' ? 'seller' : 'buyer',
    dispute: dispute.pda,
    disputedMilestone: Number(disputed.milestone),
    disputedAmount: text(disputed.amount),
    bond: text(opened.bond),
    deposit: text(deposit.amount),
    total: text(opened.total),
    milestones: account.milestones.map((milestone, index) => {
      const state = Object.keys(milestone.state)[0] ?? 'unknown'
      return {
        index,
        amount: milestone.amount,
        state,
        passed: state === 'Pending' ? ['Pending'] : ['Pending', state],
      }
    }),
    transactions,
    collected: { transactions: transactions.length, truncated: false, logsCut: 0 },
    escrowProgramKnown: true,
  }
}

const pdas = process.argv.slice(2)
for (const [offset, pda] of pdas.entries()) {
  const [row] = await db.select().from(disputes).where(eq(disputes.pda, pda))
  if (!row) throw new Error(`No dispute ${pda} in the mirror`)

  const stored = await db
    .select()
    .from(evidenceTable)
    .where(eq(evidenceTable.disputePda, pda))
    .orderBy(desc(evidenceTable.slot))
  // As `collectEvidence` returns them: transactions newest first, the account last.
  const rows = [...stored]
    .sort((a, b) => (a.kind === b.kind ? 0 : a.kind === 'account' ? 1 : -1))
    .map(
      (entry): EvidenceRow => ({
        disputePda: pda,
        kind: entry.kind,
        source: entry.source,
        slot: Number(entry.slot),
        payload: entry.payload as Record<string, Json>,
      }),
    )
  const evidence: EvidenceSet = { rows, truncated: false }

  const dispute = {
    pda,
    escrowRef: row.escrowRef,
    claimant: row.claimant,
    respondent: row.respondent,
    amount: row.amount.toString(),
    openedAt: Number(row.openedAt),
    claimantClaimHash: row.claimantClaimHash,
    respondentClaimHash: row.respondentClaimHash,
  }
  const id = `${FIRST_ID + offset}-devnet-${pda.slice(0, 6)}`
  saveInputs(id, { dispute, evidence, truth: truthOf(dispute, evidence) })
  console.log(`${id}: ${rows.length} evidence row(s)`)
}

await db.$client.end()
