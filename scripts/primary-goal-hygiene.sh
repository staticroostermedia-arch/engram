#!/usr/bin/env bash
# Restore primary_goal marker away from test pollution (tier3_wake_test, set_at: test, etc.).
# Does NOT require exclusive MCP lock if using file-level note only — prefer MCP goal_set_primary
# when Engram MCP is available.
#
# Usage:
#   scripts/primary-goal-hygiene.sh [goal_id] [evidence_out]
# Default goal_id: goal:engram_mvp_v1
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GOAL="${1:-goal:engram_mvp_v1}"
OUT="${2:-}"
STORE="${ENGRAM_STORE:-$HOME/.engram/stalks/}"
STORE="${STORE/#\~/$HOME}"

report() {
  echo "=== primary-goal-hygiene $(date -Iseconds) ==="
  echo "STORE=$STORE GOAL=$GOAL"
  PG="$STORE/primary_goal.leg"
  PG3="$STORE/primary_goal.leg3"
  F=""
  [[ -f "$PG3" ]] && F="$PG3"
  [[ -z "$F" && -f "$PG" ]] && F="$PG"
  if [[ -z "$F" ]]; then
    echo "status=no_primary_marker"
    echo "action=set_via_mcp: mcp_engram_goal_set_primary(goal=\"$GOAL\")"
    return 0
  fi
  echo "marker_file=$F"
  PREVIEW=$(strings -n 6 "$F" 2>/dev/null | head -20 || true)
  echo "--- preview ---"
  echo "$PREVIEW"
  echo "---"
  if echo "$PREVIEW" | grep -qE 'tier3_wake_test|set_at: test|temp_|lean_gaps|tier1_dogfood'; then
    echo "status=POLLUTED"
    echo "fix=call MCP: mcp_engram_goal_set_primary(goal=\"$GOAL\")"
    echo "also=prefer ENGRAM_DISABLE_SHEAF=1 for unit tests; StoreHandle isolates non-sheaf paths"
  else
    echo "status=OK_or_non_test"
  fi
  echo "discipline=Align TUI /goal Active with Engram goal_set_primary at block start; complete both at end"
  echo "=== end ==="
}

if [[ -n "$OUT" ]]; then
  report | tee "$OUT"
else
  report
fi
