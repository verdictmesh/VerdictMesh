import type { DisputeView } from '@verdictmesh/shared'
import { useParams } from 'react-router-dom'
import DisputeHeader from '@/components/DisputeHeader'
import HashRef from '@/components/HashRef'
import { Failure, Loading } from '@/components/LoadState'
import Shell from '@/components/Shell'
import { explorerTx } from '@/lib/dispute'
import { useDispute, useNow } from '@/lib/queries'
import { buildTimeline } from '@/lib/timeline'

const Panel = ({ dispute }: { dispute: DisputeView }) => {
  const seats = dispute.panel.length
  const revealed = dispute.votesClaimant + dispute.votesRespondent

  return (
    <div className="rounded border border-border bg-surface p-4">
      <div className="flex items-baseline justify-between">
        <span className="label-xs text-muted-foreground">Seats</span>
        <span className="mono tabular text-[17px] font-medium text-foreground">{seats}</span>
      </div>
      <div className="mt-3 flex items-baseline justify-between border-t border-border pt-3">
        <span className="label-xs text-muted-foreground">Revealed</span>
        <span className="mono tabular text-[17px] font-medium text-foreground">
          {revealed} / {seats}
        </span>
      </div>
      <p className="mt-4 border-t border-border pt-3 text-[12px] leading-relaxed text-muted-foreground">
        Which way any seat voted is never shown. Only the totals are counted on-chain, and only
        after each vote is opened — that secrecy is what keeps the votes independent.
      </p>
    </div>
  )
}

const Body = ({ dispute, now }: { dispute: DisputeView; now: number }) => {
  const timeline = buildTimeline(dispute, now)

  return (
    <>
      <DisputeHeader
        dispute={dispute}
        now={now}
        view="party view · no sign-in"
        back={{ to: `/hearing/${dispute.pda}`, label: 'Hearing' }}
      />

      <div className="grid gap-6 lg:grid-cols-[minmax(0,300px)_minmax(0,1fr)]">
        <section>
          <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
            The panel
          </h2>
          <Panel dispute={dispute} />
        </section>

        <section>
          <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
            Where the hearing is
          </h2>
          <ol className="rounded border border-border bg-surface">
            {timeline.map((step, i) => (
              <li
                key={step.key}
                className={`flex gap-3.5 px-4 py-3 ${i > 0 ? 'border-t border-border' : ''} ${
                  step.state === 'now' ? 'bg-surface-2' : ''
                }`}
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
                  {step.signature ? (
                    <div className="mt-2">
                      <HashRef
                        value={step.signature}
                        lead={6}
                        tail={6}
                        href={explorerTx(step.signature)}
                      />
                    </div>
                  ) : null}
                </div>
              </li>
            ))}
          </ol>
        </section>
      </div>
    </>
  )
}

const PartyView = () => {
  const { id = '' } = useParams()
  const now = useNow()
  const { data, error } = useDispute(id)

  return (
    <Shell>
      {error ? (
        <Failure error={error} />
      ) : !data ? (
        <Loading what="the hearing" />
      ) : (
        <Body dispute={data} now={now} />
      )}
    </Shell>
  )
}

export default PartyView
