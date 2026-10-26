import { keccak_256 } from '@noble/hashes/sha3'
import { PublicKey } from '@solana/web3.js'
import { describe, expect, it } from 'vitest'
import { claimOf, depinClaimOf, depinEscrowPositions, referenceEscrowPositions } from './claims.js'

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

/** A `depin_escrow` dispute on devnet (T037): 60 % reported, 90 % claimed. */
const depinDevnet = {
  escrowRef: 'EZYH3YWdjjEVvgzXk1h37M81YYwoUE5DiMontp9CzXKC',
  claimantClaimHash: '663b2c886ccd9720cb6ad6e959baf39e8c1564c485a30acdf546e0acca9a01b1',
  respondentClaimHash: 'afd9fe329edf2a5a349f6ed4b707f8a6641317211da4dd91f1f0a7ec901a87a1',
}

describe('positions of depin_escrow', () => {
  it('reproduces the fingerprints the deployed escrow wrote', () => {
    const epoch = new PublicKey(depinDevnet.escrowRef)
    expect(depinClaimOf(epoch, 9_000)).toBe(depinDevnet.claimantClaimHash)
    expect(depinClaimOf(epoch, 6_000)).toBe(depinDevnet.respondentClaimHash)
    expect(depinEscrowPositions(depinDevnet.escrowRef, depinDevnet.respondentClaimHash)).toBe(
      "The operator earned 60% of the epoch's reward (6000 bps); the network keeps the rest.",
    )
  })

  /** The layout `claims.rs` pins in its own test: domain, epoch, u16 LE. */
  it('hashes the share as two little-endian bytes', () => {
    const bytes = Buffer.concat([
      Buffer.from('depin_escrow/claim/v1'),
      Buffer.alloc(32, 7),
      Buffer.from([0x28, 0x23]),
    ])
    expect(depinClaimOf(escrow, 9_000)).toBe(Buffer.from(keccak_256(bytes)).toString('hex'))
  })

  it('words an offer it finds, at both ends of the range', () => {
    for (const [bps, text] of [
      [6_000, '60%'],
      [6_050, '60.5%'],
      [0, '0%'],
      [10_000, '100%'],
    ] as const) {
      expect(depinEscrowPositions(escrow.toBase58(), depinClaimOf(escrow, bps))).toBe(
        `The operator earned ${text} of the epoch's reward (${bps} bps); the network keeps the rest.`,
      )
    }
  })

  it('recovers nothing under another epoch', () => {
    const other = new PublicKey(new Uint8Array(32).fill(8))
    expect(depinEscrowPositions(other.toBase58(), depinClaimOf(escrow, 6_000))).toBeNull()
  })

  it("does not mistake reference_escrow's fingerprints for offers", () => {
    expect(depinEscrowPositions(devnet.escrowRef, devnet.claimantClaimHash)).toBeNull()
  })

  it('refuses a share outside 0..=10000', () => {
    expect(() => depinClaimOf(escrow, 10_001)).toThrow(/out of range/)
    expect(() => depinClaimOf(escrow, -1)).toThrow(/out of range/)
  })
})
