#!/bin/bash
# engram-mcp-health.sh — Installer-friendly MCP check (avoids false negatives from lock contention)
#
# grok mcp doctor spawns a second engram mcp on the same store and fails when the TUI
# already holds the exclusive flock. This script:
#   1. Reports healthy if a live engram mcp is already running for the store
#   2. Otherwise probes initialize on an isolated temp store (no lock collision)
#
# Orphan recovery: scripts/engram-grok (and grok-plugin-engram/bin/engram-grok) auto-remove
# stale ~/.engram/locks/mcp-*.lock files when the recorded PID is dead (before mcp launch).
# If TUI shows "Tool not found" after restart, run: pkill -f "engram.*mcp" OR restart TUI
# (engram-grok will print "Recovered orphaned MCP lock..." when it clears a stale lock).

set -euo pipefail

STORE="${ENGRAM_STORE:-$HOME/.engram/stalks/}"
BINARY="${ENGRAM_BINARY:-}"
if [ -z "$BINARY" ]; then
  if command -v engram >/dev/null 2>&1; then
    BINARY="$(command -v engram)"
  elif [ -x "$(dirname "$0")/../target/debug/engram" ]; then
    BINARY="$(cd "$(dirname "$0")/.." && pwd)/target/debug/engram"
  else
    echo "FAIL: engram binary not found"
    exit 1
  fi
fi

echo "==> Engram MCP health"
echo "    Binary: $BINARY"
echo "    Store:  $STORE"
"$BINARY" --version 2>/dev/null || true

# Live session already owns MCP for this store?
if pgrep -f "engram.*mcp" >/dev/null 2>&1; then
  LIVE_PID=$(pgrep -f "engram.*mcp" | head -1)
  BIN_MTIME=$(stat -c %Y "$BINARY" 2>/dev/null || echo 0)
  PROC_START=$(stat -c %Y "/proc/$LIVE_PID" 2>/dev/null || echo 0)
  if [[ "$BIN_MTIME" -gt "$PROC_START" ]]; then
    echo "WARN: Binary newer than live MCP (pid=$LIVE_PID). Restart required for new tools."
    echo "      Run: pkill -f 'engram.*mcp' && open a new Grok session, or scripts/install-engram-plugin.sh"
    echo "      Probing fresh isolated MCP for composite registration..."
  else
    echo "OK: Live engram MCP already running (pid=$LIVE_PID)."
    echo "    grok mcp doctor will fail while this session is open — that is expected."
    echo "    Open /engram-wake in your Grok session to verify tools."
    exit 0
  fi
fi

# Isolated handshake (no flock on production store)
TMPSTORE=$(mktemp -d /tmp/engram-health-XXXXXX)
trap 'rm -rf "$TMPSTORE"' EXIT

INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"health","version":"1"}}}'
RESP=$(printf '%s\n' "$INIT" | timeout 10 "$BINARY" --store "$TMPSTORE" mcp 2>/dev/null | head -1 || true)

if echo "$RESP" | grep -q '"serverInfo".*"engram"'; then
  echo "OK: MCP initialize handshake succeeded (isolated probe)."
  # Verify agent-tool-fidelity composites are registered (post agent_tool_fidelity_v1)
  LIST='{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
  TOOLS=$(printf '%s\n%s\n' "$INIT" "$LIST" | timeout 15 "$BINARY" --store "$TMPSTORE" mcp 2>/dev/null | tail -1 || true)
  for composite in mcp_engram_safe_edit_and_verify mcp_engram_update_with_tensor_bond; do
    if echo "$TOOLS" | grep -q "\"name\":\"$composite\""; then
      echo "    Composite registered: $composite"
    else
      echo "WARN: $composite missing from tools/list — rebuild: cargo build -p engram-server"
      echo "      Then restart MCP (new Grok session or pkill -f 'engram.*mcp')."
    fi
  done
  exit 0
fi

echo "FAIL: MCP handshake did not return engram serverInfo."
echo "    Response: ${RESP:-<empty>}"
echo "    Try: cargo build -p engram-server"
exit 1