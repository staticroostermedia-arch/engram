#!/usr/bin/env bash
# live-observer.sh — Standalone live monitor for any running engram (harness or production)
# Usage: live-observer.sh [optional pid or store path fragment]
# Tails relevant logs, ps, lsof on store, manifold queries (if MCP tools available in context)

set -euo pipefail

FRAG="${1:-engram-harness}"

echo "=== Live Engram Observer (filter: $FRAG) ==="
echo "Press Ctrl-C to stop. Also watch your TUI for any MCP symptoms."

# Background monitors
(
  while true; do
    echo "--- $(date -u +%H:%M:%S) ---"
    pgrep -af "$FRAG" | cat
    echo "Open fds on likely stores:"
    for p in $(pgrep -f "$FRAG" || true); do
      lsof -p "$p" 2>/dev/null | grep -E '(\.leg|stalks|engram-harness)' | head -4 || true
    done
    sleep 4
  done
) &
MON_PID=$!

# Log tails (common locations)
for log in ~/.grok/logs/mcp/engram.stderr.log /tmp/*.log /path/to/Documents/Engram/*.log 2>/dev/null; do
  if [[ -f "$log" ]]; then
    tail -f "$log" | grep --line-buffered -iE '(transport|closed|mcp-fast|pipeline|lbvh|bvh|starv|error|ready|ki_hijacker)' &
  fi
done

# If in a context with working MCP tools, periodically surface high-level health
echo ""
echo "If your TUI MCP is healthy, also run in parallel (another pane):"
echo "  mcp_engram_stats ; mcp_engram_verify_manifold_integrity min_crs:0.6 sample_size:8 ; mcp_engram_spatial_status"

trap "kill $MON_PID 2>/dev/null || true; pkill -P $$ 2>/dev/null || true; echo 'Observer stopped.'" INT TERM EXIT
wait
