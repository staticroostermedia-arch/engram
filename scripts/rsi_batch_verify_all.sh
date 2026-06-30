#!/usr/bin/env bash
# Run per-cycle tests + MCP capture for RSI batch cycles 2-5; refresh all scratch evidence.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SCRATCH="${SCRATCH:-/tmp/grok-goal-f165d4817872/implementer}"
SESSION_MCP_DIR="${SESSION_MCP_DIR:-$SCRATCH/session-mcp}"

mkdir -p "$SCRATCH" "$SESSION_MCP_DIR"
cd "$REPO_ROOT"

: > "$SCRATCH/rsi-batch-tests.log"
: > "$SCRATCH/rsi-batch-lint.log"
: > "$SCRATCH/rsi-batch-mcp.txt"
: > "$SCRATCH/rsi-batch-artifacts.txt"

CYCLE_MIN="${RSI_CYCLE_MIN:-2}"
CYCLE_MAX="${RSI_CYCLE_MAX:-9}"

declare -A FILTERS=(
  [2]="combined_sentinel"
  [3]="session_intent_wires"
  [4]="full_system_audit_loop"
  [5]="rsi_batch_verify_scripts"
  [6]="weighted_sentinel"
  [7]="resolve_rsi_cycle"
  [8]="meta_workflow_registry"
  [9]="rsi_batch_verify_scripts"
)

declare -A DECISIONS=(
  [2]="RSI Cycle 2 Lyapunov-ego sentinel blend shipped"
  [3]="RSI Cycle 3 turn_record session_intent sentinel parity"
  [4]="RSI Cycle 4 full_system_audit_loop TOML parse fix"
  [5]="RSI Cycle 5 batch verify pipeline v0.7.0-beta.7"
  [6]="RSI Cycle 6 weighted Lyapunov sentinel blend"
  [7]="RSI Cycle 7 ENGRAM_RSI_CYCLE harness metrics"
  [8]="RSI Cycle 8 meta_workflow_registry harness exposure"
  [9]="RSI Cycle 9 batch verify extended cycles 6-9 v0.7.0-beta.8"
)

declare -A TITLES=(
  [2]="RSI Cycle 2 — Lyapunov-ego blend"
  [3]="RSI Cycle 3 — session_intent parity"
  [4]="RSI Cycle 4 — audit loop TOML"
  [5]="RSI Cycle 5 — batch verify"
  [6]="RSI Cycle 6 — weighted blend"
  [7]="RSI Cycle 7 — RSI cycle env"
  [8]="RSI Cycle 8 — meta workflow registry"
  [9]="RSI Cycle 9 — batch verify 6-9"
)

OVERALL=0

for CYCLE in $(seq "$CYCLE_MIN" "$CYCLE_MAX"); do
  FILTER="${FILTERS[$CYCLE]:-}"
  if [[ -z "$FILTER" ]]; then
    echo "SKIP CYCLE $CYCLE (no filter)" | tee -a "$SCRATCH/rsi-batch-tests.log"
    continue
  fi
  echo "=== CYCLE $CYCLE tests ($FILTER) ===" | tee -a "$SCRATCH/rsi-batch-tests.log"
  if cargo test -p engram-server -- "$FILTER" 2>&1 | tee -a "$SCRATCH/rsi-batch-tests.log"; then
    echo "CYCLE${CYCLE}_TEST_EXIT=0" | tee -a "$SCRATCH/rsi-batch-tests.log"
  else
    echo "CYCLE${CYCLE}_TEST_EXIT=1" | tee -a "$SCRATCH/rsi-batch-tests.log"
    OVERALL=1
  fi
done

{
  cargo clippy -p engram-server -p engram-core -- -D warnings 2>&1
  echo "CLIPPY_EXIT=$?"
  cargo fmt --check 2>&1
  echo "FMT_EXIT=$?"
} | tee "$SCRATCH/rsi-batch-lint.log"

CLIPPY_EXIT=$(grep '^CLIPPY_EXIT=' "$SCRATCH/rsi-batch-lint.log" | tail -1 | cut -d= -f2)
FMT_EXIT=$(grep '^FMT_EXIT=' "$SCRATCH/rsi-batch-lint.log" | tail -1 | cut -d= -f2)
[[ "$CLIPPY_EXIT" == "0" ]] || OVERALL=1
[[ "$FMT_EXIT" == "0" ]] || OVERALL=1

ENGRAM_BIN="${ENGRAM_BINARY:-$REPO_ROOT/target/debug/engram}"
STORE="${ENGRAM_STORE:-$HOME/.engram/stalks/}"

for CYCLE in $(seq "$CYCLE_MIN" "$CYCLE_MAX"); do
  if [[ -z "${DECISIONS[$CYCLE]:-}" ]]; then
    continue
  fi
  echo "=== CYCLE $CYCLE MCP ===" | tee -a "$SCRATCH/rsi-batch-mcp.txt"
  if python3 "$REPO_ROOT/scripts/rsi_batch_mcp_capture.py" \
    --binary "$ENGRAM_BIN" --store "$STORE" --scratch "$SCRATCH" \
    --session-mcp-dir "$SESSION_MCP_DIR" --cycle "$CYCLE" \
    --decision "${DECISIONS[$CYCLE]}" --title "${TITLES[$CYCLE]}" \
    2>&1 | tee -a "$SCRATCH/rsi-batch-mcp.txt"; then
    echo "CYCLE${CYCLE}_MCP_EXIT=0" | tee -a "$SCRATCH/rsi-batch-mcp.txt"
  else
    echo "CYCLE${CYCLE}_MCP_EXIT=1" | tee -a "$SCRATCH/rsi-batch-mcp.txt"
    OVERALL=1
  fi
  if [[ -f "$SCRATCH/rsi-cycle${CYCLE}-mcp-capture.json" ]]; then
    python3 -c "
import json
d=json.load(open('$SCRATCH/rsi-cycle${CYCLE}-mcp-capture.json'))
print('CYCLE${CYCLE}_TRACE_ID=', d.get('trace_id'))
print('CYCLE${CYCLE}_TILE_ID=', d.get('tile_id'))
for p in d.get('session_call_paths', []):
    print('CALL_FILE:', p)
" | tee -a "$SCRATCH/rsi-batch-mcp.txt"
  fi
done

echo "=== session mcp grep rsi_cycle ===" | tee -a "$SCRATCH/rsi-batch-mcp.txt"
find "$SESSION_MCP_DIR" -name 'call-*-rsi_cycle*.json' 2>/dev/null | sort | tee -a "$SCRATCH/rsi-batch-mcp.txt"

cat "$REPO_ROOT/docs/rsi_evolution_log.md" > "$SCRATCH/rsi-batch-artifacts.txt"
echo "" >> "$SCRATCH/rsi-batch-artifacts.txt"
echo "=== CHANGELOG-RSI.md ===" >> "$SCRATCH/rsi-batch-artifacts.txt"
cat "$REPO_ROOT/CHANGELOG-RSI.md" >> "$SCRATCH/rsi-batch-artifacts.txt"

{
  echo "=== git log ==="
  git log -8 --oneline
  echo ""
  echo "=== version ==="
  grep 'version' Cargo.toml | head -1
  echo ""
  echo "OVERALL_EXIT=$OVERALL"
  echo "CLIPPY_EXIT=$CLIPPY_EXIT FMT_EXIT=$FMT_EXIT"
} > "$SCRATCH/rsi-batch-git.txt"

{
  echo "=== Batch Checkpoint (Cycles ${CYCLE_MIN}-${CYCLE_MAX}) ==="
  cat "$SCRATCH/rsi-batch-git.txt"
  echo ""
  grep -E 'CYCLE[2-5]_(TEST|MCP)_EXIT' "$SCRATCH/rsi-batch-tests.log" "$SCRATCH/rsi-batch-mcp.txt" 2>/dev/null || true
} > "$SCRATCH/rsi-batch-checkpoint.txt"

echo "OVERALL_EXIT=$OVERALL"
exit "$OVERALL"