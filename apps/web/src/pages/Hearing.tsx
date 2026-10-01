import type { DisputeView } from '@verdictmesh/shared'
import { Link, useParams } from 'react-router-dom'
import DisputeHeader from '@/components/DisputeHeader'
import EvidenceReport from '@/components/EvidenceReport'
import HashRef from '@/components/HashRef'
import { Failure, Loading } from '@/components/LoadState'
import Shell from '@/components/Shell'
import VotePanel from '@/components/VotePanel'
import { explorerAddress, formatAmount } from '@/lib/dispute'
import { useDispute, useNow, useReport } from '@/lib/queries'

const Party = ({ side, address, note }: { side: string; address: string; note: string }) => (
  <div className="rounded border border-border bg-surface p-4">
    <div className="flex items-baseline justify-between gap-3">
      <span className="label-xs text-foreground">{side}</span>
      <HashRef value={address} href={explorerAddress(address)} />
    </div>
    <p className="mt-2.5 border-t border-border pt-2.5 text-[12px] leading-snug text-unestablished">
      {note}
    </p>
  </div>
)

const Body = ({ dispute, now }: { dispute: DisputeView; now: number }) => {
  const report = useReport(dispute.pda)
  const amount = formatAmount(dispute.amount)

  return (
    <>
      <DisputeHeader
        dispute={dispute}
        now={now}
        view="hearing"
        back={{ to: '/', label: 'Hearings' }}
        aside={
          <div className="flex gap-2">
            <Link
              to={`/hearing/${dispute.pda}/party`}
              className="focus-ring label-xs rounded-sm border border-border px-2.5 py-1.5 text-muted-foreground transition-colors hover:border-border-strong hover:text-foreground"
            >
              Status &amp; windows
            </Link>
            {dispute.verdict ? (
              <Link
                to={`/settlement/${dispute.pda}`}
                className="focus-ring label-xs rounded-sm border border-border px-2.5 py-1.5 text-muted-foreground transition-colors hover:border-border-strong hover:text-foreground"
              >
                Settlement
              </Link>
            ) : null}
          </div>
        }
      />

      <section className="mb-7">
        <div className="mb-2.5 flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
          <h2 className="text-[15px] font-semibold tracking-tight text-foreground">The parties</h2>
          <span className="label-xs text-muted-foreground">as recorded in the dispute account</span>
        </div>
        <div className="grid gap-2 md:grid-cols-2">
          <Party
            side="Claimant"
            address={dispute.claimant}
            note="Opened the dispute. Each side's position is fingerprinted on-chain; the report below reads both."
          />
          <Party
            side="Respondent"
            address={dispute.respondent}
            note="Neither side can edit, summarise or answer for the other."
          />
        </div>
      </section>

      <section className="mb-7">
        <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
          <h2 className="text-[15px] font-semibold tracking-tight text-foreground">
            Evidence report
          </h2>
          <span className="label-xs text-muted-foreground">
            machine-assembled from on-chain history
          </span>
        </div>

        <div className="rounded border border-border bg-surface p-4">
          <EvidenceReport report={report.data} error={report.error} />

          <div className="mt-6 border-t border-border pt-3.5">
            <span className="label-xs text-muted-foreground">What the money does</span>
            <p className="mt-2 text-[13.5px] leading-relaxed text-foreground">
              If the claimant’s position wins, {amount} goes to the claimant; if the respondent’s
              wins, it goes to the respondent. If the panel cannot reach a verdict, the status quo
              holds and nothing moves. The escrow pays out itself, by reading the recorded verdict —
              no key at VerdictMesh can direct the funds.
            </p>
          </div>
        </div>
      </section>

      <section>
        <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
          <h2 className="text-[15px] font-semibold tracking-tight text-foreground">Your vote</h2>
          <span className="label-xs text-muted-foreground">panel members only · two steps</span>
        </div>
        <VotePanel dispute={dispute} now={now} />
      </section>
    </>
  )
}

const Hearing = () => {
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

export default Hearing
