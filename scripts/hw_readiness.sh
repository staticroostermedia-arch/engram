#!/usr/bin/env bash
# Dump Engram hardware readiness fields for this host (dual-GPU + cuFile story).
# Usage: scripts/hw_readiness.sh [store] [out_file]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STORE="${1:-${ENGRAM_STORE:-$HOME/.engram/stalks/}}"
OUT="${2:-}"
BIN="${ENGRAM_BINARY:-$ROOT/target/debug/engram}"

if [[ ! -x "$BIN" ]]; then
  (cd "$ROOT" && cargo build -p engram-server -q)
  BIN="$ROOT/target/debug/engram"
fi

report() {
  echo "=== hw_readiness $(date -Iseconds) ==="
  echo "BIN=$BIN"
  echo "STORE=$STORE"
  echo "ENGRAM_CUFILE_HOT=${ENGRAM_CUFILE_HOT:-unset}"
  echo "ENGRAM_GPU_HOT_DEVICE=${ENGRAM_GPU_HOT_DEVICE:-unset (default often 0)}"
  echo "ENGRAM_GPU_COMPUTE_DEVICE=${ENGRAM_GPU_COMPUTE_DEVICE:-unset (default often 1)}"
  echo
  echo "--- wait-ready (timeout 60s) ---"
  ENGRAM_PROFILE="${ENGRAM_PROFILE:-agent}" ENGRAM_CUFILE_HOT="${ENGRAM_CUFILE_HOT:-1}" \
    timeout 90 "$BIN" --store "$STORE" wait-ready --timeout 60 2>&1 | tail -40
  echo
  echo "--- policy ---"
  echo "gpu_hot_device: BVH + hot residency (device 0 recommended)"
  echo "gpu_compute_device: batch encode / NREM (device 1 recommended)"
  echo "cufile_dma: only valid after successful DMA (see engram-gpu cufile.rs)"
  echo "cufile_hot_ready + transfer_path=unavailable: driver open, no DMA success yet (honest)"
  echo "=== end ==="
}

if [[ -n "$OUT" ]]; then
  report | tee "$OUT"
else
  report
fi
