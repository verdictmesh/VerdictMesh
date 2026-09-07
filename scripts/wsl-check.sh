#!/usr/bin/env bash
# Ончейн-гейт, дзеркало rust-джоби з .github/workflows/ci.yml.
# Запускати з WSL: bash scripts/wsl-check.sh
set -euo pipefail
source "$HOME/.cargo/env" 2>/dev/null || true
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
