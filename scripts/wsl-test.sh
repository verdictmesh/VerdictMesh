#!/usr/bin/env bash
set -euo pipefail
source "$HOME/.cargo/env" 2>/dev/null || true
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
cargo test -p verdict-mesh --test harness 2>&1 | tail -30
