/**
 * Copies whatever `anchor build` generated into the repository tree.
 *
 * `target/` is not committed, and CI typechecks TypeScript without an on-chain
 * build, so the surface of the program has to live in the repo — in two forms,
 * because the consumers differ:
 *
 * - the **type** for `Program<…>`: `packages/sdk/src/idl/` for `verdict_mesh`,
 *   `scripts/idl/` for `reference_escrow`.
 *   The IDL itself the run reads from `target/idl/` at launch: it cannot go
 *   anywhere without a fresh build anyway, and a type drifting from the actual
 *   surface of the program should be a red typecheck rather than a surprise on
 *   devnet.
 * - the **value** for `BorshCoder` in the watcher and the evidence collector
 *   (and for `Program` in the SDK): `packages/sdk/src/idl/` for `verdict_mesh`,
 *   `apps/api/src/idl/` for `reference_escrow`. The service on a host will
 *   never reach `target/`, so the IDL travels with it. The collector decodes
 *   the escrow's events and account as well, hence `reference_escrow` too.
 *
 * Two forms are not two copies of one text: `target/types/*.ts` camel-cases
 * field names while `target/idl/*.json` leaves them in snake_case, and
 * `Program` converts the first from the second on its own. A value made out of
 * the type would not match what `BorshCoder` expects on input, so each form is
 * taken from its own file.
 */

import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'

const PROGRAMS = ['verdict_mesh', 'reference_escrow'] as const

/**
 * Where each surface goes. `verdict_mesh` belongs to the SDK: it is what an
 * integrator imports, and the service and the runs take it from there instead
 * of keeping copies of their own. `reference_escrow` is ours alone — the
 * evidence collector decodes it, the runs drive it.
 */
const TARGETS: Record<(typeof PROGRAMS)[number], { type: string; value: string; name: string }> = {
  verdict_mesh: {
    type: '../packages/sdk/src/idl/verdict_mesh.ts',
    value: '../packages/sdk/src/idl/verdict-mesh.ts',
    name: 'verdictMeshIdl',
  },
  reference_escrow: {
    type: './idl/reference_escrow.ts',
    value: '../apps/api/src/idl/reference-escrow.ts',
    name: 'referenceEscrowIdl',
  },
}

const header = (program: string, name: string) => `/**
 * The IDL of the \`${program}\` program as a **value**: \`BorshCoder\` decodes
 * both accounts and events with it, and outside \`target/\` there is nowhere
 * else to get it.
 *
 * Generated from \`target/idl/${program}.json\` by \`sync-idl\` in \`scripts/\`
 * — editing it by hand is pointless, the next build overwrites it anyway. Field
 * names here are snake_case, the way \`anchor build\` leaves them; \`Program\`
 * camel-cases them, \`BorshCoder\` does not, and this is the form it has to be
 * fed.
 */

import type { Idl } from '@coral-xyz/anchor'

export const ${name} = `

for (const program of PROGRAMS) {
  const target = TARGETS[program]
  const type = new URL(target.type, import.meta.url)
  const value = new URL(target.value, import.meta.url)
  mkdirSync(new URL('.', type), { recursive: true })

  copyFileSync(new URL(`../target/types/${program}.ts`, import.meta.url), type)

  const idl: unknown = JSON.parse(
    readFileSync(new URL(`../target/idl/${program}.json`, import.meta.url), 'utf8'),
  )
  writeFileSync(
    value,
    `${header(program, target.name)}${JSON.stringify(idl, null, 2)} satisfies Idl
`,
  )
  console.log(`synced ${program} (type + value)`)
}
