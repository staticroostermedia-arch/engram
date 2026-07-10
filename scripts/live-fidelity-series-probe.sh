#!/usr/bin/env bash
# Persist N cold-start fidelity samples on a store via cargo-tested StoreHandle API.
# Usage: scripts/live-fidelity-series-probe.sh [store] [N] [out_file]
# Default: ~/.engram/stalks, N=10
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STORE="${1:-${ENGRAM_STORE:-$HOME/.engram/stalks/}}"
STORE="${STORE/#\~/$HOME}"
N="${2:-10}"
OUT="${3:-}"
export ENGRAM_LIVE_FIDELITY_PROBE=1
export ENGRAM_LIVE_FIDELITY_STORE="$STORE"
export ENGRAM_LIVE_FIDELITY_N="$N"
export ENGRAM_DISABLE_SHEAF="${ENGRAM_DISABLE_SHEAF:-0}"
# Allow production path to use sheaf when STORE matches stalk
export ENGRAM_FORCE_CPU_BACKEND="${ENGRAM_FORCE_CPU_BACKEND:-0}"

run() {
  echo "=== live-fidelity-series-probe $(date -Iseconds) ==="
  echo "STORE=$STORE N=$N"
  (cd "$ROOT" && cargo test -p engram-server live_fidelity_series_probe -- --nocapture --ignored) 2>&1
  echo "=== end ==="
}

if [[ -n "$OUT" ]]; then
  run | tee "$OUT"
else
  run
fi
