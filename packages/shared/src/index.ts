import { z } from 'zod'

/**
 * Одна схема працює тричі: валідація входу Hono, типи фронту і схема
 * структурованого виводу для моделі. Розійтися звіту з контрактом ніде.
 */

export const evidenceRef = z.object({
  sourceSignature: z.string().min(64).max(128).optional(),
  sourceAccount: z.string().min(32).max(44).optional(),
})

export const timelineEntry = evidenceRef.extend({
  at: z.number().int(),
  what: z.string().min(1).max(500),
})

export const factAssessment = z.enum(['confirmed', 'unconfirmed', 'contradicted'])

export const fact = evidenceRef.extend({
  statement: z.string().min(1).max(500),
  verdict: factAssessment,
})

export const claim = z.object({
  party: z.enum(['claimant', 'respondent']),
  statement: z.string().min(1).max(1000),
  assessment: z.enum(['supported', 'unsupported', 'contradicted']),
})

export const factFindingReport = z.object({
  summary: z.string().min(1).max(2000),
  timeline: z.array(timelineEntry).max(100),
  facts: z.array(fact).max(100),
  claims: z.array(claim).max(20),
  /**
   * Чого бракує, щоб вирішити спір. Порожній масив — сильне твердження, тому
   * поле обов'язкове: модель має сказати «фактів недостатньо» вголос, а не
   * заповнити прогалину здогадкою (SPEC.md → Припущення).
   */
  gaps: z.array(z.string().max(500)).max(20),
})

export const disputeState = z.enum([
  'OptimisticPending',
  'Committing',
  'Revealing',
  'Tallied',
  'Appealed',
  'Finalized',
])

export const verdict = z.enum(['Claimant', 'Respondent', 'StatusQuo'])

/**
 * Whether the escrow has carried out the verdict (`FR-020`). The escrow pulls
 * the verdict and moves the funds itself, so this is read off the escrow's own
 * transaction, never asserted by us:
 *
 * - `settled` — the escrow's settlement transaction, to link to.
 * - `awaiting` — an escrow we can read has not settled (yet).
 * - `untracked` — the integrator's escrow is a program whose events we cannot
 *   decode. Saying `awaiting` here would claim a silence we never listened to.
 */
export const settlement = z.discriminatedUnion('status', [
  z.object({
    status: z.literal('settled'),
    signature: z.string().min(64).max(128),
    slot: z.number().int().nonnegative(),
  }),
  z.object({ status: z.literal('awaiting') }),
  z.object({ status: z.literal('untracked') }),
])

export const disputeView = z.object({
  pda: z.string().min(32).max(44),
  integrator: z.string().min(32).max(44),
  escrowRef: z.string().min(32).max(44),
  claimant: z.string().min(32).max(44),
  respondent: z.string().min(32).max(44),
  amount: z.string(),
  state: disputeState,
  panel: z.array(z.string().min(32).max(44)),
  reportHash: z.string().length(64).nullable(),
  openedAt: z.number().int(),
  commitDeadline: z.number().int(),
  revealDeadline: z.number().int(),
  appealDeadline: z.number().int(),
  /** Revealed votes, across both rounds: escalation does not reset them. */
  votesClaimant: z.number().int().nonnegative(),
  votesRespondent: z.number().int().nonnegative(),
  escalated: z.boolean(),
  verdict: verdict.nullable(),
  settlement,
})

/**
 * Why a dispute has no report to show (`FR-018`). The first two are what the
 * reporter knows right now and may still change; the rest follow from the
 * dispute's state on chain and never will:
 *
 * - `model_unavailable` — the model is down; the reporter keeps trying.
 * - `generation_failed` — the answers were not reports, and the attempts ran out.
 * - `window_closed` — the commit window closed with no fingerprint on chain.
 * - `escalated` — the second round opened with no fingerprint on chain.
 * - `body_missing` — a fingerprint is on chain, but its body is not stored.
 */
export const reportUnavailableReason = z.enum([
  'model_unavailable',
  'generation_failed',
  'window_closed',
  'escalated',
  'body_missing',
])

/**
 * `GET /disputes/:pda/report`, by `status`.
 *
 * `ready` (`FR-017b`): `hash` is computed from the very body carried in
 * `report`, not taken from the database — a client that hashes the canonical
 * JSON of `report` gets exactly this value. `matchesOnchain` compares it with
 * the `report_hash` read from the dispute account at request time.
 *
 * `pending`: the report is due and nothing is known against it yet.
 * `unavailable` (`FR-018`): the dispute goes on without it; `final` — it will
 * not come at all.
 */
export const reportResponse = z.discriminatedUnion('status', [
  z.object({
    status: z.literal('ready'),
    report: factFindingReport,
    hash: z.string().regex(/^[0-9a-f]{64}$/),
    matchesOnchain: z.boolean(),
  }),
  z.object({ status: z.literal('pending') }),
  z.object({
    status: z.literal('unavailable'),
    reason: reportUnavailableReason,
    final: z.boolean(),
  }),
])

export const apiErrorCode = z.enum([
  'INVALID_INPUT',
  'UNAUTHORIZED',
  'NOT_FOUND',
  'RATE_LIMITED',
  'INTERNAL',
])

export const apiError = z.object({
  error: z.object({
    code: apiErrorCode,
    message: z.string(),
    details: z.record(z.string(), z.unknown()).optional(),
  }),
})

export type FactFindingReport = z.infer<typeof factFindingReport>
export type DisputeView = z.infer<typeof disputeView>
export type Settlement = z.infer<typeof settlement>
export type Verdict = z.infer<typeof verdict>
export type DisputeState = z.infer<typeof disputeState>
export type ApiError = z.infer<typeof apiError>
export type ReportResponse = z.infer<typeof reportResponse>
export type ReportUnavailableReason = z.infer<typeof reportUnavailableReason>
