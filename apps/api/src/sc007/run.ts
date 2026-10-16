/**
 * The measured `SC-007` run: the reporter's own path — `recoverPositions`,
 * `buildRequest`, two copies through `firstReport` with one retry when every
 * copy came back malformed, then `checkReport` — over each fixture, with the
 * production model and settings. Only the chain and the stores are left out.
 *
 * Writes `report.json` next to each fixture and skips a fixture that has one,
 * so an interrupted run resumes; `--force` regenerates.
 *
 *   pnpm --filter @verdictmesh/api sc007:run [--force] [id…]
 */
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import Anthropic from '@anthropic-ai/sdk'
import { referenceEscrowPositions } from '../claims.js'
import {
  anthropicReportModel,
  buildRequest,
  checkReport,
  escrowOwner,
  firstReport,
  MalformedReport,
  recoverPositions,
} from '../reporter.js'
import { fixtureIds, loadFixture, type MeasuredReport, writeJson } from './fixture.js'
import { knownPrograms } from './scenario.js'

const { values, positionals } = parseArgs({
  options: {
    force: { type: 'boolean', default: false },
    concurrency: { type: 'string', default: '3' },
    // Connection-level retries of the SDK. Production uses one; raising it
    // here only rides out a flaky network and does not touch what is measured.
    retries: { type: 'string', default: '1' },
  },
  allowPositionals: true,
})

const envFile = fileURLToPath(new URL('../../../../.env', import.meta.url))
const fromFile = Object.fromEntries(
  readFileSync(envFile, 'utf8')
    .split(/\r?\n/)
    .filter((line) => /^[A-Z_0-9]+=/.test(line))
    .map((line) => [line.slice(0, line.indexOf('=')), line.slice(line.indexOf('=') + 1)]),
)
const apiKey = process.env.ANTHROPIC_API_KEY ?? fromFile.ANTHROPIC_API_KEY
if (!apiKey) throw new Error('ANTHROPIC_API_KEY is not set')

// As in `index.ts`: one retry and 90 s instead of two and ten minutes.
const model = anthropicReportModel(
  new Anthropic({ apiKey, maxRetries: Number(values.retries), timeout: 90_000 }),
)
const programs = knownPrograms(referenceEscrowPositions)
const COPIES = 2

const ids = (positionals.length > 0 ? positionals : fixtureIds()).filter((id) => {
  const fixture = loadFixture(id)
  return values.force || fixture.report === null
})

async function measure(id: string): Promise<void> {
  const fixture = loadFixture(id)
  const dispute = { ...fixture.dispute, amount: BigInt(fixture.dispute.amount) }
  const positions = recoverPositions(dispute, escrowOwner(fixture.evidence), programs)
  const request = buildRequest(dispute, fixture.evidence, positions)

  const failures: string[] = []
  const failed = (error: unknown, copy: number) => {
    const reason = error instanceof Error ? `${error.name}: ${error.message}` : String(error)
    failures.push(`copy ${copy}: ${reason}`)
  }

  const started = performance.now()
  const generated = await firstReport(model, request, COPIES, failed).catch((error: unknown) => {
    if (!(error instanceof MalformedReport)) throw error
    return firstReport(model, request, COPIES, failed)
  })
  const generationMs = Math.round(performance.now() - started)
  const { report, demoted, stripped } = checkReport(
    generated.report,
    fixture.evidence,
    dispute.pda,
    positions,
  )

  const measured: MeasuredReport = {
    model: generated.model,
    generationMs,
    demoted,
    stripped,
    failures,
    generated: generated.report,
    report,
  }
  writeJson(`${fixture.dir}report.json`, measured)

  const confirmed = report.facts.filter((fact) => fact.verdict === 'confirmed').length
  console.log(
    `${id}: ${(generationMs / 1000).toFixed(1)} s, ${generated.model}, ${confirmed} confirmed, demoted ${demoted}, stripped ${stripped}${failures.length ? `, ${failures.length} failed copy(ies)` : ''}`,
  )
}

const queue = [...ids]
const failedIds: string[] = []
await Promise.all(
  Array.from({ length: Number(values.concurrency) }, async () => {
    for (let id = queue.shift(); id !== undefined; id = queue.shift()) {
      await measure(id).catch((error: unknown) => {
        failedIds.push(id)
        const causes: string[] = []
        for (let cause: unknown = error; cause instanceof Error; cause = cause.cause) {
          causes.push(`${cause.name}: ${cause.message}`)
        }
        console.error(`${id}: FAILED — ${causes.join(' ← ') || String(error)}`)
      })
    }
  }),
)

console.log(`${ids.length - failedIds.length} of ${ids.length} measured`)
process.exitCode = failedIds.length > 0 ? 1 : 0
