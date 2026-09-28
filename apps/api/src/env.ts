import { z } from 'zod'

/**
 * The environment is validated once, at start-up, and fails right there. A
 * service that came up without the reporter key and found out an hour later,
 * on its first dispute, is worse than one that never came up.
 */
const schema = z.object({
  PORT: z.coerce.number().int().positive().default(8080),
  SOLANA_RPC_URL: z.url(),
  SOLANA_WS_URL: z.string().startsWith('ws'),
  VERDICT_MESH_PROGRAM_ID: z.string().min(32).max(44),
  /**
   * Where `reference_escrow` lives in this network. Its events and accounts
   * are decoded in the evidence and its positions are recovered for the
   * report; any other escrow is collected as raw facts.
   */
  REFERENCE_ESCROW_PROGRAM_ID: z.string().min(32).max(44),
  SETTLEMENT_MINT: z.string().min(32).max(44),
  REPORTER_KEYPAIR: z.string().min(1),
  ANTHROPIC_API_KEY: z.string().min(1),
  DATABASE_URL: z.string().min(1),
  LOG_LEVEL: z.enum(['fatal', 'error', 'warn', 'info', 'debug', 'trace']).default('info'),
})

export type Env = z.infer<typeof schema>

export function loadEnv(): Env {
  const parsed = schema.safeParse(process.env)
  if (!parsed.success) {
    throw new Error(`Invalid environment: ${JSON.stringify(z.treeifyError(parsed.error))}`)
  }
  return parsed.data
}
