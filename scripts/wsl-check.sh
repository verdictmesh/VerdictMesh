#!/usr/bin/env bash
# Перевірка ончейн-крейтів. Запускати з WSL: bash scripts/wsl-check.sh
set -euo pipefail
source "$HOME/.cargo/env" 2>/dev/null || true
export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
cargo check --workspace --all-targets
