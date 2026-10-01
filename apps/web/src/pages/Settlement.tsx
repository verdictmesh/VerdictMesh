import type { DisputeView } from '@verdictmesh/shared'
import type { ReactNode } from 'react'
import { useParams } from 'react-router-dom'
import DisputeHeader from '@/components/DisputeHeader'
import HashRef from '@/components/HashRef'
import { Failure, Loading } from '@/components/LoadState'
import Shell from '@/components/Shell'
import {
  explorerAddress,
  explorerTx,
  formatAgo,
  formatAmount,
  recipientOf,
  stageOf,
  VERDICT_LABEL,
} from '@/lib/dispute'
import { useDispute, useNow } from '@/lib/queries'

const Row = ({ label, children }: { label: string; children: ReactNode }) => (
  <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1 border-t border-border px-4 py-3 first:border-t-0">
    <span className="label-xs text-muted-foreground">{label}</span>
    <span className="mono tabular text-[13px] text-foreground">{children}</span>
  </div>
)

const Body = ({ dispute, now }: { dispute: DisputeView; now: number }) => {
  const stage = stageOf(dispute, now)
  const recipient = recipientOf(dispute)
  const { settlement } = dispute

  return (
    <>
      <DisputeHeader
        dispute={dispute}
        now={now}
        view="settlement receipt"
        back={{ to: `/hearing/${dispute.pda}`, label: 'Hearing' }}
      />

      {dispute.verdict === null ? (
        <p className="text-[13.5px] text-muted-foreground">
          No verdict yet — there is nothing to settle. The receipt appears once the panel has
          decided.
        </p>
      ) : (
        <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,340px)]">
          <div className="flex flex-col gap-6">
            <section>
              <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
                Verdict
              </h2>
              <div className="rounded border border-border bg-surface p-4">
                <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-2">
                  <p className="text-[15px] font-semibold leading-snug text-foreground">
                    {VERDICT_LABEL[dispute.verdict]}
                  </p>
                  <span className="mono tabular text-[15px] font-medium text-foreground">
                    {dispute.votesClaimant} — {dispute.votesRespondent}
                  </span>
                </div>
                <p className="mt-2.5 text-[13px] leading-relaxed text-muted-foreground">
                  Revealed votes for the claimant and for the respondent, across{' '}
                  {dispute.escalated ? 'both rounds' : 'the round'}, of {dispute.panel.length}{' '}
                  seats.
                </p>
              </div>
            </section>

            <section>
              <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
                Money moved
              </h2>
              <div className="rounded border border-border bg-surface">
                <Row label="Amount in dispute">{formatAmount(dispute.amount)}</Row>
                <Row label="Paid to">
                  {recipient ? (
                    <HashRef value={recipient} href={explorerAddress(recipient)} />
                  ) : (
                    'nobody — the status quo holds'
                  )}
                </Row>
                <div className="border-t border-border-strong px-4 py-3">
                  <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <span className="label-xs text-muted-foreground">Settlement transaction</span>
                    {settlement.status === 'settled' ? (
                      <HashRef
                        value={settlement.signature}
                        lead={6}
                        tail={6}
                        href={explorerTx(settlement.signature)}
                      />
                    ) : (
                      <span className="mono text-[12px] text-unestablished">
                        {settlement.status === 'untracked'
                          ? 'not tracked by this service'
                          : stage === 'appeal'
                            ? 'after the appeal window'
                            : 'not yet sent'}
                      </span>
                    )}
                  </div>
                </div>
              </div>
              <p className="mt-2.5 text-[12px] leading-relaxed text-unestablished">
                Review bonds, the opening deposit and the panel’s stake movements are in the same
                transaction and in the stake settlement — open them on the explorer.
              </p>
            </section>
          </div>

          <aside className="flex flex-col gap-6">
            <section>
              <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
                Appeal window
              </h2>
              <div className="rounded border border-border bg-surface">
                <Row label="Status">{stage === 'appeal' ? 'open' : 'closed'}</Row>
                <Row label={stage === 'appeal' ? 'Closes' : 'Closed'}>
                  {stage === 'appeal'
                    ? 'see the clock above'
                    : formatAgo(dispute.appealDeadline, now)}
                </Row>
              </div>
            </section>

            <section>
              <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
                How the funds leave escrow
              </h2>
              <div className="rounded border border-border-strong bg-surface p-4">
                <p className="text-[13.5px] leading-relaxed text-foreground">
                  The escrow releases the funds by reading the recorded verdict itself. No key held
                  by either side, the panel or the team that built VerdictMesh can direct the money
                  anywhere else.
                </p>
              </div>
            </section>
          </aside>
        </div>
      )}
    </>
  )
}

const Settlement = () => {
  const { id = '' } = useParams()
  const now = useNow()
  const { data, error } = useDispute(id)

  return (
    <Shell>
      {error ? (
        <Failure error={error} />
      ) : !data ? (
        <Loading what="the settlement" />
      ) : (
        <Body dispute={data} now={now} />
      )}
    </Shell>
  )
}

export default Settlement
