#!/usr/bin/env bash
# RSI batch verification — parameterized cycle tests + MCP capture append.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SCRATCH="${SCRATCH:-/tmp/grok-goal-f165d4817872/implementer}"
SESSION_MCP_DIR="${SESSION_MCP_DIR:-$SCRATCH/session-mcp}"
CYCLE="${1:-2}"
TEST_FILTER="${2:-combined_sentinel}"

mkdir -p "$SCRATCH" "$SESSION_MCP_DIR"
ENGRAM_BIN="${ENGRAM_BINARY:-$REPO_ROOT/target/debug/engram}"
STORE="${ENGRAM_STORE:-$HOME/.engram/stalks/}"

cd "$REPO_ROOT"

echo "=== RSI batch verify cycle $CYCLE ===" | tee -a "$SCRATCH/rsi-batch-tests.log"

cargo test -p engram-server -- "$TEST_FILTER" 2>&1 | tee -a "$SCRATCH/rsi-batch-tests.log"
echo "CYCLE${CYCLE}_TEST_EXIT=${PIPESTATUS[0]}" | tee -a "$SCRATCH/rsi-batch-tests.log"

{
  cargo clippy -p engram-server -p engram-core -- -D warnings 2>&1
  echo "CLIPPY_EXIT=$?"
  cargo fmt --check 2>&1
  echo "FMT_EXIT=$?"
} | tee -a "$SCRATCH/rsi-batch-lint.log"

DECISION="RSI Cycle ${CYCLE} shipped improvement"
TITLE="RSI Cycle ${CYCLE} — continuity batch"
python3 "$REPO_ROOT/scripts/rsi_batch_mcp_capture.py" \
  --binary "$ENGRAM_BIN" --store "$STORE" --scratch "$SCRATCH" \
  --session-mcp-dir "$SESSION_MCP_DIR" --cycle "$CYCLE" \
  --decision "$DECISION" --title "$TITLE" \
  2>&1 | tee -a "$SCRATCH/rsi-batch-mcp.txt"

echo "CYCLE${CYCLE}_MCP_EXIT=${PIPESTATUS[0]}" | tee -a "$SCRATCH/rsi-batch-mcp.txt"

cat "$REPO_ROOT/docs/rsi_evolution_log.md" >> "$SCRATCH/rsi-batch-artifacts.txt"
git log -5 --oneline > "$SCRATCH/rsi-batch-git.txt"