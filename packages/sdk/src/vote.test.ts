import { PublicKey } from '@solana/web3.js'
import { describe, expect, it } from 'vitest'
import { ballotArg, commitmentOf } from './vote.js'

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
    const other = new PublicKey('8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq')
    expect(hex(commitmentOf(devnet.dispute, other, 'Claimant', devnet.salt))).not.toBe(
      devnet.commitment,
    )
  })

  it('refuses a salt of the wrong length', () => {
    expect(() =>
      commitmentOf(devnet.dispute, devnet.juror, 'Claimant', new Uint8Array(31)),
    ).toThrow()
  })
})

describe('ballotArg', () => {
  it('is the enum argument Anchor takes', () => {
    expect(ballotArg('Claimant')).toEqual({ claimant: {} })
    expect(ballotArg('Respondent')).toEqual({ respondent: {} })
  })
})
