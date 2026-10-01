import {
  SolanaSignAndSendTransaction,
  type SolanaSignAndSendTransactionFeature,
  SolanaSignMessage,
  type SolanaSignMessageFeature,
  SolanaSignTransaction,
  type SolanaSignTransactionFeature,
} from '@solana/wallet-standard-features'
import { PublicKey, type Transaction } from '@solana/web3.js'
import { getWallets } from '@wallet-standard/app'
import type { Wallet, WalletAccount } from '@wallet-standard/base'
import {
  StandardConnect,
  type StandardConnectFeature,
  StandardDisconnect,
  type StandardDisconnectFeature,
  StandardEvents,
  type StandardEventsFeature,
} from '@wallet-standard/features'
import bs58 from 'bs58'
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from 'react'
import { CHAIN, connection, prepare, waitFor } from './chain'

/**
 * The juror's wallet, through Wallet Standard directly — not
 * `@solana/wallet-adapter-react`, which under pnpm pulls in React Native for
 * its mobile adapter. Phantom, Solflare and Backpack register themselves this
 * way; what is lost is the Mobile Wallet Adapter only.
 *
 * Connecting is not signing in (`FR-028`): there is no account on our side,
 * and nothing about the wallet is sent to `api`. The right to vote is the
 * program's to check, at `commit_vote`, against the panel and the stake.
 */

const REMEMBERED = 'verdictmesh.wallet'

const remember = (name: string | null) => {
  try {
    if (name === null) localStorage.removeItem(REMEMBERED)
    else localStorage.setItem(REMEMBERED, name)
  } catch {
    // Storage blocked: the wallet is simply not remembered.
  }
}

const remembered = () => {
  try {
    return localStorage.getItem(REMEMBERED)
  } catch {
    return null
  }
}

/** A wallet the page can vote with: it connects, and it can sign a transaction somehow. */
const usable = (wallet: Wallet) =>
  StandardConnect in wallet.features &&
  wallet.chains.includes(CHAIN) &&
  (SolanaSignAndSendTransaction in wallet.features || SolanaSignTransaction in wallet.features)

type Features = Partial<
  StandardConnectFeature &
    StandardDisconnectFeature &
    StandardEventsFeature &
    SolanaSignAndSendTransactionFeature &
    SolanaSignTransactionFeature &
    SolanaSignMessageFeature
>

const featuresOf = (wallet: Wallet) => wallet.features as Features

export interface WalletState {
  wallets: readonly Wallet[]
  wallet: Wallet | null
  address: string | null
  publicKey: PublicKey | null
  connecting: boolean
  connect(wallet: Wallet): Promise<void>
  disconnect(): Promise<void>
  /** `null` — the wallet cannot sign messages; the vote falls back to a stored salt. */
  signMessage: ((message: Uint8Array) => Promise<Uint8Array>) | null
  /** Signs, sends and waits for `confirmed`. Resolves with the signature. */
  send(transaction: Transaction): Promise<string>
}

const WalletContext = createContext<WalletState | null>(null)

export function WalletProvider({ children }: { children: ReactNode }) {
  const [wallets, setWallets] = useState<readonly Wallet[]>([])
  const [wallet, setWallet] = useState<Wallet | null>(null)
  const [account, setAccount] = useState<WalletAccount | null>(null)
  const [connecting, setConnecting] = useState(false)

  useEffect(() => {
    const registry = getWallets()
    const refresh = () => setWallets(registry.get().filter(usable))
    refresh()
    const offRegister = registry.on('register', refresh)
    const offUnregister = registry.on('unregister', refresh)
    return () => {
      offRegister()
      offUnregister()
    }
  }, [])

  const connect = useCallback(async (target: Wallet, silent = false) => {
    const connectFeature = featuresOf(target)[StandardConnect]
    if (!connectFeature) return
    setConnecting(true)
    try {
      const { accounts } = await connectFeature.connect(silent ? { silent: true } : undefined)
      const first = accounts.find((candidate) => candidate.chains.includes(CHAIN)) ?? accounts[0]
      if (!first) return
      setWallet(target)
      setAccount(first)
      remember(target.name)
    } finally {
      setConnecting(false)
    }
  }, [])

  // Reconnect silently to the wallet chosen last time, once it registers.
  useEffect(() => {
    if (wallet) return
    const name = remembered()
    const known = name === null ? undefined : wallets.find((candidate) => candidate.name === name)
    if (known) connect(known, true).catch(() => remember(null))
  }, [wallets, wallet, connect])

  // The wallet may switch or drop the account on its own side.
  useEffect(() => {
    const events = wallet ? featuresOf(wallet)[StandardEvents] : undefined
    if (!wallet || !events) return
    return events.on('change', ({ accounts }) => {
      if (!accounts) return
      setAccount(
        accounts.find((candidate) => candidate.chains.includes(CHAIN)) ?? accounts[0] ?? null,
      )
    })
  }, [wallet])

  const disconnect = useCallback(async () => {
    const feature = wallet ? featuresOf(wallet)[StandardDisconnect] : undefined
    setWallet(null)
    setAccount(null)
    remember(null)
    await feature?.disconnect()
  }, [wallet])

  const value = useMemo<WalletState>(() => {
    const features = wallet ? featuresOf(wallet) : {}
    const publicKey = account ? new PublicKey(account.publicKey) : null
    const signMessageFeature = features[SolanaSignMessage]

    return {
      wallets,
      wallet,
      address: account?.address ?? null,
      publicKey,
      connecting,
      connect: (target) => connect(target),
      disconnect,
      signMessage:
        account && signMessageFeature
          ? async (message) => {
              const [output] = await signMessageFeature.signMessage({ account, message })
              if (!output) throw new Error('The wallet returned no signature')
              return output.signature
            }
          : null,
      async send(transaction) {
        if (!account || !publicKey) throw new Error('No wallet connected')
        const latest = await prepare(transaction, publicKey)
        const bytes = transaction.serialize({
          requireAllSignatures: false,
          verifySignatures: false,
        })

        let signature: string
        const signAndSend = features[SolanaSignAndSendTransaction]
        const signOnly = features[SolanaSignTransaction]
        if (signAndSend) {
          const [output] = await signAndSend.signAndSendTransaction({
            account,
            chain: CHAIN,
            transaction: bytes,
            options: { commitment: 'confirmed' },
          })
          if (!output) throw new Error('The wallet returned no signature')
          signature = bs58.encode(output.signature)
        } else if (signOnly) {
          const [output] = await signOnly.signTransaction({
            account,
            chain: CHAIN,
            transaction: bytes,
          })
          if (!output) throw new Error('The wallet returned no transaction')
          signature = await connection.sendRawTransaction(output.signedTransaction, {
            preflightCommitment: 'confirmed',
          })
        } else {
          throw new Error('This wallet cannot sign transactions')
        }

        const result = await waitFor(signature, latest.lastValidBlockHeight)
        if (!result.ok) throw new Error(result.error)
        return signature
      },
    }
  }, [wallets, wallet, account, connecting, connect, disconnect])

  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>
}

export function useWallet(): WalletState {
  const state = useContext(WalletContext)
  if (!state) throw new Error('useWallet outside WalletProvider')
  return state
}
