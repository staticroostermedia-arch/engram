#!/usr/bin/env bash
# Tier-4c: isolated-store continuity loop (wake → remember → end → wake2 handoff).
# Uses cargo test continuity_wake_remember_end_wake2_handoff (shipped StoreHandle path).
# Usage: scripts/continuity-demo.sh [out_log]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-}"
export ENGRAM_DISABLE_SHEAF=1
export ENGRAM_FORCE_CPU_BACKEND=1

run() {
  echo "=== continuity-demo $(date -Iseconds) ==="
  echo "Drives shipped StoreHandle: primary + remember + handoff + wake2 + fidelity series"
  (cd "$ROOT" && cargo test -p engram-server continuity_wake_remember_end_wake2_handoff -- --nocapture) 2>&1
  echo "=== also: print hello-engram-agent lean loop (shim) ==="
  (cd "$ROOT" && python3 examples/hello-engram-agent.py) 2>&1 | tail -40
  echo "=== end continuity-demo ==="
}

if [[ -n "$OUT" ]]; then
  run | tee "$OUT"
else
  run
fi
