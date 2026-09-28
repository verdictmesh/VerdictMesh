import { createHash } from 'node:crypto'
import type Anthropic from '@anthropic-ai/sdk'
import { AnthropicError, APIConnectionError } from '@anthropic-ai/sdk'
import { PublicKey } from '@solana/web3.js'
import type { FactFindingReport } from '@verdictmesh/shared'
import { factFindingReport } from '@verdictmesh/shared'
import { describe, expect, it, vi } from 'vitest'
import { claimOf, referenceEscrowPositions } from './claims.js'
import type { EvidenceRow, EvidenceSet, KnownProgram } from './evidence.js'
import { referenceEscrowIdl } from './idl/reference-escrow.js'
import {
  anthropicReportModel,
  buildRequest,
  canonicalJson,
  checkReport,
  createReporter,
  type Generated,
  MalformedReport,
  needsReport,
  type Positions,
  REPORT_MODEL,
  type ReportRow,
  recoverPositions,
  reportHash,
  SYSTEM_PROMPT,
} from './reporter.js'
import type { DisputeRow } from './watcher.js'

const key = (fill: number) => new PublicKey(new Uint8Array(32).fill(fill)).toBase58()

const escrowProgram = key(200)
const escrowRef = key(10)
const pda = key(13)

/** Signatures are 64 bytes in base58 — the contract schema checks the length. */
const signature = (letter: string) => letter.repeat(88)
const opening = signature('A')
const creation = signature('B')
const stranger = signature('C')

const programs: KnownProgram[] = [
  {
    name: 'reference_escrow',
    programId: new PublicKey(escrowProgram),
    idl: referenceEscrowIdl,
    positions: referenceEscrowPositions,
  },
]

const dispute = (overrides: Partial<DisputeRow> = {}): DisputeRow => ({
  pda,
  integrator: key(15),
  escrowRef,
  claimant: key(12),
  respondent: key(11),
  amount: 18_446_744_073_709_551_615n,
  state: 'Committing',
  panel: [],
  reportHash: null,
  claimantClaimHash: claimOf(new PublicKey(escrowRef), 2, 'release'),
  respondentClaimHash: claimOf(new PublicKey(escrowRef), 2, 'refund'),
  openedAt: 1_700_000_100,
  commitDeadline: 1_700_000_160,
  revealDeadline: 1_700_000_220,
  appealDeadline: 0,
  votesClaimant: 0,
  votesRespondent: 0,
  escalated: false,
  verdict: null,
  syncedSlot: 500,
  ...overrides,
})

const transaction = (
  source: string,
  slot: number,
  blockTime: number | null,
  extra: Record<string, EvidenceRow['payload'][string]> = {},
): EvidenceRow => ({
  disputePda: pda,
  kind: 'transaction',
  source,
  slot,
  payload: {
    blockTime,
    signers: [key(12)],
    programs: [escrowProgram],
    events: [],
    logsTruncated: false,
    ...extra,
  },
})

const account: EvidenceRow = {
  disputePda: pda,
  kind: 'account',
  source: escrowRef,
  slot: 600,
  payload: {
    owner: escrowProgram,
    program: 'reference_escrow',
    account: 'Escrow',
    data: {},
    bytes: 250,
  },
}

/** Newest first, the way `collectEvidence` returns it. */
const evidence = (overrides: Partial<EvidenceSet> = {}): EvidenceSet => ({
  rows: [
    transaction(opening, 450, 1_700_000_100),
    transaction(creation, 300, 1_699_990_000),
    account,
  ],
  truncated: false,
  ...overrides,
})

const positions: Positions = recoverPositions(dispute(), escrowProgram, programs)

const generated = (overrides: Partial<FactFindingReport> = {}): FactFindingReport => ({
  summary: 'The seller says milestone #2 was delivered; the buyer disputes it.',
  timeline: [],
  facts: [],
  claims: [
    { party: 'claimant', statement: 'paraphrased by the model', assessment: 'unsupported' },
    { party: 'respondent', statement: 'paraphrased by the model', assessment: 'unsupported' },
  ],
  gaps: ['Whether the work of milestone #2 was delivered off chain.'],
  ...overrides,
})

describe('canonical JSON', () => {
  it('sorts keys at every depth and leaves no whitespace', () => {
    expect(canonicalJson({ b: [{ z: 1, a: 'x' }], a: null })).toBe(
      '{"a":null,"b":[{"a":"x","z":1}]}',
    )
  })

  /** An absent optional field is `undefined` in JS and absent in JSON. */
  it('leaves out undefined members', () => {
    expect(canonicalJson({ a: 1, b: undefined })).toBe('{"a":1}')
  })

  it('does not depend on the order the object was built in', () => {
    expect(canonicalJson({ a: 1, b: 2 })).toBe(canonicalJson({ b: 2, a: 1 }))
  })

  it('refuses what JCS would write differently from JSON.stringify', () => {
    expect(() => canonicalJson({ a: 0.5 })).toThrow(/safe integer/)
    expect(() => canonicalJson({ a: new Date(0) })).toThrow(/Cannot canonicalise/)
  })

  /**
   * An independent computation of the same fingerprint: the canonical text
   * written out by hand, hashed by hand. Anyone checking a report
   * (`FR-017b`) has to arrive at this exact value.
   */
  it('fingerprints the canonical bytes with sha256', () => {
    const report: FactFindingReport = {
      summary: 'ü',
      timeline: [{ at: 5, what: 'w', sourceSignature: opening }],
      facts: [],
      claims: [],
      gaps: [],
    }
    const text = `{"claims":[],"facts":[],"gaps":[],"summary":"ü","timeline":[{"at":5,"sourceSignature":"${opening}","what":"w"}]}`

    expect(reportHash(report)).toBe(createHash('sha256').update(text, 'utf8').digest('hex'))
  })
})

describe('positions of the parties', () => {
  it('are recovered through the formula of the program that owns the escrow', () => {
    expect(positions.claimant.statement).toMatch(/^Milestone #2 was delivered/)
    expect(positions.respondent.statement).toMatch(/^Milestone #2 was not delivered/)
  })

  it('stay fingerprints for an escrow whose program we do not know', () => {
    const foreign = recoverPositions(dispute(), key(202), programs)

    expect(foreign.claimant).toEqual({ fingerprint: dispute().claimantClaimHash, statement: null })
    expect(foreign.respondent.statement).toBeNull()
  })

  it('stay fingerprints when the escrow account is gone', () => {
    expect(recoverPositions(dispute(), null, programs).claimant.statement).toBeNull()
  })
})

describe('the request', () => {
  const request = buildRequest(dispute(), evidence(), positions)
  const input = JSON.parse(request.input)

  /** The cached prefix. A dispute field in it would make every request a miss. */
  it('keeps the system prompt free of anything about the dispute', () => {
    expect(request.system).toBe(SYSTEM_PROMPT)
    expect(request.system).not.toContain(pda)
  })

  it('lists evidence oldest first, as a timeline is written', () => {
    expect(input.evidence.items.map((item: { source: string }) => item.source)).toEqual([
      creation,
      opening,
      escrowRef,
    ])
  })

  it('keeps every digit of the largest amount', () => {
    expect(input.dispute.amount).toBe('18446744073709551615')
  })

  it('says whether the history was cut', () => {
    expect(
      JSON.parse(buildRequest(dispute(), evidence({ truncated: true }), positions).input).evidence
        .truncated,
    ).toBe(true)
  })

  it('hands over the positions with their fingerprints', () => {
    expect(input.positions.claimant.fingerprint).toBe(dispute().claimantClaimHash)
    expect(input.positions.claimant.statement).toMatch(/delivered/)
  })
})

describe('checking a report against the evidence', () => {
  const check = (report: FactFindingReport, set: EvidenceSet = evidence(), known = positions) =>
    checkReport(report, set, pda, known)

  it('keeps a confirmed fact that cites collected evidence', () => {
    const { report, demoted, stripped } = check(
      generated({
        facts: [
          { statement: 'A dispute was opened.', verdict: 'confirmed', sourceSignature: opening },
        ],
      }),
    )

    expect(report.facts).toEqual([
      { statement: 'A dispute was opened.', verdict: 'confirmed', sourceSignature: opening },
    ])
    expect(demoted).toBe(0)
    expect(stripped).toBe(0)
  })

  /** `SC-007`: a "confirmed" fact must stand on something the chain holds. */
  it('demotes a confirmed fact whose only source was not collected', () => {
    const { report, demoted, stripped } = check(
      generated({
        facts: [{ statement: 'Funds were paid.', verdict: 'confirmed', sourceSignature: stranger }],
      }),
    )

    expect(report.facts).toEqual([{ statement: 'Funds were paid.', verdict: 'unconfirmed' }])
    expect(demoted).toBe(1)
    expect(stripped).toBe(1)
  })

  it('demotes a contradicted statement without a source too', () => {
    const { report } = check(generated({ facts: [{ statement: 'x', verdict: 'contradicted' }] }))
    expect(report.facts[0]?.verdict).toBe('unconfirmed')
  })

  it('leaves an unconfirmed statement without a source alone', () => {
    const { report, demoted } = check(
      generated({ facts: [{ statement: 'x', verdict: 'unconfirmed' }] }),
    )

    expect(report.facts[0]).toEqual({ statement: 'x', verdict: 'unconfirmed' })
    expect(demoted).toBe(0)
  })

  it('does not accept an account that was not collected', () => {
    const { report } = check(
      generated({ facts: [{ statement: 'x', verdict: 'confirmed', sourceAccount: key(99) }] }),
    )
    expect(report.facts[0]?.verdict).toBe('unconfirmed')
  })

  it('keeps the good half of a reference and drops the invented half', () => {
    const { report, demoted, stripped } = check(
      generated({
        facts: [
          {
            statement: 'x',
            verdict: 'confirmed',
            sourceSignature: stranger,
            sourceAccount: escrowRef,
          },
        ],
      }),
    )

    expect(report.facts[0]).toEqual({
      statement: 'x',
      verdict: 'confirmed',
      sourceAccount: escrowRef,
    })
    expect(demoted).toBe(0)
    expect(stripped).toBe(1)
  })

  it('accepts the dispute account as a source of its own fields', () => {
    const { report } = check(
      generated({
        facts: [{ statement: 'The amount is locked.', verdict: 'confirmed', sourceAccount: pda }],
      }),
    )
    expect(report.facts[0]?.verdict).toBe('confirmed')
  })

  it('takes the time of a timeline entry from the chain, not from the model', () => {
    const { report } = check(
      generated({
        timeline: [
          { at: 1, what: 'Escrow created.', sourceSignature: creation },
          { at: 42, what: 'Something off chain.' },
        ],
      }),
    )

    expect(report.timeline).toEqual([
      { at: 1_699_990_000, what: 'Escrow created.', sourceSignature: creation },
      { at: 42, what: 'Something off chain.' },
    ])
  })

  it('keeps the time of the model when the node had no block time', () => {
    const set = evidence({ rows: [transaction(opening, 450, null), account] })
    const { report } = check(
      generated({ timeline: [{ at: 7, what: 'x', sourceSignature: opening }] }),
      set,
    )

    expect(report.timeline[0]?.at).toBe(7)
  })

  it('removes an invented source from the timeline', () => {
    const { report, stripped } = check(
      generated({ timeline: [{ at: 7, what: 'x', sourceSignature: stranger }] }),
    )

    expect(report.timeline[0]).toEqual({ at: 7, what: 'x' })
    expect(stripped).toBe(1)
  })

  /** The positions are the dispute's, whatever the model wrote. */
  it('shows the positions the dispute carries and keeps only the assessment', () => {
    const { report } = check(
      generated({
        claims: [
          { party: 'respondent', statement: 'invented', assessment: 'contradicted' },
          { party: 'claimant', statement: 'invented', assessment: 'supported' },
          { party: 'claimant', statement: 'a third one', assessment: 'contradicted' },
        ],
      }),
    )

    expect(report.claims).toEqual([
      { party: 'claimant', statement: positions.claimant.statement, assessment: 'supported' },
      {
        party: 'respondent',
        statement: positions.respondent.statement,
        assessment: 'contradicted',
      },
    ])
  })

  it('calls a party the model skipped unsupported', () => {
    const { report } = check(generated({ claims: [] }))
    expect(report.claims.map((claim) => claim.assessment)).toEqual(['unsupported', 'unsupported'])
  })

  it('shows an unrecoverable position by its fingerprint and names it as a gap', () => {
    const foreign = recoverPositions(dispute(), key(202), programs)
    const { report } = check(generated(), evidence(), foreign)

    expect(report.claims[0]?.statement).toContain(dispute().claimantClaimHash)
    expect(report.gaps).toContain(
      "The text of the claimant's position could not be recovered from its on-chain fingerprint.",
    )
  })

  it('puts the gaps the code knows about before the model’s', () => {
    const set = evidence({
      truncated: true,
      rows: [transaction(opening, 450, 1_700_000_100, { logsTruncated: true }), account],
    })
    const { report } = check(
      generated({ facts: [{ statement: 'x', verdict: 'confirmed', sourceSignature: stranger }] }),
      set,
    )

    expect(report.gaps).toEqual([
      expect.stringMatching(/^The transaction history of the escrow was cut/),
      expect.stringMatching(/^1 transaction\(s\) had their logs cut/),
      expect.stringMatching(
        /^1 reference\(s\) pointed at evidence that was not collected.*1 statement/,
      ),
      'Whether the work of milestone #2 was delivered off chain.',
    ])
  })

  it('stays within the contract when the model filled every gap slot', () => {
    const { report } = check(
      generated({ gaps: Array.from({ length: 20 }, (_, i) => `gap ${i}`) }),
      evidence({ truncated: true }),
    )

    expect(report.gaps).toHaveLength(20)
    expect(report.gaps[0]).toMatch(/was cut/)
    expect(factFindingReport.safeParse(report).success).toBe(true)
  })
})

describe('which disputes need a report', () => {
  /** A second before the commit window of `dispute()` closes. */
  const open = 1_700_000_159_000

  it('is a dispute waiting for commits without a fingerprint on chain', () => {
    expect(needsReport(dispute(), open)).toBe(true)
    expect(needsReport(dispute({ reportHash: 'a'.repeat(64) }), open)).toBe(false)
    for (const state of [
      'OptimisticPending',
      'Revealing',
      'Tallied',
      'Appealed',
      'Finalized',
    ] as const) {
      expect(needsReport(dispute({ state }), open)).toBe(false)
    }
  })

  /** The state moves only with the next transaction; the clock does not wait for it. */
  it('is not a dispute whose commit window has already closed', () => {
    expect(needsReport(dispute(), 1_700_000_160_000)).toBe(false)
  })
})

const silentLog = () => ({ info: vi.fn(), warn: vi.fn(), error: vi.fn() })

function harness(
  options: { has?: boolean; generate?: () => Promise<Generated>; maxAttempts?: number } = {},
) {
  const order: string[] = []
  const saved: ReportRow[] = []
  const log = silentLog()
  const collect = vi.fn(async () => {
    order.push('collect')
    return evidence()
  })
  const evidenceStore = {
    save: vi.fn(async () => {
      order.push('evidence')
    }),
  }
  const reports = {
    has: vi.fn(async () => options.has ?? false),
    save: vi.fn(async (row: ReportRow) => {
      order.push('report')
      saved.push(row)
    }),
  }
  const model = {
    generate: vi.fn(
      options.generate ??
        (async () => {
          order.push('model')
          return { report: generated(), model: 'claude-opus-5' }
        }),
    ),
  }
  let clock = 1_700_000_100_000
  const reporter = createReporter({
    collect,
    evidence: evidenceStore,
    reports,
    model,
    programs,
    log,
    maxAttempts: options.maxAttempts ?? 3,
    now: () => {
      clock += 4_000
      return clock
    },
  })

  return { reporter, order, saved, log, collect, model, reports }
}

describe('reporter', () => {
  it('collects, stores evidence, asks the model, then stores the report', async () => {
    const { reporter, order, saved } = harness()

    reporter.consider([dispute()])
    await reporter.idle()

    expect(order).toEqual(['collect', 'evidence', 'model', 'report'])
    expect(saved).toHaveLength(1)
    expect(saved[0]?.version).toBe(1)
    expect(saved[0]?.model).toBe('claude-opus-5')
  })

  it('stores the fingerprint of exactly the body it stores', async () => {
    const { reporter, saved } = harness()

    reporter.consider([dispute()])
    await reporter.idle()

    // biome-ignore lint/style/noNonNullAssertion: one report was stored above.
    const row = saved[0]!
    expect(row.contentHash).toBe(reportHash(row.content))
    expect(row.content.claims[0]?.statement).toBe(positions.claimant.statement)
  })

  it('records the model that served the request, which may be a fallback', async () => {
    const { reporter, saved } = harness({
      generate: async () => ({ report: generated(), model: 'claude-opus-4-8' }),
    })

    reporter.consider([dispute()])
    await reporter.idle()

    expect(saved[0]?.model).toBe('claude-opus-4-8')
  })

  it('measures the time since the dispute was opened', async () => {
    const { reporter, log } = harness()

    reporter.consider([dispute()])
    await reporter.idle()

    const [fields] = log.info.mock.calls.find(([, message]) => message === 'report generated') ?? []
    // Opened at 1_700_000_100 s, and the clock ticks 4 s per reading: once
    // when the dispute is offered, once when generation starts, once at the end.
    expect(fields).toMatchObject({ generationMs: 4_000, sinceOpenedMs: 12_000 })
  })

  it('takes up nothing that does not need a report', async () => {
    const { reporter, collect } = harness()

    reporter.consider([
      dispute({ state: 'Finalized', verdict: 'Claimant' }),
      dispute({ reportHash: 'a'.repeat(64) }),
    ])
    await reporter.idle()

    expect(collect).not.toHaveBeenCalled()
  })

  it('does not pay twice for a report that already exists', async () => {
    const { reporter, collect, model } = harness({ has: true })

    reporter.consider([dispute()])
    await reporter.idle()

    expect(collect).not.toHaveBeenCalled()
    expect(model.generate).not.toHaveBeenCalled()
  })

  /** The event and a rewrite meet on a fresh dispute every time. */
  it('generates once for a dispute offered twice while it is in flight', async () => {
    const { reporter, model } = harness()

    reporter.consider([dispute(), dispute()])
    reporter.consider([dispute()])
    await reporter.idle()

    expect(model.generate).toHaveBeenCalledTimes(1)
  })

  it('logs a failure instead of throwing, and tries again on the next offer', async () => {
    const generate = vi
      .fn<() => Promise<Generated>>()
      .mockRejectedValueOnce(new Error('overloaded'))
      .mockResolvedValue({ report: generated(), model: 'claude-opus-5' })
    const { reporter, saved, log } = harness({ generate })

    reporter.consider([dispute()])
    await reporter.idle()
    expect(saved).toHaveLength(0)
    expect(log.error).toHaveBeenCalledWith(
      expect.objectContaining({ dispute: pda, attempt: 1 }),
      'report generation failed',
    )

    reporter.consider([dispute()])
    await reporter.idle()
    expect(saved).toHaveLength(1)
  })

  /** A late report is read; a missing one is not. */
  it('asks again at once when the answer was not a report', async () => {
    const generate = vi
      .fn<() => Promise<Generated>>()
      .mockRejectedValueOnce(new MalformedReport('Unterminated string'))
      .mockResolvedValue({ report: generated(), model: 'claude-opus-5' })
    const { reporter, saved, log } = harness({ generate })

    reporter.consider([dispute()])
    await reporter.idle()

    expect(generate).toHaveBeenCalledTimes(2)
    expect(saved).toHaveLength(1)
    expect(log.warn).toHaveBeenCalledWith(
      expect.objectContaining({ dispute: pda }),
      'malformed report, asking again',
    )
  })

  it('asks again only once', async () => {
    const generate = vi
      .fn<() => Promise<Generated>>()
      .mockRejectedValue(new MalformedReport('Unterminated string'))
    const { reporter, saved, log } = harness({ generate })

    reporter.consider([dispute()])
    await reporter.idle()

    expect(generate).toHaveBeenCalledTimes(2)
    expect(saved).toHaveLength(0)
    expect(log.error).toHaveBeenCalledWith(
      expect.objectContaining({ attempt: 1 }),
      'report generation failed',
    )
  })

  /** The SDK has already retried an API error; the fallback, a refusal. */
  it('does not ask again at once after any other failure', async () => {
    const generate = vi.fn<() => Promise<Generated>>().mockRejectedValue(new Error('overloaded'))
    const { reporter } = harness({ generate })

    reporter.consider([dispute()])
    await reporter.idle()

    expect(generate).toHaveBeenCalledTimes(1)
  })

  it('gives up on a dispute after the last allowed attempt', async () => {
    const generate = vi.fn<() => Promise<Generated>>().mockRejectedValue(new Error('refused'))
    const { reporter } = harness({ generate, maxAttempts: 2 })

    for (let offer = 0; offer < 4; offer += 1) {
      reporter.consider([dispute()])
      await reporter.idle()
    }

    expect(generate).toHaveBeenCalledTimes(2)
  })
})

/** A client that records the request and answers with the given message. */
function fakeClient(message: Record<string, unknown>) {
  const stream = vi.fn(() => ({ finalMessage: async () => message }))
  const client = { beta: { messages: { stream } } } as unknown as Anthropic
  return { client, stream }
}

const request = buildRequest(dispute(), evidence(), positions)

describe('the Anthropic model', () => {
  it('asks claude-opus-5 for the contract schema, with fallbacks and a cached prefix', async () => {
    const { client, stream } = fakeClient({
      stop_reason: 'end_turn',
      parsed_output: generated(),
      model: REPORT_MODEL,
    })

    await anthropicReportModel(client).generate(request)

    expect(stream).toHaveBeenCalledWith(
      expect.objectContaining({
        model: 'claude-opus-5',
        fallbacks: 'default',
        betas: ['server-side-fallback-2026-07-01'],
        thinking: { type: 'adaptive' },
        output_config: expect.objectContaining({
          effort: 'low',
          format: expect.objectContaining({ type: 'json_schema' }),
        }),
        system: [{ type: 'text', text: SYSTEM_PROMPT, cache_control: { type: 'ephemeral' } }],
        messages: [{ role: 'user', content: request.input }],
      }),
    )
  })

  it('returns the parsed report and the model that served it', async () => {
    const { client } = fakeClient({
      stop_reason: 'end_turn',
      parsed_output: generated(),
      model: 'claude-opus-4-8',
    })

    await expect(anthropicReportModel(client).generate(request)).resolves.toEqual({
      report: generated(),
      model: 'claude-opus-4-8',
    })
  })

  it('keeps the output ceiling low enough to cut a runaway string short', async () => {
    const { client, stream } = fakeClient({
      stop_reason: 'end_turn',
      parsed_output: generated(),
      model: REPORT_MODEL,
    })

    await anthropicReportModel(client).generate(request)

    expect(stream).toHaveBeenCalledWith(expect.objectContaining({ max_tokens: 4_096 }))
  })

  it('calls an output the SDK could not parse malformed', async () => {
    const stream = vi.fn(() => ({
      finalMessage: async () => {
        throw new AnthropicError('Failed to parse structured output: Unterminated string')
      },
    }))
    const client = { beta: { messages: { stream } } } as unknown as Anthropic

    await expect(anthropicReportModel(client).generate(request)).rejects.toBeInstanceOf(
      MalformedReport,
    )
  })

  it('leaves an error on the wire as it is', async () => {
    const stream = vi.fn(() => ({
      finalMessage: async () => {
        throw new APIConnectionError({ message: 'socket hang up' })
      },
    }))
    const client = { beta: { messages: { stream } } } as unknown as Anthropic

    const failure = anthropicReportModel(client).generate(request)
    await expect(failure).rejects.toBeInstanceOf(APIConnectionError)
    await expect(failure).rejects.not.toBeInstanceOf(MalformedReport)
  })

  it('fails on a refusal instead of storing an empty report', async () => {
    const { client } = fakeClient({
      stop_reason: 'refusal',
      stop_details: { category: 'cyber' },
      parsed_output: null,
    })

    await expect(anthropicReportModel(client).generate(request)).rejects.toThrow(/declined.*cyber/)
  })

  it('fails on a report cut off at max_tokens', async () => {
    const { client } = fakeClient({ stop_reason: 'max_tokens', parsed_output: null })

    const failure = anthropicReportModel(client).generate(request)
    await expect(failure).rejects.toThrow(/max_tokens/)
    await expect(failure).rejects.toBeInstanceOf(MalformedReport)
  })

  it('fails when nothing could be parsed', async () => {
    const { client } = fakeClient({ stop_reason: 'end_turn', parsed_output: null })

    await expect(anthropicReportModel(client).generate(request)).rejects.toBeInstanceOf(
      MalformedReport,
    )
  })
})
