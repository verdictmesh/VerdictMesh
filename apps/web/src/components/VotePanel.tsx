import { Check, KeyRound, Lock } from 'lucide-react'
import { type ReactNode, useState } from 'react'
import HashRef from '@/components/HashRef'
import { sealVote } from '@/hooks/useHearingClock'
import { formatDuration, type Hearing, PENALTIES, type Phase } from '@/lib/verdictmesh'

interface VotePanelProps {
  hearing: Hearing
  phase: Phase
  msLeft: number
  urgent: boolean
}

/** A hearing has exactly two sides, and a vote is one of them. */
type SideIndex = 0 | 1

const SIDE_INDEXES: SideIndex[] = [0, 1]

const SEALED_FINGERPRINTS: Record<SideIndex, string> = {
  0: 'a91cf4Rk7bTsQm2NvXd8Lp3ZyHgWu6EoJt5BnCr9Ka1MsDx7Vq2Ph4f7e',
  1: 'c47bE8Zq2LmTf9XvNk3RdSy6WgUa1JoHt5BpCr7Kn4MsQx8Vd2Ph6b3a1c',
}
const LOCAL_SECRET = 'vm-secret-8f3c21ab9de74c05bf6a2d18e7c40593'

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
}) => {
  const tone =
    state === 'open'
      ? 'border-border-strong bg-surface'
      : state === 'done'
        ? 'border-confirmed/40 bg-surface'
        : 'border-border bg-surface/50'

  const stateLabel =
    state === 'open'
      ? 'Open now'
      : state === 'done'
        ? 'Done'
        : state === 'waiting'
          ? 'Not open yet'
          : 'Closed'

  return (
    <div className={`rounded border ${tone} p-4`}>
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
          {stateLabel}
        </span>
      </div>
      <div className={state === 'closed' || state === 'waiting' ? 'opacity-70' : ''}>
        {children}
      </div>
    </div>
  )
}

const VotePanel = ({ hearing, phase, msLeft, urgent }: VotePanelProps) => {
  const [choice, setChoice] = useState<SideIndex | null>(null)
  const [sealed, setSealed] = useState<SideIndex | null>(null)
  const [revealed, setRevealed] = useState(false)

  const commitOpen = phase === 'commit'
  const revealOpen = phase === 'reveal'

  const windowNotice = () => {
    if (phase === 'commit')
      return `The commit window is open — ${formatDuration(msLeft)} left. The reveal window opens next and runs for 60s.`
    if (phase === 'reveal')
      return `The commit window has closed. The reveal window is open — ${formatDuration(msLeft)} left.`
    if (phase === 'appeal')
      return 'Both voting windows have closed. The hearing is in the appeal window; no further votes are accepted.'
    return 'This hearing is settled. Voting is over.'
  }

  return (
    <div className="flex flex-col gap-3">
      <div
        className={`rounded border px-3.5 py-2.5 ${
          urgent && (commitOpen || revealOpen)
            ? 'border-clock-urgent/50 bg-clock-urgent/5'
            : 'border-border bg-surface-2/50'
        }`}
      >
        <p className="text-[13px] leading-relaxed text-foreground">{windowNotice()}</p>
      </div>

      {/* STEP ONE — COMMIT */}
      <StepShell
        index={1}
        title="Seal your vote"
        state={sealed !== null ? 'done' : commitOpen ? 'open' : 'closed'}
      >
        <div className="grid gap-2 sm:grid-cols-2">
          {SIDE_INDEXES.map((i) => {
            const side = hearing.sides[i]
            const active = (sealed ?? choice) === i
            const locked = sealed !== null || !commitOpen
            return (
              <button
                key={side.role}
                type="button"
                disabled={locked}
                onClick={() => setChoice(i)}
                className={`focus-ring rounded border px-3 py-2.5 text-left transition-colors ${
                  active
                    ? 'border-foreground bg-surface-2'
                    : 'border-border hover:border-border-strong'
                } ${locked && !active ? 'cursor-not-allowed opacity-50' : ''} ${
                  locked ? 'cursor-default' : ''
                }`}
              >
                <span className="label-xs text-muted-foreground">Vote for</span>
                <span className="mt-1.5 block text-[13.5px] font-medium text-foreground">
                  {side.role}’s position
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

        {sealed === null ? (
          <>
            <button
              type="button"
              disabled={!commitOpen || choice === null}
              onClick={() => {
                if (choice === null) return
                setSealed(choice)
                sealVote(hearing.id)
              }}
              className="focus-ring mt-3 w-full rounded border border-border-strong bg-surface-2 px-3 py-2.5 text-[13px] font-medium text-foreground transition-colors hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-45"
            >
              {commitOpen ? 'Seal choice' : 'Commit window closed'}
            </button>
            <p className="mt-3 text-[12.5px] leading-relaxed text-muted-foreground">
              Sealing hides your choice. Until the reveal window opens it is unreadable to everyone
              — the other two panel members, both parties, and the team that built VerdictMesh.
            </p>
          </>
        ) : (
          <div className="mt-3 flex flex-col gap-2.5 border-t border-border pt-3">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
              <span className="label-xs flex items-center gap-1.5 text-confirmed">
                <Lock className="h-3 w-3" /> sealed fingerprint
              </span>
              <HashRef value={SEALED_FINGERPRINTS[sealed]} lead={6} tail={6} />
            </div>
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
              <span className="label-xs flex items-center gap-1.5 text-muted-foreground">
                <KeyRound className="h-3 w-3" /> secret stored locally
              </span>
              <HashRef value={LOCAL_SECRET} lead={6} tail={6} tone="muted" />
            </div>
            <p className="text-[12.5px] leading-relaxed text-muted-foreground">
              The fingerprint is on-chain; the secret stayed in this browser. Step two cannot be
              completed without it.
            </p>
          </div>
        )}
      </StepShell>

      {/* STEP TWO — REVEAL */}
      <StepShell
        index={2}
        title="Open your sealed vote"
        state={revealed ? 'done' : revealOpen ? 'open' : phase === 'commit' ? 'waiting' : 'closed'}
      >
        {revealed ? (
          <div className="flex flex-col gap-2">
            <p className="text-[13.5px] font-medium text-foreground">
              Revealed: you voted for {hearing.sides[sealed ?? 0].role}’s position.
            </p>
            <p className="text-[12.5px] leading-relaxed text-muted-foreground">
              Your vote now counts toward the verdict. No stake penalty applies for revealing on
              time.
            </p>
          </div>
        ) : (
          <>
            <button
              type="button"
              disabled={!revealOpen || sealed === null}
              onClick={() => setRevealed(true)}
              className="focus-ring w-full rounded border border-border-strong bg-surface-2 px-3 py-2.5 text-[13px] font-medium text-foreground transition-colors hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-45"
            >
              {phase === 'commit'
                ? 'Opens when the reveal window starts'
                : revealOpen
                  ? sealed === null
                    ? 'Nothing sealed to open'
                    : 'Open sealed vote'
                  : 'Reveal window closed'}
            </button>
            <div className="mt-3 flex flex-col gap-1.5">
              <p className="text-[13px] font-medium leading-relaxed text-clock-urgent">
                Not revealing costs {PENALTIES.missedReveal * 100}% of your stake.
              </p>
              <p className="text-[12.5px] leading-relaxed text-muted-foreground">
                Voting with the losing side costs {PENALTIES.losingSide * 100}%. Silence is the
                worst outcome available to you — worse than being wrong.
              </p>
            </div>
          </>
        )}
      </StepShell>
    </div>
  )
}

export default VotePanel
