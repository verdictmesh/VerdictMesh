#!/usr/bin/env bash
# Builds the on-chain programs. Run from WSL: bash scripts/wsl-build.sh
#
# The Agave release is pinned here by path, not taken from `active_release`:
# that symlink is one per machine and other projects switch it. Anchor picks
# its own version from Anchor.toml, cargo-build-sbf does not — so a build under
# whatever release happens to be active goes through silently and yields a
# program that neither devnet nor mollusk can run (`ProgramFailedToComplete`).
#
# Objects in target/sbpf-solana-solana built by another release look fresh to
# cargo (every release calls its compiler the same `rustc` version) and get
# linked into the next build, poisoning it too. The release of the previous
# build is stamped next to them, and the cache is dropped when it differs.
set -euo pipefail

SOLANA_VERSION=4.2.0
RELEASE="$HOME/.local/share/solana/install/releases/$SOLANA_VERSION/solana-release/bin"

if [ ! -x "$RELEASE/solana" ]; then
  echo "Agave $SOLANA_VERSION is not installed: $RELEASE" >&2
  echo "Install it with: agave-install init $SOLANA_VERSION (then switch active_release back if needed)" >&2
  exit 1
fi

source "$HOME/.cargo/env" 2>/dev/null || true
export PATH="$RELEASE:$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

STAMP=target/.sbf-release
if [ -d target/sbpf-solana-solana ] && [ "$(cat "$STAMP" 2>/dev/null)" != "$SOLANA_VERSION" ]; then
  echo "SBF cache was built by another release ($(cat "$STAMP" 2>/dev/null || echo unknown)) — dropping it"
  rm -rf target/sbpf-solana-solana
fi

solana --version
anchor build
mkdir -p target
echo "$SOLANA_VERSION" > "$STAMP"
sha256sum target/deploy/*.so
