/**
 * Writes the modelled `SC-007` cases into `fixtures/disputes/`. Deterministic:
 * a second run writes the same bytes, and `sc007.test.ts` holds it to that, so
 * the files on disk are always what the scenarios describe.
 *
 *   pnpm --filter @verdictmesh/api sc007:build
 */
import { referenceEscrowPositions } from '../claims.js'
import { saveInputs } from './fixture.js'
import { build, knownPrograms } from './scenario.js'
import { scenarios } from './scenarios.js'

const programs = knownPrograms(referenceEscrowPositions)

for (const scenario of scenarios) {
  const built = await build(scenario, programs)
  saveInputs(scenario.id, built)
  const { collected } = built.truth
  console.log(
    `${scenario.id}: ${collected.transactions} transaction(s)${collected.truncated ? ', history cut' : ''}${collected.logsCut ? `, ${collected.logsCut} log(s) cut` : ''}`,
  )
}
