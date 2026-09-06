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
by reading a single account, and gets the full cycle: dispute → evidence →
verdict → payout, with no privileged key anywhere in the path.

## How it works

1. **A dispute opens** over locked funds. The policy in force is snapshotted, so
   changing it later cannot affect a dispute already under way.
2. **Evidence is gathered** from on-chain history and compiled into a fact-finding
   report that separates confirmed facts from party claims — and says plainly
   where facts are missing instead of guessing.
3. **A panel of staked jurors votes** using commit-reveal, so no one can copy an
   earlier vote. Jurors who go against the outcome lose part of their stake;
   jurors who stay silent lose more.
4. **The escrow executes the verdict** by reading the dispute account directly.
   VerdictMesh never holds integrator funds and has no authority over them.

Small disputes skip the panel entirely: below a policy threshold, an unchallenged
assertion becomes the verdict. Calling a jury for a $20 dispute costs more than
the dispute is worth — which is exactly why arbitration has not worked elsewhere.

## Status

Early development on **devnet**. Not audited, not for mainnet, not holding real
money. See `docs/` for the specification and delivery plan.

## Toolchain

Anchor 0.32.1 · Agave 4.2.0 · Rust 1.97.1 · pnpm 9 + Turborepo · TypeScript strict

The on-chain toolchain is pinned deliberately: crates.io ships `anchor-lang` 1.1.2,
but no TypeScript client for 1.x exists on npm. 0.32.1 is the newest pair where the
CLI and the client agree.

```bash
pnpm install
pnpm gate                                    # lint + typecheck + test
anchor build                                 # requires Linux or WSL
```

## License

Apache-2.0
