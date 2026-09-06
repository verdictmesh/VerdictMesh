import { afterEach, describe, expect, it } from 'vitest'
import { loadEnv } from './env.js'

const complete = {
  SOLANA_RPC_URL: 'https://api.devnet.solana.com',
  SOLANA_WS_URL: 'wss://api.devnet.solana.com',
  VERDICT_MESH_PROGRAM_ID: '8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq',
  SETTLEMENT_MINT: '4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU',
  REPORTER_KEYPAIR: 'base58-secret',
  ANTHROPIC_API_KEY: 'sk-ant-test',
  DATABASE_URL: 'postgresql://user:pass@host:6543/postgres',
}

const original = { ...process.env }

afterEach(() => {
  process.env = { ...original }
})

describe('loadEnv', () => {
  it('parses a complete environment and defaults the port', () => {
    process.env = { ...complete } as NodeJS.ProcessEnv
    const env = loadEnv()
    expect(env.PORT).toBe(8080)
    expect(env.LOG_LEVEL).toBe('info')
  })

  it('fails fast when the reporter key is missing', () => {
    const { REPORTER_KEYPAIR: _missing, ...withoutReporter } = complete
    process.env = { ...withoutReporter } as NodeJS.ProcessEnv
    expect(() => loadEnv()).toThrow(/Invalid environment/)
  })

  it('rejects an RPC url that is not a url', () => {
    process.env = { ...complete, SOLANA_RPC_URL: 'devnet' } as NodeJS.ProcessEnv
    expect(() => loadEnv()).toThrow(/Invalid environment/)
  })

  it('rejects a websocket url that is not a websocket', () => {
    process.env = { ...complete, SOLANA_WS_URL: 'https://api.devnet.solana.com' } as NodeJS.ProcessEnv
    expect(() => loadEnv()).toThrow(/Invalid environment/)
  })
})
