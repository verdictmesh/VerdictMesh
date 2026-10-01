import { PublicKey, Transaction } from '@solana/web3.js'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import type { DisputeView } from '@verdictmesh/shared'
import { Buffer } from 'buffer'
import { Check, KeyRound, Lock } from 'lucide-react'
import { type ReactNode, useState } from 'react'
import HashRef from '@/components/HashRef'
import { PROGRAM_ID, readVote } from '@/lib/chain'
import { explorerTx, formatDuration, stageOf } from '@/lib/dispute'
import {
  type Ballot,
  choiceSealedBy,
  commitmentOf,
  commitVoteInstruction,
  randomSalt,
  revealVoteInstruction,
  saltFromSignature,
  saltMessage,
  storedSalt,
  storeSalt,
  voteStep,
} from '@/lib/vote'
import { useWallet } from '@/lib/wallet'

const SIDES: { ballot: Ballot; label: string }[] = [
  { ballot: 'Claimant', label: 'Claimant’s position' },
  { ballot: 'Respondent', label: 'Respondent’s position' },
]

const StepShell = ({
  index,
  title,
  state,
  children,
}: {
  index: number
  title: string
  state: 'open' | 'done' | 'closed' | 'waiting'
  children: ReactNode
}) => (
  <div
    className={`rounded border p-4 ${
      state === 'open'
        ? 'border-border-strong bg-surface'
        : state === 'done'
          ? 'border-confirmed/40 bg-surface'
          : 'border-border bg-surface/50'
    }`}
  >
    <div className="mb-3 flex items-baseline justify-between gap-4">
      <h4 className="text-[13.5px] font-semibold tracking-tight text-foreground">
        <span className="mono mr-2 text-muted-foreground">Step {index}</span>
        {title}
      </h4>
      <span
        className={`label-xs ${
          state === 'open'
            ? 'text-claimed'
            : state === 'done'
              ? 'text-confirmed'
              : 'text-unestablished'
        }`}
      >
        {state === 'open'
          ? 'Open now'
          : state === 'done'
            ? 'Done'
            : state === 'waiting'
              ? 'Not open yet'
              : 'Closed'}
      </span>
    </div>
    <div className={state === 'closed' || state === 'waiting' ? 'opacity-70' : ''}>{children}</div>
  </div>
)

const actionClass =
  'focus-ring mt-3 w-full rounded border border-border-strong bg-surface-2 px-3 py-2.5 text-[13px] font-medium text-foreground transition-colors hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-45'

/**
 * The juror's two steps (`FR-019`), sent from the juror's own wallet.
 *
 * The salt comes from a signature of a fixed message: deterministic, so the
 * reveal re-derives it instead of reading it from storage, and the choice is
 * read back from the commitment on chain. A wallet that cannot sign messages
 * gets a random salt kept in this browser — said out loud, because clearing
 * the browser then costs the juror their stake.
 */
const VotePanel = ({ dispute, now }: { dispute: DisputeView; now: number }) => {
  const wallet = useWallet()
  const queryClient = useQueryClient()
  const [choice, setChoice] = useState<Ballot | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [lastSignature, setLastSignature] = useState<string | null>(null)

  const disputeKey = new PublicKey(dispute.pda)
  const juror = wallet.publicKey
  const stage = stageOf(dispute, now)

  const vote = useQuery({
    queryKey: ['vote', dispute.pda, wallet.address],
    queryFn: () => (juror ? readVote(disputeKey, juror) : null),
    enabled: juror !== null,
    refetchInterval: stage === 'commit' || stage === 'reveal' ? 5_000 : false,
  })

  if (!juror || !wallet.address) {
    return (
      <p className="text-[13px] leading-relaxed text-muted-foreground">
        Panel members vote with the wallet that holds their stake. Connect it at the top of the
        page; nothing about it is sent to our service.
      </p>
    )
  }

  if (vote.isLoading)
    return <p className="label-xs animate-pulse text-unestablished">Reading your vote…</p>

  const step = voteStep(dispute, wallet.address, vote.data ?? null)

  if (step === 'not-seated') {
    return (
      <p className="text-[13px] leading-relaxed text-muted-foreground">
        This wallet is not on the panel of this hearing. Panels are drawn from staked jurors when
        the hearing opens; only they can vote, and the program checks it, not this page.
      </p>
    )
  }

  const salt = async (): Promise<{ salt: Uint8Array; kept: 'signature' | 'browser' }> => {
    if (wallet.signMessage) {
      return {
        salt: saltFromSignature(await wallet.signMessage(saltMessage(disputeKey))),
        kept: 'signature',
      }
    }
    const stored = storedSalt(disputeKey, juror)
    if (stored) return { salt: stored, kept: 'browser' }
    const fresh = randomSalt()
    if (!storeSalt(disputeKey, juror, fresh)) {
      throw new Error(
        'This browser does not allow storing the vote secret, and this wallet cannot sign messages.',
      )
    }
    return { salt: fresh, kept: 'browser' }
  }

  const run = async (label: string, work: () => Promise<string>) => {
    setBusy(label)
    setError(null)
    try {
      setLastSignature(await work())
      await queryClient.invalidateQueries({ queryKey: ['vote', dispute.pda] })
      await queryClient.invalidateQueries({ queryKey: ['dispute', dispute.pda] })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  const seal = () =>
    run('Sealing…', async () => {
      if (choice === null) throw new Error('Choose a side first')
      const secret = await salt()
      const commitment = commitmentOf(disputeKey, juror, choice, secret.salt)
      return wallet.send(
        new Transaction().add(commitVoteInstruction(PROGRAM_ID, disputeKey, juror, commitment)),
      )
    })

  const open = () =>
    run('Opening…', async () => {
      const sealed = vote.data
      if (!sealed) throw new Error('Nothing sealed to open')
      const secret = await salt()
      const sealedChoice = choiceSealedBy(disputeKey, juror, secret.salt, sealed.commitment)
      if (sealedChoice === null) {
        throw new Error(
          'This secret does not open your sealed vote. It was sealed with another one — from another browser, or before this browser was cleared.',
        )
      }
      return wallet.send(
        new Transaction().add(
          revealVoteInstruction(PROGRAM_ID, disputeKey, juror, sealedChoice, secret.salt),
        ),
      )
    })

  const commitOpen = stage === 'commit'
  const revealOpen = stage === 'reveal'
  const sealedHere = step === 'committed' || step === 'revealed'

  const notice =
    step === 'stale'
      ? 'You sealed a vote in the first round. The hearing was escalated to a wider panel, and a first-round vote does not count in the second.'
      : commitOpen
        ? `The commit window is open — ${formatDuration((dispute.commitDeadline - now) * 1000)} left.`
        : revealOpen
          ? `The reveal window is open — ${formatDuration((dispute.revealDeadline - now) * 1000)} left.`
          : 'Both voting windows have closed. No further votes are accepted.'

  return (
    <div className="flex flex-col gap-3">
      <div className="rounded border border-border bg-surface-2/50 px-3.5 py-2.5">
        <p className="text-[13px] leading-relaxed text-foreground">{notice}</p>
      </div>

      <StepShell
        index={1}
        title="Seal your vote"
        state={sealedHere ? 'done' : commitOpen && step === 'can-commit' ? 'open' : 'closed'}
      >
        {sealedHere ? (
          <div className="flex flex-col gap-2.5">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
              <span className="label-xs flex items-center gap-1.5 text-confirmed">
                <Lock className="h-3 w-3" /> sealed fingerprint on-chain
              </span>
              <HashRef
                value={Buffer.from(vote.data?.commitment ?? []).toString('hex')}
                lead={6}
                tail={6}
              />
            </div>
            <p className="flex items-center gap-1.5 text-[12.5px] leading-relaxed text-muted-foreground">
              <KeyRound className="h-3 w-3" />
              {wallet.signMessage
                ? 'The secret is your wallet’s signature of a fixed message: it is re-derived to open the vote, on any device.'
                : 'The secret is stored in this browser. Clearing it before the reveal makes the vote impossible to open.'}
            </p>
          </div>
        ) : (
          <>
            <div className="grid gap-2 sm:grid-cols-2">
              {SIDES.map((side) => {
                const active = choice === side.ballot
                return (
                  <button
                    key={side.ballot}
                    type="button"
                    disabled={!commitOpen || busy !== null}
                    onClick={() => setChoice(side.ballot)}
                    className={`focus-ring rounded border px-3 py-2.5 text-left transition-colors ${
                      active
                        ? 'border-foreground bg-surface-2'
                        : 'border-border hover:border-border-strong'
                    } disabled:cursor-not-allowed disabled:opacity-50`}
                  >
                    <span className="label-xs text-muted-foreground">Vote for</span>
                    <span className="mt-1.5 block text-[13.5px] font-medium text-foreground">
                      {side.label}
                    </span>
                    {active ? (
                      <span className="mono mt-1.5 flex items-center gap-1.5 text-[11px] text-confirmed">
                        <Check className="h-3 w-3" /> selected
                      </span>
                    ) : null}
                  </button>
                )
              })}
            </div>
            <button
              type="button"
              disabled={!commitOpen || choice === null || busy !== null}
              onClick={seal}
              className={actionClass}
            >
              {busy ?? (commitOpen ? 'Seal choice' : 'Commit window closed')}
            </button>
            <p className="mt-3 text-[12.5px] leading-relaxed text-muted-foreground">
              Sealing hides your choice until the reveal window: it is unreadable to the other panel
              members, both parties and the team that built VerdictMesh.
              {wallet.signMessage
                ? ' Your wallet will ask you to sign a message first — that signature is the secret — and then to approve the transaction.'
                : ' This wallet cannot sign messages, so the secret is kept in this browser until you open the vote.'}
            </p>
          </>
        )}
      </StepShell>

      <StepShell
        index={2}
        title="Open your sealed vote"
        state={
          step === 'revealed'
            ? 'done'
            : revealOpen && step === 'committed'
              ? 'open'
              : stage === 'commit'
                ? 'waiting'
                : 'closed'
        }
      >
        {step === 'revealed' && vote.data?.choice ? (
          <p className="text-[13.5px] font-medium text-foreground">
            Opened: you voted for the {vote.data.choice === 'Claimant' ? 'claimant' : 'respondent'}
            ’s position. It counts toward the verdict.
          </p>
        ) : (
          <>
            <button
              type="button"
              disabled={!revealOpen || step !== 'committed' || busy !== null}
              onClick={open}
              className={actionClass}
            >
              {busy ??
                (stage === 'commit'
                  ? 'Opens when the reveal window starts'
                  : revealOpen
                    ? step === 'committed'
                      ? 'Open sealed vote'
                      : 'Nothing sealed to open'
                    : 'Reveal window closed')}
            </button>
            <p className="mt-3 text-[13px] font-medium leading-relaxed text-clock-urgent">
              A sealed vote that is never opened costs part of your stake — more than voting with
              the losing side.
            </p>
          </>
        )}
      </StepShell>

      {error ? (
        <p
          role="alert"
          className="rounded border border-clock-urgent/50 bg-clock-urgent/5 px-3.5 py-2.5 text-[12.5px] text-foreground"
        >
          {error}
        </p>
      ) : null}
      {lastSignature ? (
        <p className="flex flex-wrap items-center gap-2 text-[12px] text-muted-foreground">
          Last transaction{' '}
          <HashRef value={lastSignature} lead={6} tail={6} href={explorerTx(lastSignature)} />
        </p>
      ) : null}
    </div>
  )
}

export default VotePanel
