import { PublicKey } from '@solana/web3.js'
import { describe, expect, it } from 'vitest'
import { claimOf, referenceEscrowPositions } from './claims.js'

/**
 * A dispute read from devnet (a `Dispute` account of the deployed program),
 * not produced by this code. It is the only vector that can catch a drift
 * between `claims.rs` and its TS copy: a vector computed by `claimOf` itself
 * would agree with `claimOf` whatever the formula.
 */
const devnet = {
  escrowRef: 'FrLaogpoakSn4eHwq8mTP4tQv9sDbSzWMU7A7Lak2Gzw',
  claimantClaimHash: 'a30edcd6a5d39457018418f63066264d99ad4f5bfe2cacc3dd0d66c40fef918f',
  respondentClaimHash: '1f3a428b00a8c59067dbf4fa1c2b32d4b47abb62c191b6c847ac34affcf5821f',
}

const escrow = new PublicKey(new Uint8Array(32).fill(7))

describe('claim fingerprints of reference_escrow', () => {
  it('reproduces the fingerprints the deployed escrow wrote', () => {
    const onChain = new PublicKey(devnet.escrowRef)

    expect(claimOf(onChain, 1, 'release')).toBe(devnet.claimantClaimHash)
    expect(claimOf(onChain, 1, 'refund')).toBe(devnet.respondentClaimHash)
  })

  it('tells the two positions apart', () => {
    expect(claimOf(escrow, 0, 'release')).not.toBe(claimOf(escrow, 0, 'refund'))
  })

  it('binds a position to its milestone and to its escrow', () => {
    expect(claimOf(escrow, 0, 'release')).not.toBe(claimOf(escrow, 1, 'release'))
    expect(claimOf(escrow, 0, 'release')).not.toBe(
      claimOf(new PublicKey(new Uint8Array(32).fill(8)), 0, 'release'),
    )
  })

  it('refuses a milestone a u8 cannot hold', () => {
    expect(() => claimOf(escrow, 256, 'release')).toThrow(/out of range/)
    expect(() => claimOf(escrow, -1, 'release')).toThrow(/out of range/)
  })
})

describe('positions recovered from fingerprints', () => {
  it('words both positions of a devnet dispute', () => {
    expect(referenceEscrowPositions(devnet.escrowRef, devnet.claimantClaimHash)).toBe(
      'Milestone #1 was delivered, so its locked funds should be released to the seller.',
    )
    expect(referenceEscrowPositions(devnet.escrowRef, devnet.respondentClaimHash)).toBe(
      'Milestone #1 was not delivered, so its locked funds should be refunded to the buyer.',
    )
  })

  it('finds the last milestone a u8 can index', () => {
    const fingerprint = claimOf(escrow, 255, 'refund')
    expect(referenceEscrowPositions(escrow.toBase58(), fingerprint)).toMatch(/^Milestone #255 /)
  })

  /**
   * The same fingerprint under another escrow is another claim. Returning a
   * statement here would show a juror a position the dispute does not carry.
   */
  it('recovers nothing under an escrow the fingerprint was not made for', () => {
    expect(referenceEscrowPositions(escrow.toBase58(), devnet.claimantClaimHash)).toBeNull()
  })

  it('recovers nothing from a fingerprint no candidate matches', () => {
    expect(referenceEscrowPositions(devnet.escrowRef, 'ab'.repeat(32))).toBeNull()
  })
})
