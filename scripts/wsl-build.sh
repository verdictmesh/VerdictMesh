#!/usr/bin/env bash
# Збірка ончейн-програм. Запускати з WSL: bash scripts/wsl-build.sh
set -euo pipefail
source "$HOME/.cargo/env" 2>/dev/null || true
export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
anchor build
