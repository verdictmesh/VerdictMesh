import { ChevronLeft } from 'lucide-react'
import { Link, useParams } from 'react-router-dom'
import Countdown from '@/components/Countdown'
import PhaseChip from '@/components/PhaseChip'
import Shell from '@/components/Shell'
import { useHearingClock } from '@/hooks/useHearingClock'
import { formatDuration, getHearing, PHASE_LABEL, type Phase, usd } from '@/lib/verdictmesh'

type StepState = 'done' | 'now' | 'next'

interface Step {
  label: string
  detail: string
  when: string
  state: StepState
}

const ORDER: Phase[] = ['commit', 'reveal', 'appeal', 'settled']

function buildTimeline(phase: Phase, msLeft: number): Step[] {
  const idx = ORDER.indexOf(phase)
  const stateFor = (target: Phase): StepState => {
    const t = ORDER.indexOf(target)
    if (t < idx) return 'done'
    if (t === idx) return 'now'
    return 'next'
  }

  return [
    {
      label: 'Deal signed, funds locked',
      detail:
        '60.00 USDC locked across three milestones. Both sides posted a 5.00 USDC review bond.',
      when: '14 days ago',
      state: 'done',
    },
    {
      label: 'Milestone 1 released',
      detail: 'Released by the client without a hearing.',
      when: '9 days ago',
      state: 'done',
    },
    {
      label: 'Hearing opened',
      detail:
        'Milestone 2 marked disputed. A 5.00 USDC review deposit was paid by the party that opened it.',
      when: '3 minutes ago',
      state: 'done',
    },
    {
      label: 'Commit window — 60s',
      detail:
        'Each panel member seals a choice. Sealed choices are unreadable to everyone, including the panel.',
      when: stateFor('commit') === 'now' ? `${formatDuration(msLeft)} left` : 'closed',
      state: stateFor('commit'),
    },
    {
      label: 'Reveal window — 60s',
      detail:
        'Each panel member opens their sealed choice. A member who stays silent loses 20% of stake.',
      when:
        stateFor('reveal') === 'now'
          ? `${formatDuration(msLeft)} left`
          : stateFor('reveal') === 'next'
            ? 'starts when the commit window closes'
            : 'closed',
      state: stateFor('reveal'),
    },
    {
      label: 'Verdict recorded',
      detail:
        'The majority of revealed votes is written on-chain the moment the reveal window closes.',
      when: stateFor('appeal') === 'next' ? 'at the end of the reveal window' : 'recorded',
      state: stateFor('appeal') === 'next' ? 'next' : 'done',
    },
    {
      label: 'Appeal window — 90s',
      detail:
        'Either side may escalate to a wider panel by posting a second deposit. Nothing moves until this window closes.',
      when:
        stateFor('appeal') === 'now'
          ? `${formatDuration(msLeft)} left`
          : stateFor('appeal') === 'next'
            ? '90s once the verdict is recorded'
            : 'closed',
      state: stateFor('appeal'),
    },
    {
      label: 'Escrow pays out',
      detail:
        'The escrow reads the recorded verdict and releases the funds. No key can direct them elsewhere.',
      when: phase === 'settled' ? 'released' : 'immediately after the appeal window closes',
      state: phase === 'settled' ? 'done' : 'next',
    },
  ]
}

const HearingParty = () => {
  const { id } = useParams()
  const hearing = getHearing(id)
  const { phase, msLeft, urgent } = useHearingClock(id)

  if (!hearing) {
    return (
      <Shell>
        <p className="text-[13.5px] text-muted-foreground">
          No such hearing.{' '}
          <Link to="/" className="text-foreground underline">
            Back to the juror panel
          </Link>
        </p>
      </Shell>
    )
  }

  const sealed = phase === 'commit' ? hearing.sealed : 3
  const revealed = phase === 'commit' ? 0 : phase === 'reveal' ? hearing.revealed : 3
  const seats = [0, 1, 2].map((i) => ({
    name: `Seat ${i + 1}`,
    state: i < revealed ? 'revealed' : i < sealed ? 'sealed' : 'waiting',
  }))

  const timeline = buildTimeline(phase, msLeft)

  return (
    <Shell>
      <div className="mb-5 flex flex-wrap items-start justify-between gap-4 border-b border-border pb-4">
        <div className="min-w-0">
          <Link
            to={`/hearing/${hearing.id}`}
            className="focus-ring label-xs inline-flex items-center gap-1 text-muted-foreground transition-colors hover:text-foreground"
          >
            <ChevronLeft className="h-3 w-3" /> Juror view
          </Link>
          <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-2">
            <h1 className="mono text-[15px] font-semibold tracking-[0.1em] text-foreground">
              {hearing.id}
            </h1>
            <PhaseChip phase={phase} />
            <span className="label-xs text-unestablished">party view · no sign-in</span>
          </div>
          <p className="mt-2 text-[14.5px] leading-snug text-foreground">
            {hearing.dealParty}
            <span className="text-muted-foreground"> — {hearing.dealDetail}</span>
          </p>
          <p className="mono tabular mt-1.5 text-[13px] text-foreground">
            {usd(hearing.amount)}
            <span className="ml-1.5 text-[11px] text-muted-foreground">
              in dispute · opened by {hearing.openedBy}
            </span>
          </p>
        </div>
        <div className="text-right">
          <Countdown
            msLeft={msLeft}
            urgent={urgent}
            size="lg"
            label={`${PHASE_LABEL[phase]} ends in`}
          />
        </div>
      </div>

      <div className="grid gap-6 lg:grid-cols-[minmax(0,300px)_minmax(0,1fr)]">
        <section>
          <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
            The panel
          </h2>
          <div className="rounded border border-border bg-surface p-4">
            <div className="flex items-baseline justify-between">
              <span className="label-xs text-muted-foreground">Sealed</span>
              <span className="mono tabular text-[17px] font-medium text-foreground">
                {sealed} / 3
              </span>
            </div>
            <div className="mt-3 flex items-baseline justify-between border-t border-border pt-3">
              <span className="label-xs text-muted-foreground">Revealed</span>
              <span className="mono tabular text-[17px] font-medium text-foreground">
                {revealed} / 3
              </span>
            </div>

            <ul className="mt-4 flex flex-col gap-1.5 border-t border-border pt-3.5">
              {seats.map((seat) => (
                <li key={seat.name} className="flex items-center justify-between">
                  <span className="mono text-[12px] text-foreground">{seat.name}</span>
                  <span
                    className={`label-xs ${
                      seat.state === 'revealed'
                        ? 'text-confirmed'
                        : seat.state === 'sealed'
                          ? 'text-claimed'
                          : 'text-unestablished'
                    }`}
                  >
                    {seat.state}
                  </span>
                </li>
              ))}
            </ul>

            <p className="mt-4 border-t border-border pt-3 text-[12px] leading-relaxed text-muted-foreground">
              Which way any seat voted is never shown — not before the reveal, not after. That
              secrecy is what keeps the votes independent, and it is not withheld out of politeness.
            </p>
          </div>
        </section>

        <section>
          <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
            What happens next
          </h2>
          <ol className="rounded border border-border bg-surface">
            {timeline.map((step, i) => (
              <li
                key={step.label}
                className={`flex gap-3.5 px-4 py-3 ${
                  i > 0 ? 'border-t border-border' : ''
                } ${step.state === 'now' ? 'bg-surface-2' : ''}`}
              >
                <span
                  className={`mt-[7px] h-1.5 w-1.5 shrink-0 rounded-full ${
                    step.state === 'done'
                      ? 'bg-confirmed'
                      : step.state === 'now'
                        ? 'bg-claimed'
                        : 'bg-unestablished/50'
                  }`}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
                    <span
                      className={`text-[13.5px] font-medium leading-snug ${
                        step.state === 'next' ? 'text-muted-foreground' : 'text-foreground'
                      }`}
                    >
                      {step.label}
                    </span>
                    <span
                      className={`mono tabular text-[11px] ${
                        step.state === 'now' ? 'text-claimed' : 'text-unestablished'
                      }`}
                    >
                      {step.when}
                    </span>
                  </div>
                  <p className="mt-1 text-[12.5px] leading-relaxed text-muted-foreground">
                    {step.detail}
                  </p>
                </div>
              </li>
            ))}
          </ol>

          {phase === 'settled' ? (
            <Link
              to={`/settlement/${hearing.id}`}
              className="focus-ring label-xs mt-3 inline-block rounded-sm border border-border-strong px-2.5 py-1.5 text-foreground transition-colors hover:bg-surface-2"
            >
              Open settlement receipt
            </Link>
          ) : null}
        </section>
      </div>
    </Shell>
  )
}

export default HearingParty
