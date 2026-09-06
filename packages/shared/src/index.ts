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
  commitDeadline: z.number().int(),
  revealDeadline: z.number().int(),
  appealDeadline: z.number().int(),
  escalated: z.boolean(),
  verdict: verdict.nullable(),
  settled: z.boolean(),
})

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
export type Verdict = z.infer<typeof verdict>
export type DisputeState = z.infer<typeof disputeState>
export type ApiError = z.infer<typeof apiError>
