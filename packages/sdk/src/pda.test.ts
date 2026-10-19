import { readFileSync } from 'node:fs'
import { PublicKey } from '@solana/web3.js'
import { describe, expect, it } from 'vitest'
import { disputeAddress, disputeVaultAddress, PROGRAM_ID, SEEDS, voteAddress } from './pda.js'

/** A dispute opened on devnet: `DisputeOpened` names its integrator, the account is its address. */
const devnet = {
  integrator: new PublicKey('DezHTPxw6bX5Dna6fXCQFmzBuk8Ud435SLa8Em93XbZ7'),
  id: 106n,
  dispute: new PublicKey('9qVtSBKgvNWizi5TdTier37WLNnrmdyqJ8vUnGoRLKMP'),
}

describe('seeds', () => {
  it('are the seeds the program declares', () => {
    const source = readFileSync(
      new URL('../../../programs/verdict-mesh/src/seeds.rs', import.meta.url),
      'utf8',
    )
    const declared = [...source.matchAll(/pub const \w+: &\[u8\] = b"([^"]+)";/g)].map(
      (match) => match[1],
    )
    const ours = Object.values(SEEDS).map((seed) => new TextDecoder().decode(seed))

    expect(declared.length).toBeGreaterThan(0)
    expect(ours.sort()).toEqual(declared.sort())
  })
})

describe('disputeAddress', () => {
  it('finds a dispute the program opened on devnet', () => {
    expect(disputeAddress(devnet.integrator, devnet.id).equals(devnet.dispute)).toBe(true)
  })

  it('takes the number as a u64, not a u32', () => {
    expect(disputeAddress(devnet.integrator, 106).equals(devnet.dispute)).toBe(true)
    expect(disputeAddress(devnet.integrator, 2n ** 32n + 106n).equals(devnet.dispute)).toBe(false)
  })

  it('belongs to the program it is asked for', () => {
    const local = PublicKey.unique()
    expect(disputeAddress(devnet.integrator, devnet.id, local).equals(devnet.dispute)).toBe(false)
  })
})

describe('per-dispute addresses', () => {
  it('differ between a dispute vault and a vote', () => {
    const juror = PublicKey.unique()
    const vault = disputeVaultAddress(devnet.dispute)
    const vote = voteAddress(devnet.dispute, juror)

    expect(vault.equals(vote)).toBe(false)
    expect(voteAddress(devnet.dispute, juror, PROGRAM_ID).equals(vote)).toBe(true)
  })
})
