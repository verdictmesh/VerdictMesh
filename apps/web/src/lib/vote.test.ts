import { PublicKey } from '@solana/web3.js'
import { describe, expect, it } from 'vitest'
import {
  choiceSealedBy,
  commitmentOf,
  commitVoteInstruction,
  decodeJuror,
  decodeVoteCommit,
  revealVoteInstruction,
  saltFromSignature,
  saltMessage,
  type VoteCommitAccount,
  voteAddress,
  voteStep,
} from './vote'

const hex = (bytes: Uint8Array) => Buffer.from(bytes).toString('hex')
const unhex = (value: string) => Uint8Array.from(Buffer.from(value, 'hex'))

/**
 * A vote that was sealed and opened on devnet: the commitment from the
 * `commit_vote` transaction, the salt and choice from the `reveal_vote` one
 * the program accepted. The program is the only other copy of the formula, so
 * matching what it accepted is the test.
 */
const devnet = {
  dispute: new PublicKey('AmFiyRjwPPkcHF3PwctqEzbp5sUH7EGhamfSsTic2aof'),
  juror: new PublicKey('2c8Q5hyBTWxmn5yH7VPVJPrj6tPAZVwAeSPUcJQyJCQx'),
  salt: unhex('91f1878966c4ad22c58dd05e9ce77b8183eae149ae091a5b3f744da2cb586c9c'),
  commitment: '17d7cf0002ec711b028d52ab48a0567fe0757e6c1b832e6ded1f5365a544d1d0',
}

const program = new PublicKey('8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq')

describe('commitmentOf', () => {
  it('matches a commitment the program accepted on devnet', () => {
    expect(hex(commitmentOf(devnet.dispute, devnet.juror, 'Claimant', devnet.salt))).toBe(
      devnet.commitment,
    )
  })

  it('gives each choice its own commitment', () => {
    expect(hex(commitmentOf(devnet.dispute, devnet.juror, 'Respondent', devnet.salt))).not.toBe(
      devnet.commitment,
    )
  })

  it('belongs to one juror', () => {
    expect(hex(commitmentOf(devnet.dispute, program, 'Claimant', devnet.salt))).not.toBe(
      devnet.commitment,
    )
  })

  it('refuses a salt of the wrong length', () => {
    expect(() =>
      commitmentOf(devnet.dispute, devnet.juror, 'Claimant', new Uint8Array(31)),
    ).toThrow()
  })
})

describe('salt', () => {
  it('is the same for the same signature and differs per dispute message', () => {
    const signature = new Uint8Array(64).fill(7)
    expect(hex(saltFromSignature(signature))).toBe(hex(saltFromSignature(signature)))
    expect(saltFromSignature(signature)).toHaveLength(32)
    expect(Buffer.from(saltMessage(devnet.dispute)).toString()).not.toBe(
      Buffer.from(saltMessage(devnet.juror)).toString(),
    )
  })
})

describe('instructions', () => {
  it('lays out commit_vote as the IDL does', () => {
    const ix = commitVoteInstruction(
      program,
      devnet.dispute,
      devnet.juror,
      unhex(devnet.commitment),
    )
    expect([...ix.data.subarray(0, 8)]).toEqual([134, 97, 90, 126, 91, 66, 16, 26])
    expect(hex(ix.data.subarray(8))).toBe(devnet.commitment)
    expect(ix.keys.map((k) => [k.isSigner, k.isWritable])).toEqual([
      [true, true],
      [false, false],
      [false, true],
      [false, false],
    ])
    expect(ix.keys[2]?.pubkey.equals(voteAddress(program, devnet.dispute, devnet.juror))).toBe(true)
  })

  it('lays out reveal_vote with the borsh index of the choice, then the salt', () => {
    const claimant = revealVoteInstruction(
      program,
      devnet.dispute,
      devnet.juror,
      'Claimant',
      devnet.salt,
    )
    const respondent = revealVoteInstruction(
      program,
      devnet.dispute,
      devnet.juror,
      'Respondent',
      devnet.salt,
    )
    expect([...claimant.data.subarray(0, 8)]).toEqual([100, 157, 139, 17, 186, 75, 185, 149])
    expect(claimant.data[8]).toBe(0)
    expect(respondent.data[8]).toBe(1)
    expect(hex(claimant.data.subarray(9))).toBe(hex(devnet.salt))
    expect(claimant.keys.map((k) => [k.isSigner, k.isWritable])).toEqual([
      [true, false],
      [false, true],
      [false, true],
    ])
  })
})

const voteCommitBytes = (choice: number | null, round: number) =>
  Uint8Array.from([
    125,
    216,
    109,
    1,
    40,
    87,
    250,
    47,
    ...devnet.dispute.toBytes(),
    ...devnet.juror.toBytes(),
    ...unhex(devnet.commitment),
    ...(choice === null ? [0] : [1, choice]),
    round,
    255,
  ])

describe('decodeVoteCommit', () => {
  it('reads a sealed vote', () => {
    const vote = decodeVoteCommit(voteCommitBytes(null, 0))
    expect(vote?.choice).toBeNull()
    expect(vote?.round).toBe(0)
    expect(hex(vote?.commitment ?? new Uint8Array())).toBe(devnet.commitment)
  })

  it('reads an opened vote and its round', () => {
    expect(decodeVoteCommit(voteCommitBytes(1, 1))).toMatchObject({
      choice: 'Respondent',
      round: 1,
    })
  })

  it('refuses another account', () => {
    expect(decodeVoteCommit(new Uint8Array(120))).toBeNull()
  })
})

describe('decodeJuror', () => {
  it('reads the stake as a u64', () => {
    const data = new Uint8Array(8 + 32 + 8 + 2 + 4 + 1)
    data.set([209, 201, 239, 217, 237, 84, 189, 152])
    new DataView(data.buffer).setBigUint64(40, 18_446_744_073_709_551_615n, true)
    new DataView(data.buffer).setUint16(48, 3, true)
    expect(decodeJuror(data)).toEqual({ stake: 18_446_744_073_709_551_615n, activeDisputes: 3 })
  })
})

describe('voteStep', () => {
  const wallet = devnet.juror.toBase58()
  const seated = { panel: [wallet], escalated: false }
  const sealed: VoteCommitAccount = { commitment: new Uint8Array(32), choice: null, round: 0 }

  it.each([
    ['off the panel', { ...seated, panel: [] }, null, 'not-seated'],
    ['seated, nothing sealed', seated, null, 'can-commit'],
    ['sealed this round', seated, sealed, 'committed'],
    ['opened', seated, { ...sealed, choice: 'Claimant' as const }, 'revealed'],
    [
      'sealed in the first round of an escalated dispute',
      { ...seated, escalated: true },
      sealed,
      'stale',
    ],
  ])('%s', (_, dispute, vote, step) => {
    expect(voteStep(dispute, wallet, vote)).toBe(step)
  })
})

describe('choiceSealedBy', () => {
  it('reads the choice back from the devnet commitment', () => {
    expect(
      choiceSealedBy(devnet.dispute, devnet.juror, devnet.salt, unhex(devnet.commitment)),
    ).toBe('Claimant')
  })

  it('finds a respondent vote too', () => {
    const sealed = commitmentOf(devnet.dispute, devnet.juror, 'Respondent', devnet.salt)
    expect(choiceSealedBy(devnet.dispute, devnet.juror, devnet.salt, sealed)).toBe('Respondent')
  })

  it('opens nothing with another salt', () => {
    expect(
      choiceSealedBy(devnet.dispute, devnet.juror, new Uint8Array(32), unhex(devnet.commitment)),
    ).toBeNull()
  })
})
