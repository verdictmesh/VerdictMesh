import type { DisputeAccounts } from '@verdictmesh/sdk'
import { describe, expect, it } from 'vitest'
import { depinEscrowIdl } from './idl/depin-escrow.js'
import { referenceEscrowIdl } from './idl/reference-escrow.js'

/**
 * Every key of the SDK's `DisputeAccounts`. A `Record` over the type's keys:
 * a key added to or dropped from the type is a red typecheck here, so the
 * list cannot drift from what `prepareDispute` returns.
 */
const sdkNames: Record<keyof DisputeAccounts, true> = {
  integrator: true,
  config: true,
  settlementMint: true,
  dispute: true,
  claimantTokens: true,
  disputeVault: true,
  verdictMeshProgram: true,
  tokenProgram: true,
  systemProgram: true,
}

const snake = (name: string) => name.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`)

const accountsOf = (
  idl: typeof depinEscrowIdl | typeof referenceEscrowIdl,
  instruction: string,
) => {
  const found = idl.instructions.find((candidate) => candidate.name === instruction)
  if (found === undefined) throw new Error(`${instruction} is not in the IDL`)
  return found.accounts.map((account) => account.name)
}

/**
 * The SDK's half of `SC-004`: an escrow that names its VerdictMesh accounts
 * the way `open_dispute` does takes `prepared.accounts` as is. Two escrows,
 * structurally unlike, both do — a third that does not pays ~9 lines.
 */
describe('escrows take the SDK dispute accounts as is', () => {
  for (const [escrow, idl, instruction] of [
    ['reference_escrow', referenceEscrowIdl, 'dispute_milestone'],
    ['depin_escrow', depinEscrowIdl, 'dispute_epoch'],
  ] as const) {
    it(`${escrow}.${instruction}`, () => {
      const accounts = accountsOf(idl, instruction)
      for (const name of Object.keys(sdkNames)) {
        expect(accounts).toContain(snake(name))
      }
    })
  }
})
