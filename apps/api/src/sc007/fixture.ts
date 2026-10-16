import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import type { FactFindingReport } from '@verdictmesh/shared'
import type { EvidenceSet } from '../evidence.js'
import type { FixtureDispute, Truth } from './scenario.js'

/**
 * One `SC-007` case on disk, `fixtures/disputes/<id>/`:
 *
 * - `dispute.json`, `evidence.json` — what the reporter is given;
 * - `truth.json` — the answer key, for the person labelling;
 * - `report.json` — the report of the measured run, after `checkReport`, with
 *   `generated` (what the model returned) beside it;
 * - `labels.json` — the human verdict on every confirmed fact of `report.json`.
 */

export const FIXTURES = fileURLToPath(new URL('../../../../fixtures/disputes/', import.meta.url))

export type Label = 'consistent' | 'contradicted' | 'unverifiable'

export interface LabelEntry {
  statement: string
  label: Label
  /** Why — required whenever a flag of the judge is overruled. */
  note: string
}

/** Keyed by `factKey`. */
export type Labels = Record<string, LabelEntry>

export interface MeasuredReport {
  model: string
  generationMs: number
  demoted: number
  stripped: number
  /** Copies that failed on their own before one succeeded. */
  failures: string[]
  generated: FactFindingReport
  report: FactFindingReport
}

export interface Fixture {
  id: string
  dir: string
  dispute: FixtureDispute
  evidence: EvidenceSet
  truth: Truth
  report: MeasuredReport | null
  labels: Labels
}

/** LF and a trailing newline, whatever the platform. */
export function writeJson(path: string, value: unknown): void {
  writeFileSync(path, Buffer.from(`${JSON.stringify(value, null, 2)}\n`, 'utf8'))
}

const readJson = <T>(path: string): T => JSON.parse(readFileSync(path, 'utf8')) as T
const optional = <T>(path: string): T | null => (existsSync(path) ? readJson<T>(path) : null)

export function fixtureIds(): string[] {
  return readdirSync(FIXTURES, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort()
}

export function loadFixture(id: string): Fixture {
  const dir = `${FIXTURES}${id}/`
  return {
    id,
    dir,
    dispute: readJson(`${dir}dispute.json`),
    evidence: readJson(`${dir}evidence.json`),
    truth: readJson(`${dir}truth.json`),
    report: optional(`${dir}report.json`),
    labels: optional(`${dir}labels.json`) ?? {},
  }
}

export function saveInputs(
  id: string,
  inputs: { dispute: FixtureDispute; evidence: EvidenceSet; truth: Truth },
): void {
  const dir = `${FIXTURES}${id}/`
  mkdirSync(dir, { recursive: true })
  writeJson(`${dir}dispute.json`, inputs.dispute)
  writeJson(`${dir}evidence.json`, inputs.evidence)
  writeJson(`${dir}truth.json`, inputs.truth)
}

/** A fact is labelled once; the key survives re-ordering of the report. */
export function factKey(fact: {
  statement: string
  sourceSignature?: string | undefined
  sourceAccount?: string | undefined
}): string {
  return createHash('sha256')
    .update(`${fact.statement}\n${fact.sourceSignature ?? ''}\n${fact.sourceAccount ?? ''}`)
    .digest('hex')
    .slice(0, 16)
}
