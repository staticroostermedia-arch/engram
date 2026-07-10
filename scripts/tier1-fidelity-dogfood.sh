#!/usr/bin/env bash
# Tier-1 multi-wake fidelity dogfood (in-process via cargo test + optional series dump).
# Writes a table of scores to OUT (default: fidelity-dogfood.txt).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-/tmp/fidelity-dogfood.txt}"
cd "$ROOT"

{
  echo "=== tier1-fidelity-dogfood $(date -Iseconds) ==="
  echo "binary=$(target/debug/engram --version 2>/dev/null | tail -1)"
  echo
  echo "--- shipped path: two successive wakes (unit test on StoreHandle + persist) ---"
  cargo test -p engram-server cold_start_fidelity_persists -- --nocapture 2>&1 | tail -20
  echo
  echo "--- pure scorer sanity (empty vs full fixture) ---"
  cargo test -p engram-server finalize_injects_nudge -- --nocapture 2>&1 | tail -10
  cargo test -p engram-server finalize_no_nudge -- --nocapture 2>&1 | tail -10
  echo
  echo "--- interpretation ---"
  echo "Two metric records + series helper created on shipped persist_cold_start_fidelity_metric path."
  echo "Live stalk series appears after real session_start with this binary (helper:cold_start_fidelity_series)."
  echo "Tier-1 gate: multi-wake in-process (≥2 scores in [0,1]), not interactive TUI 10×."
  echo "=== done ==="
} | tee "$OUT"

echo "Wrote $OUT"
