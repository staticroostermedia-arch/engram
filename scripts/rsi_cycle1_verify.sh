#!/usr/bin/env bash
# Atomic RSI Cycle 1 verification — plan gating steps + scratch captures.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SCRATCH="${SCRATCH:-/tmp/grok-goal-1d5f7110a8ff/implementer}"
SESSION_MCP_DIR="${SESSION_MCP_DIR:-}"
STORE="${ENGRAM_STORE:-$HOME/.engram/stalks/}"

mkdir -p "$SCRATCH"

if [[ -z "$SESSION_MCP_DIR" ]]; then
  SESSION_MCP_DIR="${GROK_SESSION_MCP_DIR:-$SCRATCH/session-mcp}"
fi
mkdir -p "$SESSION_MCP_DIR"

if [[ -x "$REPO_ROOT/target/debug/engram" ]]; then
  ENGRAM_BIN="$REPO_ROOT/target/debug/engram"
elif [[ -n "${ENGRAM_BINARY:-}" && -x "${ENGRAM_BINARY}" ]]; then
  ENGRAM_BIN="${ENGRAM_BINARY}"
elif command -v engram >/dev/null 2>&1; then
  ENGRAM_BIN="$(command -v engram)"
else
  echo "ERROR: no engram binary" >&2
  exit 2
fi

echo "=== RSI Cycle 1 verify ==="
echo "REPO_ROOT=$REPO_ROOT"
echo "SCRATCH=$SCRATCH"
echo "SESSION_MCP_DIR=$SESSION_MCP_DIR"
echo "ENGRAM_BIN=$ENGRAM_BIN"

cd "$REPO_ROOT"

# Step 1: targeted tests (plan gating)
cargo test -p engram-server -- continuity_spikes surprise_pressure surprise_elevated \
  hub_anchor_surprise update_propagates resolve_hub_anchors \
  2>&1 | tee "$SCRATCH/rsi-cycle1-tests.log"
TEST_EXIT=${PIPESTATUS[0]}
echo "TEST_EXIT=$TEST_EXIT" | tee -a "$SCRATCH/rsi-cycle1-tests.log"

# Step 2: clippy + fmt
{
  cargo clippy -p engram-server -p engram-core -- -D warnings 2>&1
  echo "CLIPPY_EXIT=$?"
  cargo fmt --check 2>&1
  echo "FMT_EXIT=$?"
} | tee "$SCRATCH/rsi-cycle1-lint.log"

CLIPPY_EXIT=$(grep -E '^CLIPPY_EXIT=' "$SCRATCH/rsi-cycle1-lint.log" | tail -1 | cut -d= -f2)
FMT_EXIT=$(grep -E '^FMT_EXIT=' "$SCRATCH/rsi-cycle1-lint.log" | tail -1 | cut -d= -f2)

# Step 3: MCP capture (AC3) — live stdio, writes grep-able session mcp JSON
python3 "$REPO_ROOT/scripts/rsi_cycle1_mcp_capture.py" \
  --binary "$ENGRAM_BIN" \
  --store "$STORE" \
  --scratch "$SCRATCH" \
  --session-mcp-dir "$SESSION_MCP_DIR" \
  2>&1 | tee "$SCRATCH/rsi-cycle1-mcp-run.log"
MCP_EXIT=${PIPESTATUS[0]}
echo "MCP_EXIT=$MCP_EXIT" | tee -a "$SCRATCH/rsi-cycle1-mcp-run.log"

# Step 4: artifacts — full docs (not curated summary)
cat "$REPO_ROOT/docs/rsi_evolution_log.md" > "$SCRATCH/rsi-cycle1-artifacts.txt"
echo "" >> "$SCRATCH/rsi-cycle1-artifacts.txt"
echo "=== CHANGELOG-RSI.md ===" >> "$SCRATCH/rsi-cycle1-artifacts.txt"
cat "$REPO_ROOT/CHANGELOG-RSI.md" >> "$SCRATCH/rsi-cycle1-artifacts.txt"

# Step 5: git + version evidence
{
  echo "=== git log ==="
  git log -3 --oneline
  echo ""
  echo "=== version ==="
  grep 'version' "$REPO_ROOT/Cargo.toml" | head -1
  grep '0.7.0-beta.6' "$REPO_ROOT/Cargo.lock" | head -3
  echo ""
  echo "=== lint ==="
  echo "CLIPPY_EXIT=$CLIPPY_EXIT FMT_EXIT=$FMT_EXIT TEST_EXIT=$TEST_EXIT MCP_EXIT=$MCP_EXIT"
  echo ""
  if [[ -f "$SCRATCH/rsi-cycle1-mcp-capture.json" ]]; then
    echo "=== mcp capture ids ==="
    python3 -c "import json; d=json.load(open('$SCRATCH/rsi-cycle1-mcp-capture.json')); print('trace_id=', d.get('trace_id')); print('tile_id=', d.get('tile_id'))"
  fi
} > "$SCRATCH/rsi-cycle1-git.txt"

OVERALL=0
[[ "$TEST_EXIT" == "0" ]] || OVERALL=1
[[ "$CLIPPY_EXIT" == "0" ]] || OVERALL=1
[[ "$FMT_EXIT" == "0" ]] || OVERALL=1
[[ "$MCP_EXIT" == "0" ]] || OVERALL=1

echo "OVERALL_EXIT=$OVERALL" | tee -a "$SCRATCH/rsi-cycle1-git.txt"
exit "$OVERALL"