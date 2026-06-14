#!/usr/bin/env bash
# Restart ONLY engram REST serve on :3456 — does NOT kill TUI MCP (engram mcp).
#
# WRONG (kills MCP):  pgrep -x engram | xargs kill
# RIGHT:              ./scripts/restart-leg-serve.sh

set -euo pipefail

SERVE_PORT="${ENGRAM_SERVE_PORT:-3456}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${ENGRAM_BINARY:-$REPO_ROOT/target/debug/engram}"
STORE="${ENGRAM_STORE:-$HOME/.engram/manifold}"
LOG="${ENGRAM_SERVE_LOG:-$REPO_ROOT/leg-serve.log}"

if command -v fuser >/dev/null 2>&1; then
  fuser -k "${SERVE_PORT}/tcp" >/dev/null 2>&1 || true
  sleep 0.8
fi

if [[ ! -x "$BINARY" ]]; then
  echo "Building engram-server..." >&2
  (cd "$REPO_ROOT" && cargo build -p engram-server)
fi

echo "Starting engram serve on :${SERVE_PORT} (MCP left running)..." >&2
ENGRAM_STORE="$STORE" nohup "$BINARY" serve --light --no-scout --port "$SERVE_PORT" >>"$LOG" 2>&1 &
sleep 2
if curl -sf "http://127.0.0.1:${SERVE_PORT}/health" >/dev/null; then
  echo "OK: serve healthy on :${SERVE_PORT}"
else
  echo "FAIL: serve did not respond — tail $LOG" >&2
  exit 1
fi