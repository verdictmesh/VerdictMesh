/**
 * Copies whatever `anchor build` generated into the repository tree.
 *
 * `target/` is not committed, and CI typechecks TypeScript without an on-chain
 * build, so the surface of the program has to live in the repo — in two forms,
 * because the consumers differ:
 *
 * - the **type** (`scripts/idl/*.ts`) for `Program<VerdictMesh>` in the runs.
 *   The IDL itself the run reads from `target/idl/` at launch: it cannot go
 *   anywhere without a fresh build anyway, and a type drifting from the actual
 *   surface of the program should be a red typecheck rather than a surprise on
 *   devnet.
 * - the **value** (`apps/api/src/idl/*.ts`) for `BorshCoder` in the watcher.
 *   The service on a host will never reach `target/`, so the IDL travels with
 *   it.
 *
 * Two forms are not two copies of one text: `target/types/*.ts` camel-cases
 * field names while `target/idl/*.json` leaves them in snake_case, and
 * `Program` converts the first from the second on its own. A value made out of
 * the type would not match what `BorshCoder` expects on input, so each form is
 * taken from its own file.
 */

import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'

const PROGRAMS = ['verdict_mesh', 'reference_escrow'] as const

/** Whose surface the off-chain service needs as a value, not just as a type. */
const RUNTIME: Partial<Record<(typeof PROGRAMS)[number], { path: string; name: string }>> = {
  verdict_mesh: { path: '../apps/api/src/idl/verdict-mesh.ts', name: 'verdictMeshIdl' },
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
  copyFileSync(
    new URL(`../target/types/${program}.ts`, import.meta.url),
    new URL(`./idl/${program}.ts`, import.meta.url),
  )

  const runtime = RUNTIME[program]
  if (!runtime) {
    console.log(`synced ${program} (type)`)
    continue
  }

  const source = new URL(`../target/idl/${program}.json`, import.meta.url)
  const target = new URL(runtime.path, import.meta.url)
  const idl: unknown = JSON.parse(readFileSync(source, 'utf8'))

  mkdirSync(new URL('.', target), { recursive: true })
  writeFileSync(
    target,
    `${header(program, runtime.name)}${JSON.stringify(idl, null, 2)} satisfies Idl\n`,
  )
  console.log(`synced ${program} (type + value)`)
}
