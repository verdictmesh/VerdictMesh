import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import StageChip from '@/components/StageChip'
import { readJuror } from '@/lib/chain'
import { formatAmount, shortenMiddle, stageOf } from '@/lib/dispute'
import { useDisputes } from '@/lib/queries'
import { useWallet } from '@/lib/wallet'

const Cell = ({ label, value }: { label: string; value: string }) => (
  <div className="flex min-w-[140px] flex-col gap-1.5 border-l border-border pl-4 first:border-l-0 first:pl-0">
    <span className="label-xs text-muted-foreground">{label}</span>
    <span className="mono tabular text-[17px] font-medium leading-none text-foreground">
      {value}
    </span>
  </div>
)

/**
 * The connected juror's standing (`FR-019`, `FR-028`): the stake as the
 * registry holds it on chain, and the hearings whose panel has this wallet.
 * Shown only with a wallet; the page is complete without one.
 */
const JurorStanding = ({ now }: { now: number }) => {
  const wallet = useWallet()
  const publicKey = wallet.publicKey
  const juror = useQuery({
    queryKey: ['juror', wallet.address],
    queryFn: () => (publicKey ? readJuror(publicKey) : null),
    enabled: publicKey !== null,
  })
  const seated = useDisputes(wallet.address ? { juror: wallet.address } : {})

  if (!wallet.address) return null

  const open = (seated.data ?? []).filter((d) => {
    const stage = stageOf(d, now)
    return stage === 'commit' || stage === 'reveal'
  })

  return (
    <section className="mb-7">
      <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-[15px] font-semibold tracking-tight text-foreground">Your standing</h2>
        <span className="label-xs mono text-unestablished">
          {shortenMiddle(wallet.address, 6, 6)}
        </span>
      </div>
      <div className="rounded border border-border bg-surface p-4">
        {juror.isLoading ? (
          <p className="label-xs animate-pulse text-unestablished">Reading the registry…</p>
        ) : juror.data ? (
          <div className="flex flex-wrap gap-x-8 gap-y-5">
            <Cell label="Stake" value={formatAmount(juror.data.stake.toString())} />
            <Cell label="Open hearings on chain" value={String(juror.data.activeDisputes)} />
            <Cell label="Voting now" value={String(open.length)} />
          </div>
        ) : (
          <p className="text-[13px] leading-relaxed text-muted-foreground">
            This wallet has no stake in the juror registry, so it is never drawn for a panel.
          </p>
        )}

        {open.length > 0 ? (
          <ul className="mt-4 flex flex-col gap-1.5 border-t border-border pt-3">
            {open.map((d) => (
              <li key={d.pda} className="flex flex-wrap items-center justify-between gap-2">
                <span className="mono text-[12px] text-foreground">
                  {shortenMiddle(d.pda, 6, 6)}
                </span>
                <span className="flex items-center gap-3">
                  <StageChip stage={stageOf(d, now)} />
                  <Link
                    to={`/hearing/${d.pda}`}
                    className="focus-ring label-xs rounded-sm border border-border-strong px-2.5 py-1.5 text-foreground hover:bg-surface-2"
                  >
                    Vote
                  </Link>
                </span>
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </section>
  )
}

export default JurorStanding
