import { utils } from '@coral-xyz/anchor'
import type { ApiError } from '@verdictmesh/shared'
import type { Context } from 'hono'
import type { ContentfulStatusCode } from 'hono/utils/http-status'
import { z } from 'zod'

/**
 * A base58 address of exactly 32 bytes. The length is checked on the decoded
 * bytes: `new PublicKey` accepts shorter strings and pads them, and `"1"`
 * would become the system program.
 */
export const address = z.string().refine((value) => {
  try {
    return utils.bytes.bs58.decode(value).length === 32
  } catch {
    return false
  }
}, 'not a 32-byte base58 address')

/** The error envelope the contract promises on every failure. */
export const fail = (
  c: Context,
  status: ContentfulStatusCode,
  code: ApiError['error']['code'],
  message: string,
) => c.json<ApiError>({ error: { code, message } }, status)
