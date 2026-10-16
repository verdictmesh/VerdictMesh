import type { FactFindingReport } from '@verdictmesh/shared'
import type { Json } from '../evidence.js'
import { type Fixture, factKey, type Label } from './fixture.js'

/**
 * The `SC-007` judge: no statement marked "confirmed" may be contradicted by
 * the chain.
 *
 * `checkReport` already guarantees that every confirmed fact cites collected
 * evidence. It does not look at what the fact **says**: a wrong amount next
 * to a real signature passes it. That is what this judge is for, and it works
 * in two layers.
 *
 * **Flags** are mechanical: every address and number in the statement is
 * looked up in the evidence, every milestone it names is checked against the
 * amounts and states that milestone has had, every role against the parties.
 * A flag is a suspicion, not a verdict — "the amount of milestone 0" may be
 * correct in a sentence that also mentions milestone 1.
 *
 * **Labels** are the verdict, written by a person who read the fact against
 * `truth.json` and stored in `labels.json`. Every confirmed fact needs one,
 * and a flag can be overruled only with a note. A second model as the judge
 * was rejected: it fails at the very task it would be checking.
 */

export type FlagKind =
  | 'unknown-address'
  | 'address-not-in-source'
  | 'unknown-number'
  | 'number-not-in-source'
  | 'milestone-amount'
  | 'milestone-state'
  | 'unknown-milestone'
  | 'role'

export interface Flag {
  kind: FlagKind
  detail: string
}

export interface JudgedFact {
  key: string
  statement: string
  source: string | null
  flags: Flag[]
  label: Label | null
  note: string
}

export interface Judgement {
  id: string
  confirmed: JudgedFact[]
  contradicted: number
  unverifiable: number
  consistent: number
  /** Confirmed facts nobody has labelled yet. */
  unlabelled: number
  /** Flagged facts labelled consistent or unverifiable without a note. */
  overruledSilently: number
}

const ADDRESS = /\b[1-9A-HJ-NP-Za-km-z]{32,88}\b/g
/**
 * Integers, with or without thousands separators. A sentence may end right
 * after one: "a bond of 4,000,000." is 4000000, not 4,000.
 */
const NUMBER = /(?<![\w.,])\d{1,3}(?:[, ]\d{3})+(?!\w|[.,]\d)|(?<![\w.,])\d+(?!\w|[.,]\d)/g
const ORDINALS = ['first', 'second', 'third', 'fourth', 'fifth', 'sixth', 'seventh', 'eighth']
const STATES = ['released', 'refunded', 'pending', 'disputed'] as const

/** Every string and number leaf of a payload, flattened. */
function leaves(value: Json | undefined, into: Set<string>): Set<string> {
  if (value === null || value === undefined) return into
  if (Array.isArray(value)) {
    for (const item of value) leaves(item, into)
  } else if (typeof value === 'object') {
    for (const item of Object.values(value)) leaves(item, into)
  } else {
    into.add(String(value))
  }
  return into
}

interface Universe {
  /** Every value anywhere in the evidence and the dispute. */
  all: Set<string>
  /** The values of each source, by signature or address. */
  bySource: Map<string, Set<string>>
}

function universeOf(fixture: Fixture): Universe {
  const { dispute, evidence } = fixture
  const disputeValues = new Set(
    [
      dispute.pda,
      dispute.escrowRef,
      dispute.claimant,
      dispute.respondent,
      dispute.amount,
      String(dispute.openedAt),
    ].map(String),
  )

  const bySource = new Map<string, Set<string>>([[dispute.pda, new Set(disputeValues)]])
  for (const row of evidence.rows) {
    const values = leaves(row.payload as Json, new Set([row.source, String(row.slot)]))
    bySource.set(row.source, new Set([...(bySource.get(row.source) ?? []), ...values]))
  }

  const all = new Set(disputeValues)
  for (const values of bySource.values()) for (const value of values) all.add(value)
  return { all, bySource }
}

/** Milestone indices a statement names: "milestone 2", "#2", "index 2", "third milestone". */
function milestonesNamed(statement: string): number[] {
  const named = new Set<number>()
  for (const match of statement.matchAll(/milestones?\s*(?:#|no\.?\s*|index\s*)?(\d+)/gi)) {
    named.add(Number(match[1]))
  }
  for (const match of statement.matchAll(/\bindex\s*(\d+)/gi)) named.add(Number(match[1]))
  ORDINALS.forEach((ordinal, index) => {
    if (new RegExp(`\\b${ordinal}\\s+milestone`, 'i').test(statement)) named.add(index)
  })
  return [...named]
}

export function flagsOf(
  fact: { statement: string; sourceSignature?: string; sourceAccount?: string },
  fixture: Fixture,
  universe = universeOf(fixture),
): Flag[] {
  const flags: Flag[] = []
  const { statement } = fact
  const { truth } = fixture
  const cited = [fact.sourceSignature, fact.sourceAccount]
    .filter((source): source is string => source !== undefined)
    .flatMap((source) => [...(universe.bySource.get(source) ?? [])])
  const inSource = new Set(cited)

  const addresses = [...statement.matchAll(ADDRESS)].map((match) => match[0])
  for (const address of addresses) {
    if (!universe.all.has(address)) {
      flags.push({ kind: 'unknown-address', detail: address })
    } else if (!inSource.has(address)) {
      flags.push({ kind: 'address-not-in-source', detail: address })
    }
  }

  // Addresses are taken out first: their digits are not numbers.
  const text = addresses.reduce((rest, address) => rest.replaceAll(address, ' '), statement)
  const numbers = [...text.matchAll(NUMBER)].map((match) => match[0].replace(/[, ]/g, ''))
  const amounts = numbers.filter((number) => BigInt(number) >= 100n)
  for (const number of amounts) {
    if (!universe.all.has(number)) {
      flags.push({ kind: 'unknown-number', detail: number })
    } else if (!inSource.has(number)) {
      flags.push({ kind: 'number-not-in-source', detail: number })
    }
  }

  const named = milestonesNamed(text)
  for (const index of named) {
    if (!truth.milestones.some((milestone) => milestone.index === index)) {
      flags.push({ kind: 'unknown-milestone', detail: `milestone ${index}` })
    }
  }

  // A sentence about one milestone: its amounts and states must be that
  // milestone's. With two or more named, the pairing is not mechanical.
  const [only] = named
  const milestone =
    named.length === 1 ? truth.milestones.find((entry) => entry.index === only) : undefined
  if (milestone) {
    const otherAmounts = new Set(
      truth.milestones
        .filter((entry) => entry.index !== milestone.index)
        .map((entry) => entry.amount),
    )
    for (const number of amounts) {
      if (number !== milestone.amount && otherAmounts.has(number)) {
        flags.push({
          kind: 'milestone-amount',
          detail: `${number} is not the amount of milestone ${milestone.index} (${milestone.amount})`,
        })
      }
    }

    const passed = new Set(milestone.passed.map((state) => state.toLowerCase()))
    for (const state of STATES) {
      if (new RegExp(`\\b${state}\\b`, 'i').test(text) && !passed.has(state)) {
        flags.push({
          kind: 'milestone-state',
          detail: `milestone ${milestone.index} was never ${state} (${milestone.passed.join(' → ')})`,
        })
      }
    }
  }

  const roles = { buyer: truth.buyer, seller: truth.seller } as const
  const claimantRole = truth.claimant
  const respondentRole = truth.respondent
  const mismatch = (said: string, actual: string, who: string) => {
    if (said !== actual)
      flags.push({ kind: 'role', detail: `${who} is the ${actual}, not the ${said}` })
  }
  for (const match of text.matchAll(/claimant\W+(?:\w+\W+){0,3}?(buyer|seller)\b/gi)) {
    mismatch(match[1]?.toLowerCase() ?? '', claimantRole, 'the claimant')
  }
  for (const match of text.matchAll(/\b(buyer|seller)\W+(?:\w+\W+){0,3}?claimant\b/gi)) {
    mismatch(match[1]?.toLowerCase() ?? '', claimantRole, 'the claimant')
  }
  for (const match of text.matchAll(/respondent\W+(?:\w+\W+){0,3}?(buyer|seller)\b/gi)) {
    mismatch(match[1]?.toLowerCase() ?? '', respondentRole, 'the respondent')
  }
  for (const match of statement.matchAll(/\b(buyer|seller)\W+([1-9A-HJ-NP-Za-km-z]{32,44})\b/gi)) {
    const role = match[1]?.toLowerCase() as keyof typeof roles
    if (match[2] !== roles[role]) {
      flags.push({ kind: 'role', detail: `${match[2]} is not the ${role}` })
    }
  }

  return flags
}

export function judge(fixture: Fixture, report: FactFindingReport): Judgement {
  const universe = universeOf(fixture)
  const confirmed = report.facts
    .filter((fact) => fact.verdict === 'confirmed')
    .map((fact): JudgedFact => {
      const key = factKey(fact)
      const labelled = fixture.labels[key]
      return {
        key,
        statement: fact.statement,
        source: fact.sourceSignature ?? fact.sourceAccount ?? null,
        flags: flagsOf(
          {
            statement: fact.statement,
            ...(fact.sourceSignature ? { sourceSignature: fact.sourceSignature } : {}),
            ...(fact.sourceAccount ? { sourceAccount: fact.sourceAccount } : {}),
          },
          fixture,
          universe,
        ),
        label: labelled?.label ?? null,
        note: labelled?.note ?? '',
      }
    })

  const count = (label: Label) => confirmed.filter((fact) => fact.label === label).length
  return {
    id: fixture.id,
    confirmed,
    contradicted: count('contradicted'),
    unverifiable: count('unverifiable'),
    consistent: count('consistent'),
    unlabelled: confirmed.filter((fact) => fact.label === null).length,
    overruledSilently: confirmed.filter(
      (fact) =>
        fact.flags.length > 0 &&
        fact.label !== null &&
        fact.label !== 'contradicted' &&
        fact.note.trim() === '',
    ).length,
  }
}
