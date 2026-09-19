# VerdictMesh

Protocol-agnostic dispute resolution for Solana.

Any dApp that holds someone else's money in escrow — a marketplace, a freelance
platform, a DePIN network — eventually faces the question escrow itself cannot
answer: **who decides, when the parties disagree?** Today there are three options,
and all of them are bad. A team multisig, which is a centralized moderator wearing
a trustless costume. A timeout, which turns the dispute into a waiting contest.
Or building your own court — juror registry, staking, voting, slashing, appeals —
which never pays off for a protocol handling a dozen disputes a week.

VerdictMesh is that court, built once and shared. An escrow program integrates it
by reading a single account and gets the cycle dispute → verdict → payout without
handing anyone a key over the money.

## What runs on devnet today

The on-chain core is deployed and measured end to end. The numbers below come
from twenty consecutive dispute cycles on devnet, driven by `scripts/e2e-dispute.ts`
with no manual step between stages.

| Criterion | Budget | Measured |
|---|---|---|
| Full cycle, dispute opened → escrow paid out | ≤ 5 min | **218 s** (worst of 20; spread 214–218 s) |
| Network fees per cycle, all participants combined | ≤ $0.10 | **$0.017** (85 000 lamports, 17 signatures) |
| Cycles that needed manual intervention | 0 of 20 | **0 of 20** |
| Re-executing an already executed verdict | rejected 100 % | **6 of 6 rejected**, no balance moved |

The cycle is bounded by the policy windows themselves (60 s commit + 60 s reveal
+ 90 s appeal = 210 s floor), not by the network. Fees are identical to the
lamport across all twenty cycles: no instruction depends on the amount in
dispute or the number of milestones.

| Program | Devnet address |
|---|---|
| `verdict_mesh` — registry, panel, voting, verdict | `8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq` |
| `reference_escrow` — milestone escrow that executes verdicts | `4iYF4WRdtuSmjTXH5fSa2ow5WrdeonEoeoY3epypfTHo` |

Demo policy: panel of 3, quorum 2, windows 60 / 60 / 90 s, review deposit 5 USDC,
slashing 10 % for voting with the losing side and 20 % for not revealing.

## How it works

Steps marked **live** exist on devnet. Steps marked **planned** are specified in
full but not yet built; the sections below say which milestone brings them.

1. **A dispute opens** over locked funds — *live*. The integrator's policy is
   snapshotted into the dispute, so changing it later cannot affect a hearing
   already under way. A panel is drawn from the staked juror registry in the same
   transaction, and the opening party pays a review deposit.
2. **Evidence is gathered** — *planned, next milestone*. A watcher compiles the
   on-chain history of the deal into a fact-finding report that separates
   confirmed facts from party claims and says plainly where facts are missing
   instead of guessing. Until then jurors vote on the raw claims and transaction
   signatures — which is visible in the demo, and deliberately so.
3. **A panel of staked jurors votes** — *live*. Commit-reveal: a juror seals a
   fingerprint of the vote, then opens it in the reveal window, so nobody can copy
   an earlier vote. Jurors who vote against the outcome lose 10 % of their stake;
   jurors who stay silent lose 20 %. Silence is the worst option available.
4. **The escrow executes the verdict** — *live*. The escrow reads the dispute
   account itself and moves the funds. VerdictMesh never holds integrator money
   and has no instruction that could direct it. That absence is a test, not a
   policy: `programs/verdict-mesh/tests/authority.rs` enumerates the program's
   surface from the IDL and tries, and fails, to bend a verdict with any key.

The one privileged key in the design is the reporter, and its authority stops at
writing a report fingerprint. It cannot touch a vote, a verdict, or a balance.

### Who pays for the hearing

The losing side, exactly once. The opening party pays a deposit; if it loses, the
deposit pays the panel. If it wins, the deposit is reimbursed out of a review bond
the other side posted when the deal was signed. `reference_escrow` holds one bond
per milestone from each side and returns it with the milestone when no dispute
happens. Should the bond fall short — a policy that got more expensive after
signing — the winner is reimbursed as far as it goes and nobody is charged the
difference; the payout under the verdict itself is never reduced.

### Planned

- **Fact-finding report** (`apps/api`): watcher, evidence collection, an
  AI-written report attested on-chain by fingerprint. Model unavailability must
  not block voting.
- **Integrator SDK and a second escrow** structurally unlike the first (payout
  split between the sides, no bond), to prove the boundary is protocol-agnostic.
  Target: a developer new to the project integrates in ≤ 30 minutes and ≤ 40 lines.
- **Appeals**: a larger panel, a bond from the appellant, final verdict.
- **Optimistic track**: below a policy threshold an unchallenged assertion becomes
  the verdict, so a $20 dispute never pays for a jury.

## Demo prototype (`apps/web`)

Four screens — juror panel, hearing as the juror sees it, the same hearing as a
party sees it without signing in, and a settlement receipt showing where the money
went. **Every deal, party, amount and signature in it is invented.** Nothing in
the prototype touches the network; it shows what the product looks like, not that
it works. Countdowns run in real time and the demo loops, so the hearing meant for
the visitor keeps its commit window open until they cast a vote.

```bash
pnpm install
pnpm --filter @verdictmesh/web dev        # http://localhost:5173
```

The same prototype is published on GitHub Pages at
<https://verdictmesh.github.io/VerdictMesh/> by `.github/workflows/pages.yml` on
every push to `main`. The workflow builds `apps/web` with the base path
`/VerdictMesh/` and ships `index.html` as the 404 page so that deep links such as
`/hearing/VM-1042` survive a refresh. Pages itself is switched on once, by the
repository owner: *Settings → Pages → Build and deployment → Source: GitHub
Actions*.

## Repository layout

```
programs/
  verdict-mesh/        # registry, panel selection, commit-reveal, tally, stakes
  reference-escrow/    # milestone escrow: bonds, dispute flag, verdict-driven payout
apps/
  web/                 # demo prototype, React 18 + Vite 5, invented data
  api/                 # Hono service — skeleton until the fact-finding milestone
packages/
  shared/              # Zod schemas shared across API boundaries
scripts/               # devnet tooling: mint, treasury, juror funding, e2e cycle
```

## Toolchain

| Layer | Choice | Why |
|---|---|---|
| On-chain | Anchor 0.32.1 · Agave 4.2.0 · Rust 1.97.1 | newest pair where the Anchor CLI and its TypeScript client agree; crates.io has `anchor-lang` 1.x but npm has no client for it |
| Program tests | mollusk-svm 0.15.0 | `litesvm` does not compile against Anchor 0.32.1 |
| TypeScript | pnpm 9 · Turborepo · TypeScript strict · Biome · Vitest 3 | — |
| Web | React 18 · Vite 5 · Tailwind | — |
| API | Hono 4 · Zod 4 | — |

```bash
pnpm install
pnpm gate                                  # lint + typecheck + test (TypeScript)

# on-chain — Linux or WSL:
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                     # mollusk tests, needs target/deploy/*.so
anchor build                               # produces target/deploy and the IDL
```

Program tests load the compiled `.so` from `target/deploy`, so `anchor build`
comes before the first `cargo test`. CI runs the TypeScript gate plus `fmt` and
`clippy`; the on-chain build is verified before deployment rather than on every
push.

Devnet scripts (`scripts/`) expect a `.env` — see `.env.example`. The end-to-end
cycle needs a dedicated RPC endpoint; the public devnet RPC rate-limits three
parallel cycles.

## Status

Early development on **devnet**. Not audited, not for mainnet, not holding real
money. The on-chain core is complete and measured; the fact-finding layer, SDK,
appeals and optimistic track are specified and scheduled.

## License

Apache-2.0 — see [LICENSE](LICENSE).
