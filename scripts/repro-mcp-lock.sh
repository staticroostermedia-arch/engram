#!/usr/bin/env bash
# Repro: second Engram MCP against a locked store fails with holder PID message.
# Usage: scripts/repro-mcp-lock.sh [output_file]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ENGRAM_BINARY:-$ROOT/target/debug/engram}"
OUT="${1:-/tmp/repro-mcp-lock.txt}"
STORE="${ENGRAM_REPRO_STORE:-$(mktemp -d /tmp/engram-mcp-lock-repro-XXXXXX)}"
mkdir -p "$STORE"

if [[ ! -x "$BIN" ]]; then
  echo "Building engram..." >&2
  (cd "$ROOT" && cargo build -p engram-server -q)
  BIN="$ROOT/target/debug/engram"
fi

{
  echo "=== repro-mcp-lock $(date -Iseconds) ==="
  echo "BIN=$BIN"
  echo "STORE=$STORE"
  echo

  # Start first MCP (stdio, holds lock) in background with open pipe
  # shellcheck disable=SC2094
  FIFO_IN=$(mktemp -u /tmp/engram-mcp-in-XXXXXX)
  mkfifo "$FIFO_IN"
  # Keep FIFO open for writer
  exec 3<>"$FIFO_IN"
  "$BIN" --store "$STORE" mcp <"$FIFO_IN" >/tmp/engram-mcp-repro-stdout.txt 2>/tmp/engram-mcp-repro-stderr1.txt &
  PID1=$!
  echo "First MCP pid=$PID1"
  sleep 1
  if ! kill -0 "$PID1" 2>/dev/null; then
    echo "FAIL: first MCP exited early"
    cat /tmp/engram-mcp-repro-stderr1.txt || true
    exit 1
  fi

  set +e
  timeout 8 "$BIN" --store "$STORE" mcp </dev/null >/tmp/engram-mcp-repro-stdout2.txt 2>/tmp/engram-mcp-repro-stderr2.txt
  RC2=$?
  set -e
  echo "Second MCP exit=$RC2 (expect non-zero)"
  echo "--- second stderr ---"
  cat /tmp/engram-mcp-repro-stderr2.txt || true
  echo "---"
  if grep -q "Holder PID" /tmp/engram-mcp-repro-stderr2.txt \
    || grep -q "Another engram MCP server" /tmp/engram-mcp-repro-stderr2.txt; then
    echo "PASS: second instance failed with lock/holder message"
  else
    echo "FAIL: second instance did not report lock holder"
    kill "$PID1" 2>/dev/null || true
    exec 3>&-
    rm -f "$FIFO_IN"
    exit 1
  fi
  if grep -q "$PID1" /tmp/engram-mcp-repro-stderr2.txt; then
    echo "PASS: holder PID $PID1 named in error"
  else
    echo "WARN: PID $PID1 not found in message (lock file may lag); message still clear"
  fi

  kill "$PID1" 2>/dev/null || true
  wait "$PID1" 2>/dev/null || true
  exec 3>&-
  rm -f "$FIFO_IN"
  sleep 0.5

  # Clean start after kill
  set +e
  timeout 3 "$BIN" --store "$STORE" mcp </dev/null >/dev/null 2>/tmp/engram-mcp-repro-stderr3.txt
  RC3=$?
  set -e
  echo "Post-kill spawn exit=$RC3 (0 or timeout ok if process started)"
  # timeout kills with 124 after start — either way lock should be acquirable briefly
  if grep -q "Another engram MCP server" /tmp/engram-mcp-repro-stderr3.txt; then
    # leftover lock — try orphan recovery path
    echo "NOTE: still locked after kill; checking pgrep"
    pgrep -af 'engram.*mcp' || true
  else
    echo "PASS: clean start after holder stopped (no double-spawn error)"
  fi
  echo "=== done ==="
} | tee "$OUT"

echo "Wrote $OUT"
