import type { Scenario } from './scenario.js'

/**
 * The modelled half of the `SC-007` set — 17 cases; the other three are
 * disputes frozen from devnet (`fixtures/disputes/18-…` to `20-…`).
 *
 * Every case is built around the mistakes a report on this escrow is most
 * likely to make: milestone indices against ordinals, the milestone amount
 * against the total and the bond, the claimant's role, the earlier fate of a
 * milestone, history the collector did not see. Amounts are base units of a
 * six-decimal token unless the case says otherwise.
 */

const DAY = 86_400
const T0 = 1_790_000_000

export const scenarios: Scenario[] = [
  {
    id: '01-buyer-first-milestone',
    title: 'The buyer disputes the first of two milestones; nothing was released before.',
    traps: [
      'Milestone 0 called "milestone 1" with the amount of index 1.',
      'The claimant called the seller.',
      'The bond or the deposit given as the disputed amount.',
    ],
    milestones: [40_000_000n, 60_000_000n],
    bond: 5_000_000n,
    deposit: 5_000_000n,
    dealId: 1n,
    history: [],
    dispute: { milestone: 0, claimant: 'buyer' },
    createdAt: T0,
    spacing: 3 * DAY,
  },
  {
    id: '02-seller-last-of-three',
    title: 'The seller disputes the third milestone after the first two were released.',
    traps: [
      '"Milestone 3" for index 2, or the amount of index 1 for the disputed one.',
      'Released milestones described as disputed or pending.',
      'The total given as still locked.',
    ],
    milestones: [10_000_000n, 25_000_000n, 65_000_000n],
    bond: 2_000_000n,
    deposit: 5_000_000n,
    dealId: 2n,
    history: [{ release: 0 }, { release: 1 }],
    dispute: { milestone: 2, claimant: 'seller' },
    createdAt: T0 + 1 * DAY,
    spacing: [5 * DAY, 9 * DAY, 4 * DAY],
  },
  {
    id: '03-buyer-after-paying-first',
    title: 'The buyer disputes milestone 1 after releasing milestone 0 to the seller.',
    traps: [
      'Milestone 0 described as refunded or disputed.',
      'The release attributed to the seller.',
    ],
    milestones: [30_000_000n, 30_000_000n, 40_000_000n],
    bond: 3_000_000n,
    deposit: 5_000_000n,
    dealId: 3n,
    history: [{ release: 0 }],
    dispute: { milestone: 1, claimant: 'buyer' },
    createdAt: T0 + 2 * DAY,
    spacing: [7 * DAY, 2 * DAY],
  },
  {
    id: '04-earlier-dispute-won-by-seller',
    title:
      'An earlier dispute over milestone 0, opened by the seller, was won by the seller; now the buyer disputes milestone 1.',
    traps: [
      'The earlier verdict reported as a win for the buyer, or milestone 0 as refunded.',
      'The earlier dispute confused with the current one.',
      'The reimbursement given as the milestone amount.',
    ],
    milestones: [20_000_000n, 50_000_000n],
    bond: 5_000_000n,
    deposit: 5_000_000n,
    dealId: 4n,
    history: [{ dispute: 0, claimant: 'seller', verdict: 'Claimant' }],
    dispute: { milestone: 1, claimant: 'buyer' },
    createdAt: T0 + 3 * DAY,
    spacing: [10 * DAY, 4 * DAY, 6 * DAY],
  },
  {
    id: '05-second-dispute-after-status-quo',
    title:
      'The same milestone is disputed a second time: the first dispute ended in status quo and returned it to Pending.',
    traps: [
      'The milestone said to be settled, paid or closed already.',
      'A winner named for the earlier dispute (there was none).',
      'The zero amount of the earlier settlement read as the milestone amount.',
    ],
    milestones: [15_000_000n, 35_000_000n, 50_000_000n],
    bond: 5_000_000n,
    deposit: 5_000_000n,
    dealId: 5n,
    history: [{ release: 0 }, { dispute: 1, claimant: 'seller', verdict: 'StatusQuo' }],
    dispute: { milestone: 1, claimant: 'seller' },
    createdAt: T0 + 4 * DAY,
    spacing: [3 * DAY, 6 * DAY, 5 * DAY, 1 * DAY],
  },
  {
    id: '06-earlier-claimant-lost',
    title:
      'The seller lost an earlier dispute over milestone 0 (refunded to the buyer); now the seller disputes milestone 1.',
    traps: [
      'The earlier dispute reported as won by the seller, or the seller reimbursed.',
      'Milestone 0 described as released.',
    ],
    milestones: [25_000_000n, 25_000_000n],
    bond: 4_000_000n,
    deposit: 5_000_000n,
    dealId: 6n,
    history: [{ dispute: 0, claimant: 'seller', verdict: 'Respondent' }],
    dispute: { milestone: 1, claimant: 'seller' },
    createdAt: T0 + 5 * DAY,
    spacing: [8 * DAY, 4 * DAY, 3 * DAY],
  },
  {
    id: '07-eight-distinct-milestones',
    title: 'Eight milestones of different sizes; five released, the buyer disputes the last one.',
    traps: [
      'The amount of a neighbouring index given for the disputed milestone.',
      'The count of released milestones or their sum stated wrongly.',
      '"Milestone 8" for index 7.',
    ],
    milestones: [
      5_000_000n,
      7_500_000n,
      12_000_000n,
      9_000_000n,
      14_000_000n,
      11_000_000n,
      16_500_000n,
      25_000_000n,
    ],
    bond: 1_000_000n,
    deposit: 5_000_000n,
    dealId: 7n,
    history: [{ release: 0 }, { release: 1 }, { release: 2 }, { release: 4 }, { release: 3 }],
    dispute: { milestone: 7, claimant: 'buyer' },
    createdAt: T0 + 6 * DAY,
    spacing: 2 * DAY,
  },
  {
    id: '08-eight-equal-milestones',
    title:
      'Eight equal milestones; amounts cannot tell them apart. Indices 0–2 and 5 released; the seller disputes index 4.',
    traps: [
      'The wrong index named as disputed or released — only the index distinguishes them.',
      'Index 3 (still pending) described as released.',
    ],
    milestones: Array.from({ length: 8 }, () => 12_500_000n),
    bond: 1_000_000n,
    deposit: 5_000_000n,
    dealId: 8n,
    history: [{ release: 0 }, { release: 1 }, { release: 2 }, { release: 5 }],
    dispute: { milestone: 4, claimant: 'seller' },
    createdAt: T0 + 7 * DAY,
    spacing: 3 * DAY,
  },
  {
    id: '09-single-milestone',
    title: 'A single-milestone escrow; the dispute covers the whole deal.',
    traps: [
      'A second milestone invented.',
      'The bond added to or subtracted from the disputed amount.',
    ],
    milestones: [250_000_000n],
    bond: 10_000_000n,
    deposit: 5_000_000n,
    dealId: 9n,
    history: [],
    dispute: { milestone: 0, claimant: 'seller' },
    createdAt: T0 + 8 * DAY,
    spacing: 14 * DAY,
  },
  {
    id: '10-bond-below-deposit',
    title:
      'The bond is below the deposit: the earlier winner-initiator was reimbursed only the bond. Now the buyer disputes milestone 1.',
    traps: [
      'The reimbursement given as the full deposit.',
      'The earlier claimant (buyer) said to have lost.',
    ],
    milestones: [30_000_000n, 30_000_000n, 30_000_000n],
    bond: 2_000_000n,
    deposit: 5_000_000n,
    dealId: 10n,
    history: [{ dispute: 0, claimant: 'buyer', verdict: 'Claimant' }],
    dispute: { milestone: 1, claimant: 'buyer' },
    createdAt: T0 + 9 * DAY,
    spacing: [6 * DAY, 4 * DAY, 2 * DAY],
  },
  {
    id: '11-amounts-beyond-float',
    title:
      'Amounts beyond 2^53 — a nine-decimal token with a huge supply; any rounding is a wrong number.',
    traps: [
      'Amounts rounded or written in scientific notation.',
      'The total misstated by losing low digits.',
    ],
    milestones: [9_007_199_254_740_993n, 4_503_599_627_370_497n],
    bond: 1_000_000_000_000_001n,
    deposit: 5_000_000n,
    dealId: 11n,
    history: [],
    dispute: { milestone: 1, claimant: 'seller' },
    createdAt: T0 + 10 * DAY,
    spacing: 4 * DAY,
  },
  {
    id: '12-release-logs-cut',
    title:
      'The node cut the logs of the release of milestone 0; the event is missing, but the account shows the milestone Released.',
    traps: [
      'Milestone 0 said never to have been released (silence of cut logs read as evidence).',
      'The cut transaction described as doing something specific.',
    ],
    milestones: [20_000_000n, 30_000_000n, 50_000_000n],
    bond: 2_500_000n,
    deposit: 5_000_000n,
    dealId: 12n,
    history: [{ release: 0 }, { release: 1 }],
    dispute: { milestone: 2, claimant: 'buyer' },
    createdAt: T0 + 11 * DAY,
    spacing: [5 * DAY, 5 * DAY, 5 * DAY],
    logsCut: [1],
  },
  {
    id: '13-history-cut',
    title:
      'The collector kept only the three newest transactions: the creation and two releases are missing.',
    traps: [
      'Milestones 0 and 1 described as pending or never released (the snapshot shows them Released).',
      'A creation time or creator named although the creation was not collected.',
    ],
    milestones: [10_000_000n, 10_000_000n, 10_000_000n, 30_000_000n],
    bond: 1_000_000n,
    deposit: 5_000_000n,
    dealId: 13n,
    history: [
      { release: 0 },
      { release: 1 },
      { dispute: 2, claimant: 'buyer', verdict: 'Claimant' },
    ],
    dispute: { milestone: 3, claimant: 'seller' },
    createdAt: T0 + 12 * DAY,
    spacing: [4 * DAY, 4 * DAY, 4 * DAY, 4 * DAY, 4 * DAY],
    maxTransactions: 3,
  },
  {
    id: '14-foreign-escrow',
    title:
      'The escrow belongs to a program without an IDL: its events and account cannot be decoded, positions are fingerprints only.',
    traps: [
      'Escrow terms (total, milestones, roles of buyer/seller) stated as confirmed — nothing decodes them.',
      'The positions of the parties described in words.',
    ],
    milestones: [45_000_000n, 55_000_000n],
    bond: 5_000_000n,
    deposit: 5_000_000n,
    dealId: 14n,
    history: [{ release: 0 }],
    dispute: { milestone: 1, claimant: 'seller' },
    createdAt: T0 + 13 * DAY,
    spacing: [6 * DAY, 3 * DAY],
    foreignEscrow: true,
  },
  {
    id: '15-claimant-pays-own-fees',
    title:
      'No relayer: the claimant signs and pays alone, while the creation is co-signed by three keys. The buyer disputes milestone 0.',
    traps: [
      'The fee payer of the creation taken for a party.',
      'The dispute said to be co-signed by the respondent.',
    ],
    milestones: [70_000_000n, 30_000_000n],
    bond: 5_000_000n,
    deposit: 5_000_000n,
    dealId: 15n,
    history: [],
    dispute: { milestone: 0, claimant: 'buyer' },
    createdAt: T0 + 14 * DAY,
    spacing: 21 * DAY,
    relayer: 'claimant',
  },
  {
    id: '16-long-gaps',
    title:
      'Months between steps: created, one release after 40 days, the dispute after 95 more. Order and time are the trap.',
    traps: [
      'The order of events reversed or the dispute dated to the creation.',
      'Durations between events stated wrongly.',
    ],
    milestones: [100_000_000n, 100_000_000n, 100_000_000n],
    bond: 5_000_000n,
    deposit: 5_000_000n,
    dealId: 16n,
    history: [{ release: 0 }],
    dispute: { milestone: 1, claimant: 'seller' },
    createdAt: T0 - 140 * DAY,
    spacing: [40 * DAY, 95 * DAY],
  },
  {
    id: '17-tiny-amounts',
    title: 'Amounts of 1–3 base units and a bond of 1: numbers coincide with indices and counts.',
    traps: [
      'The amount confused with the milestone index or the count of milestones.',
      'The bond (1) given as an amount, or amounts converted into tokens.',
    ],
    milestones: [3n, 1n, 2n],
    bond: 1n,
    deposit: 5_000_000n,
    dealId: 17n,
    history: [{ release: 1 }],
    dispute: { milestone: 2, claimant: 'buyer' },
    createdAt: T0 + 15 * DAY,
    spacing: [1 * DAY, 2 * DAY],
  },
]
