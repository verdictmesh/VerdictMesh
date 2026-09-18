/**
 * VerdictMesh demo data.
 * Everything here is invented and hardcoded. Nothing in this module touches the network.
 */

export type Phase = 'commit' | 'reveal' | 'appeal' | 'settled'

export const PHASE_LABEL: Record<Phase, string> = {
  commit: 'Commit window',
  reveal: 'Reveal window',
  appeal: 'Awaiting appeal window',
  settled: 'Settled',
}

export const PHASE_DURATION_MS: Record<Phase, number> = {
  commit: 60_000,
  reveal: 60_000,
  appeal: 90_000,
  settled: 0,
}

export interface Party {
  /** e.g. "Contractor" */
  role: string
  position: string
}

export interface Hearing {
  id: string
  deal: string
  dealParty: string
  dealDetail: string
  amount: number
  openedBy: string
  phase: Phase
  secondsLeft: number
  sides: [Party, Party]
  payout: string
  /** demo panel progress, 3 seats */
  sealed: number
  revealed: number
}

/** Full signatures / addresses. Displayed shortened, expanded on click. */
export const SIGS = {
  dealOpened: '5xQmH7bTf9vLnKcRa2wYs4XgQ8pZtVuNdE6mJhAr1o3B9sLcFyWq7TgKn2vX8Kdp',
  milestone1: '2RncV8kLpQzYb3TmXf7dHsNa9wEgUj4RtBc6vLoKi1sZyPq5MnDrW2hXtFuGvWq4t',
  disputeOpened: '7JhsK3pRvTqY9mXbNc2dFgLwUe8ZaSo5ItHj1rQkPy4VnBu6DxWfMz7CgAtEsLm2v',
  settlement: '9TpvR4nKmQb7YsXc2fLdHg8WaUe5ZoSj1ItRy6QkPn3VuBx9DwFmZt2CgAqEsRb8x',
} as const

export interface EvidenceConfirmed {
  text: string
  signature: string
}

export interface EvidenceClaimed {
  party: string
  text: string
}

export interface EvidenceReport {
  confirmed: EvidenceConfirmed[]
  claimed: EvidenceClaimed[]
  unestablished: string[]
}

export const HEARINGS: Hearing[] = [
  {
    id: 'VM-1041',
    deal: 'Halcyon Grid',
    dealParty: 'Halcyon Grid',
    dealDetail: 'bandwidth settlement, week 34',
    amount: 120,
    openedBy: 'Node operator',
    phase: 'reveal',
    secondsLeft: 18,
    sides: [
      {
        role: 'Node operator',
        position: 'The metered bandwidth was served and the settlement is owed.',
      },
      {
        role: 'Grid operator',
        position: 'The metered totals are overstated and the settlement should be reduced.',
      },
    ],
    payout:
      'If the node operator’s position wins, 120.00 USDC goes to the node operator and the grid operator’s 5.00 USDC bond reimburses the node operator’s deposit; if the grid operator’s position wins, 120.00 USDC returns to the grid operator and both bonds go back untouched.',
    sealed: 3,
    revealed: 1,
  },
  {
    id: 'VM-1039',
    deal: 'Tessera Market',
    dealParty: 'Tessera Market',
    dealDetail: 'order #8813, undelivered goods',
    amount: 65,
    openedBy: 'Buyer',
    phase: 'reveal',
    secondsLeft: 52,
    sides: [
      { role: 'Buyer', position: 'The order never arrived and the funds should be returned.' },
      { role: 'Seller', position: 'The order was shipped as agreed and payment is due.' },
    ],
    payout:
      'If the buyer’s position wins, 65.00 USDC returns to the buyer and the seller’s 5.00 USDC bond reimburses the buyer’s deposit; if the seller’s position wins, 65.00 USDC goes to the seller and both bonds go back untouched.',
    sealed: 3,
    revealed: 2,
  },
  {
    id: 'VM-1042',
    deal: 'Meridian Works',
    dealParty: 'Meridian Works',
    dealDetail: 'website rebuild, milestone 2 of 3',
    amount: 20,
    openedBy: 'Contractor',
    phase: 'commit',
    secondsLeft: 41,
    sides: [
      { role: 'Contractor', position: 'The milestone was delivered and payment is due.' },
      {
        role: 'Client',
        position: 'The milestone was not delivered and the funds should be returned.',
      },
    ],
    payout:
      'If the contractor’s position wins, 20.00 USDC goes to the contractor and the client’s 5.00 USDC bond reimburses the contractor’s deposit; if the client’s position wins, 20.00 USDC returns to the client and both bonds go back untouched.',
    sealed: 1,
    revealed: 0,
  },
  {
    id: 'VM-1036',
    deal: 'Vantage Labs',
    dealParty: 'Vantage Labs',
    dealDetail: 'audit retainer, milestone 1 of 2',
    amount: 250,
    openedBy: 'Client',
    phase: 'appeal',
    secondsLeft: 74,
    sides: [
      {
        role: 'Client',
        position: 'The retainer work was not started and the funds should be returned.',
      },
      { role: 'Auditor', position: 'The retainer was consumed as scheduled and payment is due.' },
    ],
    payout:
      'If the client’s position wins, 250.00 USDC returns to the client and the auditor’s 5.00 USDC bond reimburses the client’s deposit; if the auditor’s position wins, 250.00 USDC goes to the auditor and both bonds go back untouched.',
    sealed: 3,
    revealed: 3,
  },
]

export const REPORTS: Record<string, EvidenceReport> = {
  'VM-1042': {
    confirmed: [
      {
        text: 'Deal opened 14 days ago; 60.00 USDC locked across three milestones of 10.00, 20.00 and 30.00 USDC.',
        signature: SIGS.dealOpened,
      },
      {
        text: 'Milestone 1 released to the contractor 9 days ago, by the client, without a hearing.',
        signature: SIGS.milestone1,
      },
      {
        text: 'Milestone 2 marked disputed 3 minutes ago, by the contractor.',
        signature: SIGS.disputeOpened,
      },
      {
        text: 'A review deposit of 5.00 USDC was paid by the contractor when the hearing opened.',
        signature: SIGS.disputeOpened,
      },
      {
        text: 'Both sides posted a 5.00 USDC review bond per milestone when the deal was signed.',
        signature: SIGS.dealOpened,
      },
    ],
    claimed: [
      { party: 'Contractor', text: 'Delivery was handed over off-chain on day 11.' },
      { party: 'Client', text: 'Two revisions requested on day 12 went unanswered.' },
    ],
    unestablished: [
      'Whether anything was delivered off-chain. No on-chain trace exists either way.',
    ],
  },
}

export const JUROR = {
  stake: 100,
  seated: 3,
  agreed: 12,
  pastHearings: 13,
  disagreementCost: 10,
  /** demo juror seat identity, shown as a seat not an address */
  seat: 'Seat 2',
}

export const PENALTIES = {
  missedReveal: 0.2,
  losingSide: 0.1,
}

export function getHearing(id: string | undefined): Hearing | undefined {
  return HEARINGS.find((h) => h.id === id)
}

export function usd(amount: number): string {
  return `${amount.toFixed(2)} USDC`
}

export function shortenMiddle(value: string, lead = 4, tail = 4): string {
  if (value.length <= lead + tail + 1) return value
  return `${value.slice(0, lead)}…${value.slice(-tail)}`
}

export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000))
  const m = Math.floor(total / 60)
  const s = total % 60
  if (m > 0) return `${m}m ${String(s).padStart(2, '0')}s`
  return `${s}s`
}
