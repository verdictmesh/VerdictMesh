import { ChevronLeft } from 'lucide-react'
import { Link, useParams } from 'react-router-dom'
import HashRef from '@/components/HashRef'
import Shell from '@/components/Shell'
import { getHearing, SIGS, usd } from '@/lib/verdictmesh'

const LedgerRow = ({ label, note, amount }: { label: string; note: string; amount: string }) => (
  <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1 border-t border-border px-4 py-3 first:border-t-0">
    <div className="min-w-0">
      <p className="text-[13.5px] font-medium leading-snug text-foreground">{label}</p>
      <p className="mt-0.5 text-[12px] leading-snug text-muted-foreground">{note}</p>
    </div>
    <span className="mono tabular shrink-0 text-[13.5px] text-foreground">{amount}</span>
  </div>
)

const Settlement = () => {
  const { id } = useParams()
  const hearing = getHearing(id)

  if (hearing?.id !== 'VM-1039') {
    return (
      <Shell>
        <p className="text-[13.5px] text-muted-foreground">
          This demo carries one settled hearing.{' '}
          <Link to="/settlement/VM-1039" className="text-foreground underline">
            Open VM-1039
          </Link>
        </p>
      </Shell>
    )
  }

  return (
    <Shell>
      <div className="mb-5 border-b border-border pb-4">
        <Link
          to="/"
          className="focus-ring label-xs inline-flex items-center gap-1 text-muted-foreground transition-colors hover:text-foreground"
        >
          <ChevronLeft className="h-3 w-3" /> Juror panel
        </Link>
        <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-2">
          <h1 className="mono text-[15px] font-semibold tracking-[0.1em] text-foreground">
            {hearing.id}
          </h1>
          <span className="label-xs rounded-sm border border-confirmed/50 px-1.5 py-1 text-confirmed">
            Settled
          </span>
          <span className="label-xs text-unestablished">settlement receipt</span>
        </div>
        <p className="mt-2 text-[14.5px] leading-snug text-foreground">
          {hearing.dealParty}
          <span className="text-muted-foreground"> — {hearing.dealDetail}</span>
        </p>
      </div>

      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,340px)]">
        <div className="flex flex-col gap-6">
          <section>
            <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
              Verdict
            </h2>
            <div className="rounded border border-border bg-surface p-4">
              <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-2">
                <p className="text-[15px] font-semibold leading-snug text-foreground">
                  The buyer’s position won.
                </p>
                <span className="mono tabular text-[15px] font-medium text-foreground">2 — 1</span>
              </div>
              <p className="mt-2.5 text-[13px] leading-relaxed text-muted-foreground">
                “The order never arrived and the funds should be returned.” Three revealed votes, a
                two-vote majority. The losing position was: “The order was shipped as agreed and
                payment is due.”
              </p>
            </div>
          </section>

          <section>
            <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
              Money moved
            </h2>
            <div className="rounded border border-border bg-surface">
              <LedgerRow
                label="Disputed amount returned to the buyer"
                note="Released from escrow on the recorded verdict"
                amount={`+ ${usd(65)}`}
              />
              <LedgerRow
                label="Seller’s review bond reimburses the buyer’s deposit"
                note="The buyer paid 5.00 USDC to open the hearing; it is made whole"
                amount={`+ ${usd(5)}`}
              />
              <LedgerRow
                label="Buyer’s own review bond returned"
                note="Untouched — posted when the deal was signed"
                amount={`+ ${usd(5)}`}
              />
              <div className="border-t border-border-strong px-4 py-3">
                <div className="flex items-baseline justify-between">
                  <span className="label-xs text-muted-foreground">Settlement transaction</span>
                  <HashRef value={SIGS.settlement} />
                </div>
              </div>
            </div>
          </section>

          <section>
            <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
              The panel
            </h2>
            <div className="rounded border border-border bg-surface">
              <LedgerRow
                label="Seat 1 — agreed with the outcome"
                note="Earned from the review payment"
                amount={`+ ${usd(2)}`}
              />
              <LedgerRow
                label="Seat 3 — agreed with the outcome"
                note="Earned from the review payment"
                amount={`+ ${usd(2)}`}
              />
              <LedgerRow
                label="Seat 2 — voted the other way"
                note="10% of stake, the standard cost of a minority vote"
                amount={`− ${usd(10)}`}
              />
            </div>
            <p className="mt-2.5 text-[12px] leading-relaxed text-unestablished">
              Seats stay anonymous after settlement. Which way each one voted is recorded on-chain
              as a revealed choice; it is not attributed to a person here.
            </p>
          </section>
        </div>

        <aside className="flex flex-col gap-6">
          <section>
            <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
              Appeal window
            </h2>
            <div className="rounded border border-border bg-surface p-4">
              <div className="flex items-baseline justify-between">
                <span className="label-xs text-muted-foreground">Status</span>
                <span className="mono text-[12px] text-foreground">closed, unused</span>
              </div>
              <div className="mt-3 flex items-baseline justify-between border-t border-border pt-3">
                <span className="label-xs text-muted-foreground">Closed</span>
                <span className="mono tabular text-[12px] text-foreground">4 minutes ago</span>
              </div>
              <div className="mt-3 flex items-baseline justify-between border-t border-border pt-3">
                <span className="label-xs text-muted-foreground">Duration</span>
                <span className="mono tabular text-[12px] text-foreground">90s</span>
              </div>
              <p className="mt-3.5 border-t border-border pt-3 text-[12px] leading-relaxed text-muted-foreground">
                Neither side escalated to a wider panel. The verdict became final when the window
                closed.
              </p>
            </div>
          </section>

          <section>
            <h2 className="mb-2.5 text-[15px] font-semibold tracking-tight text-foreground">
              How the funds left escrow
            </h2>
            <div className="rounded border border-border-strong bg-surface p-4">
              <p className="text-[13.5px] leading-relaxed text-foreground">
                The escrow released the funds by reading the recorded verdict itself. No key held by
                the buyer, the seller, the panel or the team that built VerdictMesh could have
                directed the money anywhere else.
              </p>
            </div>
          </section>
        </aside>
      </div>
    </Shell>
  )
}

export default Settlement
