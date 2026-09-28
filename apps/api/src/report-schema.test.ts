import { betaZodOutputFormat } from '@anthropic-ai/sdk/helpers/beta/zod'
import { factFindingReport } from '@verdictmesh/shared'
import { describe, expect, it } from 'vitest'
import { parseReport, reportJsonSchema, strictSchema } from './report-schema.js'

type Node = { [key: string]: unknown }

/** Follows a path of property names and `items` through the schema. */
const at = (...path: string[]): Node =>
  path.reduce<Node>((node, step) => {
    const next = step === 'items' ? node.items : (node.properties as Node)[step]
    return next as Node
  }, reportJsonSchema)

const valid = {
  summary: 's',
  timeline: [],
  facts: [{ statement: 'x', verdict: 'confirmed' }],
  claims: [],
  gaps: [],
}

describe('the report schema the grammar sees', () => {
  /** The reason the schema is ours and not the SDK helper's. */
  it('keeps every enum of the contract', () => {
    expect(at('facts', 'items', 'verdict').enum).toEqual([
      'confirmed',
      'unconfirmed',
      'contradicted',
    ])
    expect(at('claims', 'items', 'party').enum).toEqual(['claimant', 'respondent'])
    expect(at('claims', 'items', 'assessment').enum).toEqual([
      'supported',
      'unsupported',
      'contradicted',
    ])
  })

  /**
   * A control: the helper this module replaces loses exactly that. If an SDK
   * update ever keeps enums, this test says the module can go.
   */
  it('keeps what the SDK helper drops', () => {
    const helper = JSON.stringify(betaZodOutputFormat(factFindingReport).schema)
    expect(helper).not.toContain('"enum"')
    expect(JSON.stringify(reportJsonSchema)).toContain('"enum"')
  })

  it('moves limits the grammar cannot hold into the description', () => {
    const statement = at('facts', 'items', 'statement')

    expect(statement).not.toHaveProperty('maxLength')
    expect(statement.description).toBe('{minLength: 1, maxLength: 500}')
    expect(at('gaps')).not.toHaveProperty('maxItems')
  })

  it('closes every object and requires what the contract requires', () => {
    expect(reportJsonSchema.additionalProperties).toBe(false)
    expect(at('facts', 'items').additionalProperties).toBe(false)
    expect(reportJsonSchema.required).toEqual(['summary', 'timeline', 'facts', 'claims', 'gaps'])
    expect(at('facts', 'items').required).toEqual(['statement', 'verdict'])
  })

  /** zod closes objects today; the grammar must not depend on it doing so tomorrow. */
  it('closes an object even when the source schema left it open', () => {
    expect(strictSchema({ type: 'object', properties: { a: { type: 'string' } } })).toEqual({
      type: 'object',
      properties: { a: { type: 'string' } },
      additionalProperties: false,
    })
  })

  it('carries no $schema', () => {
    expect(reportJsonSchema).not.toHaveProperty('$schema')
  })

  it('adds to a description rather than replacing it', () => {
    expect(strictSchema({ type: 'string', description: 'A name.', maxLength: 3 })).toEqual({
      type: 'string',
      description: 'A name.\n\n{maxLength: 3}',
    })
  })
})

describe('parsing an answer', () => {
  it('accepts a report that keeps the contract', () => {
    expect(parseReport(JSON.stringify(valid))).toEqual({ ok: true, report: valid })
  })

  it('refuses text that is not JSON', () => {
    const parsed = parseReport('{"summary":"never closed')
    expect(parsed.ok).toBe(false)
    expect(parsed.ok ? '' : parsed.reason).toMatch(/^not JSON/)
  })

  it('refuses a value outside an enum', () => {
    const parsed = parseReport(
      JSON.stringify({ ...valid, facts: [{ statement: 'x', verdict: 'likely' }] }),
    )
    expect(parsed.ok ? '' : parsed.reason).toMatch(/^breaks the contract/)
  })

  /** Lengths are only hints to the grammar, so they are enforced here. */
  it('refuses a string over its limit', () => {
    const parsed = parseReport(JSON.stringify({ ...valid, gaps: ['x'.repeat(501)] }))
    expect(parsed.ok).toBe(false)
  })
})
