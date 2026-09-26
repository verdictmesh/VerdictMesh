import { utils } from '@coral-xyz/anchor'
import type { Commitment, Connection, Finality } from '@solana/web3.js'
import { PublicKey } from '@solana/web3.js'
import type { EvidenceChain } from './evidence.js'
import { verdictMeshIdl } from './idl/verdict-mesh.js'
import type { Chain, ChainAccount } from './watcher.js'

/**
 * The same `Chain`, over RPC. Deliberately thin: everything that can be decided
 * without touching the network is decided in `watcher.ts` and checked by its
 * tests, and what is left here is exactly three RPC calls and no decisions.
 */

/**
 * The discriminator of the `Dispute` account — taken from the IDL rather than
 * computed as `sha256("account:Dispute")` here: Anchor is free to change that
 * formula, while the IDL ships with the build.
 */
const disputeDiscriminator = (): number[] => {
  const account = verdictMeshIdl.accounts.find((candidate) => candidate.name === 'Dispute')
  if (!account) throw new Error('Vendored IDL has no Dispute account')
  return account.discriminator
}

/**
 * `confirmed` rather than `finalized`: the mirror catches up with the chain in
 * seconds instead of tens of seconds, and a rolled-back confirmed slot is
 * healed by the next rewrite — `synced_slot` does not get in the way, because a
 * rollback takes the slot number back with it.
 */
const COMMITMENT: Commitment = 'confirmed'

/** The same level, under the narrower type that history calls accept. */
const FINALITY: Finality = 'confirmed'

export function solanaChain(connection: Connection, programId: PublicKey): Chain {
  return {
    async allDisputes() {
      // `withContext` is not a detail: without it a snapshot of a hundred
      // accounts arrives with no slot at all, and `synced_slot` would be filled
      // with a guess.
      const { context, value } = await connection.getProgramAccounts(programId, {
        commitment: COMMITMENT,
        withContext: true,
        filters: [
          {
            memcmp: {
              offset: 0,
              bytes: utils.bytes.bs58.encode(Buffer.from(disputeDiscriminator())),
            },
          },
        ],
      })

      const accounts: ChainAccount[] = value.map(({ pubkey, account }) => ({
        address: pubkey.toBase58(),
        data: account.data,
      }))

      return { slot: context.slot, accounts }
    },

    async readDispute(address) {
      const { context, value } = await connection.getAccountInfoAndContext(
        new PublicKey(address),
        COMMITMENT,
      )
      if (!value) return null

      return { slot: context.slot, account: { address, data: value.data } }
    },

    async subscribeLogs(onLogs) {
      const subscription = connection.onLogs(
        programId,
        (logs) => {
          // A failed transaction changed no state — reading an account for it
          // would spend an RPC call to write back exactly what is already
          // there.
          if (logs.err !== null) return
          onLogs(logs.logs)
        },
        COMMITMENT,
      )

      return async () => {
        await connection.removeOnLogsListener(subscription)
      }
    },
  }
}

/**
 * The chain as the evidence collector needs it — three RPC calls, no
 * decisions.
 */
export function solanaEvidenceChain(connection: Connection): EvidenceChain {
  return {
    async readAccount(address) {
      const { context, value } = await connection.getAccountInfoAndContext(
        new PublicKey(address),
        COMMITMENT,
      )
      if (!value) return null

      return { slot: context.slot, owner: value.owner.toBase58(), data: value.data }
    },

    async signaturesFor(address, limit) {
      const signatures = await connection.getSignaturesForAddress(
        new PublicKey(address),
        { limit },
        FINALITY,
      )

      return signatures.map((entry) => ({
        signature: entry.signature,
        slot: entry.slot,
        failed: entry.err !== null,
      }))
    },

    async readTransaction(signature) {
      const transaction = await connection.getTransaction(signature, {
        commitment: FINALITY,
        // Without it a v0 transaction is not "unsupported", it is an error for
        // the whole call — and a wallet is free to send v0.
        maxSupportedTransactionVersion: 0,
      })
      if (!transaction) return null

      const { message } = transaction.transaction
      // Signers always come first among the static keys: an address lookup
      // table can never supply a signer.
      const signers = message.staticAccountKeys
        .slice(0, message.header.numRequiredSignatures)
        .map((key) => key.toBase58())

      return {
        slot: transaction.slot,
        blockTime: transaction.blockTime ?? null,
        signers,
        logs: transaction.meta?.logMessages ?? null,
      }
    },
  }
}
