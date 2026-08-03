#!/usr/bin/env bash
# CPU-only Engram proof harness entrypoint (CI + local).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== building engram-proof-harness =="
cargo build -p engram-proof-harness --release 2>/dev/null || cargo build -p engram-proof-harness

BIN=""
if [[ -x "$ROOT/target/release/engram-proof-harness" ]]; then
  BIN="$ROOT/target/release/engram-proof-harness"
elif [[ -x "$ROOT/target/debug/engram-proof-harness" ]]; then
  BIN="$ROOT/target/debug/engram-proof-harness"
else
  echo "engram-proof-harness binary not found" >&2
  exit 1
fi

# Optional: MCP residual probe when engram server binary is present
if [[ -n "${ENGRAM_PROOF_BIN:-}" ]]; then
  export ENGRAM_PROOF_BIN
elif [[ -x "$ROOT/target/debug/engram" ]]; then
  export ENGRAM_PROOF_BIN="$ROOT/target/debug/engram"
fi

export ENGRAM_FORCE_CPU_BACKEND="${ENGRAM_FORCE_CPU_BACKEND:-1}"
echo "== running $BIN =="
exec "$BIN"
