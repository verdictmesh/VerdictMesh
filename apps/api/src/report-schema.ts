import { type FactFindingReport, factFindingReport } from '@verdictmesh/shared'
import { z } from 'zod'

/**
 * The output schema of the report as the model's grammar sees it.
 *
 * **Why not `betaZodOutputFormat`.** The SDK helper keeps `type`,
 * `properties`, `required`, `items` and `format`, and moves everything else —
 * `enum` included — into `description`. The grammar then allows any string
 * where the contract allows three, and the contract is checked only after the
 * answer: on devnet one answer in twenty put a `verdict` outside the enum and
 * came back as a failure (`docs/TASKS.md` → T029). Structured outputs do
 * support `enum`; the helper simply does not pass it on.
 *
 * So the schema is built from the same contract (`packages/shared`), keeping
 * what the grammar enforces and moving only what it cannot — string lengths,
 * item counts, number bounds — into `description`, where the model at least
 * reads it. Those limits are still enforced by `parseReport` after the answer.
 */

type JsonSchema = { [key: string]: unknown }

/** Keywords the grammar enforces. Everything else becomes a hint. */
const ENFORCED = new Set([
  'type',
  'properties',
  'required',
  'items',
  'enum',
  'const',
  'anyOf',
  'additionalProperties',
  'description',
])

const isSchema = (value: unknown): value is JsonSchema =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

/** One schema node: enforced keywords stay, the rest is written out as a hint. */
export function strictSchema(node: JsonSchema): JsonSchema {
  const strict: JsonSchema = {}
  const hints: string[] = []

  for (const [key, value] of Object.entries(node)) {
    if (key === '$schema') continue
    if (!ENFORCED.has(key)) {
      hints.push(`${key}: ${JSON.stringify(value)}`)
      continue
    }
    if (key === 'properties' && isSchema(value)) {
      strict.properties = Object.fromEntries(
        Object.entries(value).map(([name, inner]) => [
          name,
          isSchema(inner) ? strictSchema(inner) : inner,
        ]),
      )
    } else if (key === 'items' && isSchema(value)) {
      strict.items = strictSchema(value)
    } else if (key === 'anyOf' && Array.isArray(value)) {
      strict.anyOf = value.map((inner) => (isSchema(inner) ? strictSchema(inner) : inner))
    } else {
      strict[key] = value
    }
  }

  if (strict.type === 'object') strict.additionalProperties = false
  if (hints.length > 0) {
    const described = typeof strict.description === 'string' ? `${strict.description}\n\n` : ''
    strict.description = `${described}{${hints.join(', ')}}`
  }
  return strict
}

export const reportJsonSchema: JsonSchema = strictSchema(z.toJSONSchema(factFindingReport))

/** Why an answer is not a report, with the text it came as. */
export type Parsed = { ok: true; report: FactFindingReport } | { ok: false; reason: string }

/** The answer text checked against the whole contract, limits included. */
export function parseReport(text: string): Parsed {
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch (error: unknown) {
    return { ok: false, reason: `not JSON: ${String(error)}` }
  }

  const parsed = factFindingReport.safeParse(value)
  if (!parsed.success) {
    return { ok: false, reason: `breaks the contract: ${z.prettifyError(parsed.error)}` }
  }
  return { ok: true, report: parsed.data }
}
