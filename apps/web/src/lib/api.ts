import {
  apiError,
  type DisputeState,
  type DisputeView,
  disputeView,
  type ReportResponse,
  reportResponse,
} from '@verdictmesh/shared'
import { z } from 'zod'

/**
 * The one place `apps/web` talks to `apps/api`. Every answer is parsed with the
 * contract from `packages/shared` — the same schema the server answers with —
 * so a drift between the two shows up as an error on screen, not as a field
 * quietly read as `undefined`.
 *
 * No credentials, no wallet: every route is a public read (`FR-028a`).
 */

const configured: string | undefined = import.meta.env.VITE_API_URL

/** Without a trailing slash. `null` — the build was made without an api. */
export const API_URL = configured ? configured.replace(/\/+$/, '') : null

export class ApiFailure extends Error {
  constructor(
    message: string,
    readonly status: number | null,
    readonly code: string | null = null,
  ) {
    super(message)
    this.name = 'ApiFailure'
  }
}

async function get<T>(path: string, schema: z.ZodType<T>, signal?: AbortSignal): Promise<T> {
  if (API_URL === null) throw new ApiFailure('This build has no VITE_API_URL', null)

  let response: Response
  try {
    response = await fetch(`${API_URL}${path}`, { signal: signal ?? null })
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') throw err
    throw new ApiFailure('The dispute service could not be reached', null)
  }

  const body: unknown = await response.json().catch(() => null)
  if (!response.ok) {
    const envelope = apiError.safeParse(body)
    throw new ApiFailure(
      envelope.success ? envelope.data.error.message : `HTTP ${response.status}`,
      response.status,
      envelope.success ? envelope.data.error.code : null,
    )
  }

  const parsed = schema.safeParse(body)
  if (!parsed.success) {
    throw new ApiFailure('The dispute service answered outside its contract', response.status)
  }
  return parsed.data
}

export interface DisputeFilter {
  state?: DisputeState
  integrator?: string
  juror?: string
}

export function listDisputes(filter: DisputeFilter = {}, signal?: AbortSignal) {
  const query = new URLSearchParams()
  for (const [name, value] of Object.entries(filter)) {
    if (typeof value === 'string') query.set(name, value)
  }
  const suffix = query.size > 0 ? `?${query}` : ''
  return get<DisputeView[]>(`/disputes${suffix}`, z.array(disputeView), signal)
}

export function getDispute(pda: string, signal?: AbortSignal) {
  return get<DisputeView>(`/disputes/${encodeURIComponent(pda)}`, disputeView, signal)
}

export function getReport(pda: string, signal?: AbortSignal) {
  return get<ReportResponse>(`/disputes/${encodeURIComponent(pda)}/report`, reportResponse, signal)
}
