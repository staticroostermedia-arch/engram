#!/usr/bin/env bash
# Print recent cold-start fidelity scores from helper:cold_start_fidelity_series
# or metric:cold_start_fidelity_* in a store directory.
# Usage: scripts/cold-start-report.sh [store_dir] [out_file]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STORE="${1:-${ENGRAM_STORE:-$HOME/.engram/stalks/}}"
STORE="${STORE/#\~/$HOME}"
OUT="${2:-}"
BIN="${ENGRAM_BINARY:-$ROOT/target/debug/engram}"

report() {
  echo "=== cold-start-report $(date -Iseconds) ==="
  echo "STORE=$STORE"
  SERIES="$STORE/helper:cold_start_fidelity_series.leg"
  SERIES3="$STORE/helper:cold_start_fidelity_series.leg3"
  if [[ -f "$SERIES3" ]]; then
    F="$SERIES3"
  elif [[ -f "$SERIES" ]]; then
    F="$SERIES"
  else
    F=""
  fi
  if [[ -n "$F" ]]; then
    echo "--- series file: $F ---"
    # Best-effort: strings from .leg payload
    if command -v strings >/dev/null 2>&1; then
      strings -n 8 "$F" | grep -E 'score|session_key|metric:cold|COLD-START|^\s*[\[{0-9]' | head -80
    else
      # Fallback: python extract utf-8-ish
      python3 - <<PY
import re, pathlib
raw = pathlib.Path("$F").read_bytes()
text = raw.decode("utf-8", "replace")
# print last JSON array if present
if "[" in text and "]" in text:
    i, j = text.rfind("["), text.rfind("]")
    if 0 <= i < j:
        print(text[i:j+1][:4000])
else:
    print(text[-2000:])
PY
    fi
  else
    echo "No helper:cold_start_fidelity_series block yet (expected before first post-P0 session_start)."
    echo "Listing metric:cold_start_fidelity_* files if any:"
    shopt -s nullglob
    mets=("$STORE"/metric:cold_start_fidelity_*)
    if ((${#mets[@]})); then
      ls -1t "${mets[@]}" | head -15
      for f in $(ls -1t "${mets[@]}" 2>/dev/null | head -5); do
        echo "--- $f ---"
        strings -n 6 "$f" 2>/dev/null | grep -E 'score|session_key|COLD-START|version' | head -12 || true
      done
    else
      echo "(none — run session_start with target/debug/engram, or scripts/tier1-fidelity-dogfood.sh)"
    fi
    shopt -u nullglob
  fi
  echo "=== end ==="
}

if [[ -n "$OUT" ]]; then
  report | tee "$OUT"
else
  report
fi
