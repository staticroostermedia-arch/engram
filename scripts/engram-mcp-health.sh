#!/bin/bash
# engram-mcp-health.sh — Installer-friendly MCP check (avoids false negatives from lock contention)
#
# grok mcp doctor spawns a second engram mcp on the same store and fails when the TUI
# already holds the exclusive flock. This script:
#   1. Reports healthy if a live engram mcp is already running for the store
#   2. Otherwise probes initialize on an isolated temp store (no lock collision)

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
if pgrep -f "engram.*--store.*mcp" >/dev/null 2>&1; then
  LIVE_PID=$(pgrep -f "engram.*mcp" | head -1)
  echo "OK: Live engram MCP already running (pid=$LIVE_PID)."
  echo "    grok mcp doctor will fail while this session is open — that is expected."
  echo "    Open /engram-wake in your Grok session to verify tools."
  exit 0
fi

# Isolated handshake (no flock on production store)
TMPSTORE=$(mktemp -d /tmp/engram-health-XXXXXX)
trap 'rm -rf "$TMPSTORE"' EXIT

INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"health","version":"1"}}}'
RESP=$(printf '%s\n' "$INIT" | timeout 10 "$BINARY" --store "$TMPSTORE" mcp 2>/dev/null | head -1 || true)

if echo "$RESP" | grep -q '"serverInfo".*"engram"'; then
  echo "OK: MCP initialize handshake succeeded (isolated probe)."
  exit 0
fi

echo "FAIL: MCP handshake did not return engram serverInfo."
echo "    Response: ${RESP:-<empty>}"
echo "    Try: cargo build -p engram-server"
exit 1