import HashRef from '@/components/HashRef'
import type { EvidenceReport as Report } from '@/lib/verdictmesh'

interface EvidenceReportProps {
  report: Report
}

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

const EvidenceReport = ({ report }: EvidenceReportProps) => (
  <div className="flex flex-col gap-6">
    <div>
      <ClassHeader
        title="Confirmed on-chain"
        definition="Facts anyone can verify. Each carries a transaction signature."
        tone="text-confirmed"
        count={report.confirmed.length}
      />
      <ul className="flex flex-col">
        {report.confirmed.map((item, i) => (
          <li
            key={item.text}
            className="flex flex-col gap-2 border-l-2 border-confirmed bg-surface-2/60 px-3.5 py-3 sm:flex-row sm:items-start sm:justify-between sm:gap-6"
            style={{ marginTop: i === 0 ? 0 : 2 }}
          >
            <p className="text-[13.5px] font-medium leading-relaxed text-foreground">{item.text}</p>
            <div className="shrink-0">
              <HashRef value={item.signature} />
            </div>
          </li>
        ))}
      </ul>
    </div>

    <div>
      <ClassHeader
        title="Claimed by a party"
        definition="Asserted by one side. Not verifiable on-chain."
        tone="text-claimed"
        count={report.claimed.length}
      />
      <ul className="flex flex-col">
        {report.claimed.map((item, i) => (
          <li
            key={item.text}
            className="flex flex-col gap-1 border-l-2 border-claimed bg-surface-2/40 px-3.5 py-3 sm:flex-row sm:items-baseline sm:gap-4"
            style={{ marginTop: i === 0 ? 0 : 2 }}
          >
            <span className="label-xs shrink-0 text-claimed sm:w-[104px]">{item.party} says</span>
            <p className="text-[13.5px] font-normal italic leading-relaxed text-foreground/90">
              {item.text}
            </p>
          </li>
        ))}
      </ul>
    </div>

    <div>
      <ClassHeader
        title="Not established"
        definition="The report looked and found nothing either way."
        tone="text-unestablished"
        count={report.unestablished.length}
      />
      <ul className="flex flex-col">
        {report.unestablished.map((item, i) => (
          <li
            key={item}
            className="border-l-2 border-dashed border-unestablished px-3.5 py-3"
            style={{ marginTop: i === 0 ? 0 : 2 }}
          >
            <p className="text-[13.5px] leading-relaxed text-unestablished">{item}</p>
          </li>
        ))}
      </ul>
    </div>
  </div>
)

export default EvidenceReport
