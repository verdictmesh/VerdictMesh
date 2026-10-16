/**
 * Judges the measured reports and prints what a person needs to label them:
 * every confirmed fact with its source, the judge's flags and the label so far.
 * Writes the totals to `fixtures/disputes/sc007.json`, which `sc007.test.ts`
 * holds the stored labels to.
 *
 *   pnpm --filter @verdictmesh/api sc007:judge [--unlabelled]
 */
import { parseArgs } from 'node:util'
import { FIXTURES, fixtureIds, loadFixture, writeJson } from './fixture.js'
import { judge } from './judge.js'

const { values } = parseArgs({
  options: {
    unlabelled: { type: 'boolean', default: false },
    partial: { type: 'boolean', default: false },
  },
})

const judgements = fixtureIds()
  .map((id) => {
    const fixture = loadFixture(id)
    if (!fixture.report) {
      if (values.partial) return null
      throw new Error(`${id} has no report.json — run sc007:run first`)
    }
    return { fixture, judgement: judge(fixture, fixture.report.report) }
  })
  .filter((entry) => entry !== null)

for (const { fixture, judgement } of judgements) {
  const facts = judgement.confirmed.filter((fact) => !values.unlabelled || fact.label === null)
  if (facts.length === 0) continue
  console.log(`\n== ${fixture.id} — ${fixture.truth.title}`)
  for (const fact of facts) {
    console.log(`  [${fact.label ?? '?'}] ${fact.key} ${fact.statement}`)
    console.log(`      source ${fact.source}`)
    for (const flag of fact.flags) console.log(`      ! ${flag.kind}: ${flag.detail}`)
    if (fact.note) console.log(`      note: ${fact.note}`)
  }
}

const sum = (pick: (judgement: (typeof judgements)[number]['judgement']) => number) =>
  judgements.reduce((total, { judgement }) => total + pick(judgement), 0)
const reports = judgements.map(({ fixture }) => fixture.report)

const totals = {
  disputes: judgements.length,
  confirmed: sum((judgement) => judgement.confirmed.length),
  contradicted: sum((judgement) => judgement.contradicted),
  unverifiable: sum((judgement) => judgement.unverifiable),
  consistent: sum((judgement) => judgement.consistent),
  unlabelled: sum((judgement) => judgement.unlabelled),
  flagged: sum((judgement) => judgement.confirmed.filter((fact) => fact.flags.length > 0).length),
  demoted: reports.reduce((total, report) => total + (report?.demoted ?? 0), 0),
  stripped: reports.reduce((total, report) => total + (report?.stripped ?? 0), 0),
  failedCopies: reports.reduce((total, report) => total + (report?.failures.length ?? 0), 0),
  models: [...new Set(reports.map((report) => report?.model))],
  contradictedIn: judgements
    .filter(({ judgement }) => judgement.contradicted > 0)
    .map(({ fixture }) => fixture.id),
}

console.log('\n', totals)
if (!values.partial && totals.unlabelled === 0) writeJson(`${FIXTURES}sc007.json`, totals)
