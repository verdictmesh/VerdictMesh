import type { FactFindingReport, ReportResponse } from '@verdictmesh/shared'
import { ShieldAlert, ShieldCheck } from 'lucide-react'
import HashRef from '@/components/HashRef'
import { Failure, Loading } from '@/components/LoadState'
import { explorerAddress, explorerTx } from '@/lib/dispute'

const ClassHeader = ({
  title,
  definition,
  tone,
  count,
}: {
  title: string
  definition: string
  tone: string
  count: number
}) => (
  <div className="mb-2.5 flex items-baseline justify-between gap-4">
    <div>
      <h4 className={`mono text-[12px] font-semibold uppercase tracking-[0.16em] ${tone}`}>
        {title}
      </h4>
      <p className="mt-1 text-[12px] leading-snug text-muted-foreground">{definition}</p>
    </div>
    <span className="mono tabular text-[11px] text-unestablished">{count}</span>
  </div>
)

/** Where a fact can be checked: a transaction, else an account. */
const Source = ({ signature, account }: { signature?: string; account?: string }) => {
  if (signature) return <HashRef value={signature} href={explorerTx(signature)} />
  if (account) return <HashRef value={account} href={explorerAddress(account)} />
  return <span className="label-xs text-unestablished">no source</span>
}

const FACT_CLASSES = [
  {
    verdict: 'confirmed',
    title: 'Confirmed on-chain',
    definition: 'Facts anyone can verify. Each carries its transaction or account.',
    tone: 'text-confirmed',
    border: 'border-confirmed',
  },
  {
    verdict: 'contradicted',
    title: 'Contradicted on-chain',
    definition: 'Statements the chain shows to be false.',
    tone: 'text-clock-urgent',
    border: 'border-clock-urgent',
  },
  {
    verdict: 'unconfirmed',
    title: 'Not confirmed',
    definition: 'Looked for on-chain and not found either way.',
    tone: 'text-unestablished',
    border: 'border-unestablished border-dashed',
  },
] as const

const ASSESSMENT_TONE = {
  supported: 'text-confirmed',
  unsupported: 'text-unestablished',
  contradicted: 'text-clock-urgent',
} as const

const PARTY_LABEL = { claimant: 'Claimant', respondent: 'Respondent' } as const

const ReportBody = ({ report }: { report: FactFindingReport }) => (
  <div className="flex flex-col gap-6">
    <p className="text-[14px] leading-relaxed text-foreground">{report.summary}</p>

    {FACT_CLASSES.map((cls) => {
      const facts = report.facts.filter((fact) => fact.verdict === cls.verdict)
      if (facts.length === 0 && cls.verdict !== 'confirmed') return null
      return (
        <div key={cls.verdict}>
          <ClassHeader
            title={cls.title}
            definition={cls.definition}
            tone={cls.tone}
            count={facts.length}
          />
          <ul className="flex flex-col gap-0.5">
            {facts.map((fact) => (
              <li
                key={fact.statement}
                className={`flex flex-col gap-2 border-l-2 ${cls.border} bg-surface-2/60 px-3.5 py-3 sm:flex-row sm:items-start sm:justify-between sm:gap-6`}
              >
                <p className="text-[13.5px] font-medium leading-relaxed text-foreground">
                  {fact.statement}
                </p>
                <div className="shrink-0">
                  <Source
                    {...(fact.sourceSignature ? { signature: fact.sourceSignature } : {})}
                    {...(fact.sourceAccount ? { account: fact.sourceAccount } : {})}
                  />
                </div>
              </li>
            ))}
          </ul>
        </div>
      )
    })}

    <div>
      <ClassHeader
        title="Claimed by a party"
        definition="What each side asserts, and how the record bears on it."
        tone="text-claimed"
        count={report.claims.length}
      />
      <ul className="flex flex-col gap-0.5">
        {report.claims.map((claim) => (
          <li
            key={`${claim.party}:${claim.statement}`}
            className="flex flex-col gap-1 border-l-2 border-claimed bg-surface-2/40 px-3.5 py-3 sm:flex-row sm:items-baseline sm:gap-4"
          >
            <span className="label-xs shrink-0 text-claimed sm:w-[104px]">
              {PARTY_LABEL[claim.party]} says
            </span>
            <p className="flex-1 text-[13.5px] italic leading-relaxed text-foreground/90">
              {claim.statement}
            </p>
            <span className={`label-xs shrink-0 ${ASSESSMENT_TONE[claim.assessment]}`}>
              {claim.assessment}
            </span>
          </li>
        ))}
      </ul>
    </div>

    <div>
      <ClassHeader
        title="Not established"
        definition="What the record does not show — said out loud instead of guessed."
        tone="text-unestablished"
        count={report.gaps.length}
      />
      <ul className="flex flex-col gap-0.5">
        {report.gaps.map((gap) => (
          <li key={gap} className="border-l-2 border-dashed border-unestablished px-3.5 py-3">
            <p className="text-[13.5px] leading-relaxed text-unestablished">{gap}</p>
          </li>
        ))}
      </ul>
    </div>
  </div>
)

const UNAVAILABLE_TEXT = {
  model_unavailable:
    'The model that writes reports is not answering. The service keeps trying until the commit window closes.',
  generation_failed:
    'The model answered, but not with a report that fits the contract, and the attempts ran out.',
  window_closed: 'The commit window closed before a report was attested. None will be.',
  escalated: 'The hearing went to a wider panel before a report was attested. None will be.',
  body_missing:
    'A report fingerprint is on-chain, but the service does not hold its text. It cannot be shown.',
} as const

interface EvidenceReportProps {
  report: ReportResponse | undefined
  error: unknown
}

/**
 * The report and whether it is the one promised on-chain (`FR-017b`), or why
 * there is none (`FR-018`). The hearing goes on either way — said here so a
 * juror does not wait for something that will not come.
 */
const EvidenceReport = ({ report, error }: EvidenceReportProps) => {
  if (error) return <Failure error={error} />
  if (!report) return <Loading what="the evidence report" />

  if (report.status === 'pending') {
    return (
      <p className="text-[13.5px] leading-relaxed text-muted-foreground">
        The report is being assembled from the on-chain history. It usually takes under a minute.
      </p>
    )
  }

  if (report.status === 'unavailable') {
    return (
      <div className="flex flex-col gap-1.5">
        <p className="text-[13.5px] font-medium text-foreground">
          {report.final ? 'There is no report for this hearing.' : 'The report is delayed.'}
        </p>
        <p className="text-[12.5px] leading-relaxed text-muted-foreground">
          {UNAVAILABLE_TEXT[report.reason]} The hearing does not wait for it: the panel votes on the
          record either way.
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-5">
      <div
        className={`flex flex-wrap items-center gap-x-3 gap-y-2 rounded border px-3.5 py-2.5 ${
          report.matchesOnchain
            ? 'border-confirmed/40 bg-confirmed/5'
            : 'border-clock-urgent/50 bg-clock-urgent/5'
        }`}
      >
        {report.matchesOnchain ? (
          <ShieldCheck className="h-4 w-4 text-confirmed" />
        ) : (
          <ShieldAlert className="h-4 w-4 text-clock-urgent" />
        )}
        <span className="text-[13px] text-foreground">
          {report.matchesOnchain
            ? 'This text matches the fingerprint recorded on-chain.'
            : 'This text does not match any fingerprint on-chain. Do not rely on it.'}
        </span>
        <HashRef value={report.hash} lead={6} tail={6} tone="muted" />
      </div>
      <ReportBody report={report.report} />
    </div>
  )
}

export default EvidenceReport
