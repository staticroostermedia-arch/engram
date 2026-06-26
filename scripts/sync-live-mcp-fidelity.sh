#!/bin/bash
# sync-live-mcp-fidelity.sh — Rebuild engram, restart stale MCP, verify composite tools on live launch path.
# Required for connected Grok/Cursor agents to see mcp_engram_safe_edit_and_verify + update_with_tensor_bond.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${ENGRAM_BINARY:-$REPO_ROOT/target/debug/engram}"
STORE="${ENGRAM_STORE:-$HOME/.engram/stalks/}"
SCRATCH="${SCRATCH:-}"

echo "==> sync-live-mcp-fidelity"
echo "    Repo:   $REPO_ROOT"
echo "    Binary: $BINARY"
echo "    Store:  $STORE"

echo "==> cargo build -p engram-server"
(cd "$REPO_ROOT" && cargo build -p engram-server)

if [[ ! -x "$BINARY" ]]; then
  echo "FAIL: binary not found at $BINARY"
  exit 1
fi

# Prefer repo binary for engram-grok launcher (already default when target/debug exists)
export ENGRAM_BINARY="$BINARY"

# Symlink ~/.local/bin/engram for PATH users (non-destructive)
mkdir -p "$HOME/.local/bin"
ln -sf "$BINARY" "$HOME/.local/bin/engram"

LIVE_PID=""
if pgrep -f "engram.*mcp" >/dev/null 2>&1; then
  LIVE_PID=$(pgrep -f "engram.*mcp" | head -1)
  BIN_MTIME=$(stat -c %Y "$BINARY" 2>/dev/null || echo 0)
  PROC_START=$(stat -c %Y "/proc/$LIVE_PID" 2>/dev/null || echo 0)
  if [[ "$BIN_MTIME" -gt "$PROC_START" ]] || [[ "${FORCE_MCP_RESTART:-0}" == "1" ]]; then
    echo "==> Stopping stale engram MCP (pid=$LIVE_PID) — binary newer than process"
    pkill -f "engram.*mcp" 2>/dev/null || true
    sleep 1
    LIVE_PID=""
  else
    echo "==> Live MCP running (pid=$LIVE_PID) — binary not newer; use FORCE_MCP_RESTART=1 to recycle"
  fi
fi

probe_mcp() {
  local store="$1"
  local label="$2"
  local tmp
  tmp=$(mktemp)
  INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"sync-fidelity","version":"1"}}}'
  LIST='{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
  printf '%s\n%s\n' "$INIT" "$LIST" | timeout 20 "$BINARY" --store "$store" mcp 2>/dev/null | tail -1 >"$tmp"
  python3 - "$tmp" "$label" "$BINARY" <<'PY'
import json, sys
path, label, binary = sys.argv[1:4]
line = open(path).read().strip()
data = json.loads(line) if line else {}
tools = data.get("result", {}).get("tools", [])
names = {t.get("name") for t in tools}
composites = [
    "mcp_engram_safe_edit_and_verify",
    "mcp_engram_update_with_tensor_bond",
]
out = {
    "probe_label": label,
    "binary": binary,
    "tool_count": len(tools),
    "composites_present": {c: c in names for c in composites},
    "all_ok": all(c in names for c in composites),
}
for c in composites:
    if c in names:
        t = next(x for x in tools if x.get("name") == c)
        out[f"{c}_few_shot"] = "FEW-SHOT" in (t.get("description") or "")
print(json.dumps(out, indent=2))
sys.exit(0 if out["all_ok"] else 1)
PY
  rm -f "$tmp"
}

echo "==> Probing fresh isolated MCP (post-build binary)"
TMPSTORE=$(mktemp -d /tmp/engram-sync-fidelity-XXXXXX)
trap 'rm -rf "$TMPSTORE"' EXIT
probe_mcp "$TMPSTORE" "isolated_fresh" | tee "${SCRATCH:+$SCRATCH/}live_mcp_probe.json" 2>/dev/null || {
  echo "FAIL: isolated probe missing composites"
  exit 1
}

echo "OK: Composite tools registered on $("$BINARY" --version 2>/dev/null || echo built-binary)"
if [[ -n "$LIVE_PID" ]]; then
  echo "NOTE: Live session MCP still on old pid=$LIVE_PID — open NEW Grok/Cursor session to pick up composites."
else
  echo "NOTE: No live MCP — next session_start will launch fresh binary via engram-grok."
fi