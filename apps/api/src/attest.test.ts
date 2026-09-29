import { utils } from '@coral-xyz/anchor'
import { type Connection, Keypair, PublicKey, type SignatureStatus } from '@solana/web3.js'
import { describe, expect, it, vi } from 'vitest'
import { attestInstruction, configPda, reporterKeypair, solanaAttester } from './attest.js'
import { verdictMeshIdl } from './idl/verdict-mesh.js'

const programId = new PublicKey(verdictMeshIdl.address)
const reporter = Keypair.fromSeed(new Uint8Array(32).fill(7))
const dispute = new PublicKey(new Uint8Array(32).fill(13))
const hash = 'ab'.repeat(32)

const idlInstruction = () => {
  const found = verdictMeshIdl.instructions.find((entry) => entry.name === 'attest_report')
  if (!found) throw new Error('no attest_report in the vendored IDL')
  return found
}

describe('the attest_report instruction', () => {
  it('carries the discriminator from the IDL followed by the 32 fingerprint bytes', () => {
    const ix = attestInstruction(programId, reporter.publicKey, dispute, hash)

    expect(ix.programId.equals(programId)).toBe(true)
    expect([...ix.data.subarray(0, 8)]).toEqual(idlInstruction().discriminator)
    expect(ix.data.subarray(8).toString('hex')).toBe(hash)
    expect(ix.data).toHaveLength(40)
  })

  /** The reporter signs and writes nothing; the dispute is the one account written. */
  it('names the reporter, the protocol config and the dispute, as the program expects', () => {
    const ix = attestInstruction(programId, reporter.publicKey, dispute, hash)

    expect(
      ix.keys.map(({ pubkey, isSigner, isWritable }) => [pubkey.toBase58(), isSigner, isWritable]),
    ).toEqual([
      [reporter.publicKey.toBase58(), true, false],
      [configPda(programId).toBase58(), false, false],
      [dispute.toBase58(), false, true],
    ])
  })

  it('refuses anything that is not a fingerprint before it costs a transaction', () => {
    for (const bad of ['0'.repeat(64), 'AB'.repeat(32), 'ab'.repeat(31), `${'ab'.repeat(31)}zz`]) {
      expect(() => attestInstruction(programId, reporter.publicKey, dispute, bad)).toThrow(
        /Not a report fingerprint/,
      )
    }
  })
})

describe('the reporter key', () => {
  it('reads a base58 secret key', () => {
    const secret = utils.bytes.bs58.encode(reporter.secretKey)

    expect(reporterKeypair(secret).publicKey.equals(reporter.publicKey)).toBe(true)
  })

  it('never repeats the value it could not read', () => {
    const secret = 'not-a-key-0OIl'

    expect(() => reporterKeypair(secret)).toThrow('REPORTER_KEYPAIR is not a base58 secret key')
    try {
      reporterKeypair(secret)
    } catch (error) {
      expect(String(error)).not.toContain(secret)
    }
  })
})

type Status = Pick<SignatureStatus, 'err' | 'confirmationStatus'> | null

/** The four RPC calls the attester makes, and nothing else. */
function fakeConnection(statuses: Status[], heights: number[] = [100]) {
  const sent: Buffer[] = []
  const connection = {
    getLatestBlockhash: vi.fn(async () => ({
      blockhash: new PublicKey(new Uint8Array(32).fill(1)).toBase58(),
      lastValidBlockHeight: 150,
    })),
    sendRawTransaction: vi.fn(async (raw: Buffer) => {
      sent.push(raw)
      return 'signature'
    }),
    getSignatureStatuses: vi.fn(async () => ({
      context: { slot: 1 },
      value: [statuses.shift() ?? null],
    })),
    getBlockHeight: vi.fn(async () => heights.shift() ?? 100),
  }
  return { connection: connection as unknown as Connection, calls: connection, sent }
}

const attest = (connection: Connection) =>
  solanaAttester(connection, programId, reporter, { sleep: async () => {} }).attest(
    dispute.toBase58(),
    hash,
  )

describe('attesting over RPC', () => {
  it('signs with the reporter and resolves once the chain has confirmed it', async () => {
    const { connection, calls, sent } = fakeConnection([
      null,
      { err: null, confirmationStatus: 'processed' },
      { err: null, confirmationStatus: 'confirmed' },
    ])

    await expect(attest(connection)).resolves.toBe('signature')
    expect(calls.getSignatureStatuses).toHaveBeenCalledTimes(3)
    expect(sent).toHaveLength(1)
  })

  it('fails with the program error the transaction landed with', async () => {
    const { connection } = fakeConnection([
      { err: { InstructionError: [0, { Custom: 6010 }] }, confirmationStatus: 'confirmed' },
    ])

    await expect(attest(connection)).rejects.toThrow(/6010/)
  })

  /** Past its last valid block height a transaction can no longer land. */
  it('gives up once the blockhash has expired', async () => {
    const { connection } = fakeConnection([null, null], [149, 151])

    await expect(attest(connection)).rejects.toThrow(/expired unconfirmed/)
  })
})
