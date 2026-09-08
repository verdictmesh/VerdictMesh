#!/usr/bin/env bash
# Тести ончейн-програми. Запускати з WSL: bash scripts/wsl-test.sh [аргументи cargo test]
# Потребує свіжого target/deploy/verdict_mesh.so — спершу bash scripts/wsl-build.sh
# Логи програми йдуть у stderr повз захоплення cargo. Заглушити: RUST_LOG=off
set -euo pipefail
source "$HOME/.cargo/env" 2>/dev/null || true
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
cargo test -p verdict-mesh --tests "$@"
