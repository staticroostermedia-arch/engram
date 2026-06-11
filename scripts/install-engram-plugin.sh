#!/bin/bash
# install-engram-plugin.sh — Build/install engram binary + register Grok plugin
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLUGIN_DIR="$REPO_ROOT/grok-plugin-engram"
STORE="${ENGRAM_STORE:-$HOME/.engram/stalks/}"

echo "==> Engram Grok plugin installer"
echo "    Repo: $REPO_ROOT"
echo "    Store: $STORE"

mkdir -p "$STORE" "$HOME/.local/bin"

# Build if no binary on PATH
if ! command -v engram >/dev/null 2>&1 && [ ! -x "$REPO_ROOT/target/debug/engram" ]; then
  if [ -n "${ENGRAM_RELEASE_TAG:-}" ]; then
    echo "==> Downloading release ${ENGRAM_RELEASE_TAG} (set ENGRAM_RELEASE_TAG to override)…"
    ASSET="engram-${ENGRAM_RELEASE_TAG}-linux-x86_64.tar.gz"
    URL="https://github.com/staticroostermedia-arch/engram/releases/download/${ENGRAM_RELEASE_TAG}/${ASSET}"
    TMP=$(mktemp -d)
    curl -fsSL "$URL" -o "$TMP/$ASSET"
    tar -xzf "$TMP/$ASSET" -C "$TMP"
    install -m 755 "$TMP/engram-linux-x86_64" "$HOME/.local/bin/engram"
    rm -rf "$TMP"
  else
    echo "==> Building engram-server (first time)…"
    (cd "$REPO_ROOT" && cargo build -p engram-server)
  fi
fi

# Prefer cargo install for global PATH (marketplace users)
if command -v cargo >/dev/null 2>&1; then
  echo "==> Installing engram to ~/.local/bin …"
  cargo install --path "$REPO_ROOT/crates/engram-server" --force 2>/dev/null || {
    echo "    cargo install failed; symlinking target/debug/engram"
    ln -sf "$REPO_ROOT/target/debug/engram" "$HOME/.local/bin/engram"
  }
else
  ln -sf "$REPO_ROOT/target/debug/engram" "$HOME/.local/bin/engram"
fi

chmod +x "$PLUGIN_DIR/bin/engram-grok"
ln -sf "$REPO_ROOT/scripts/engram-grok" "$HOME/.local/bin/engram-grok" 2>/dev/null || true

if command -v grok >/dev/null 2>&1; then
  echo "==> Installing Grok plugin (trusted)…"
  grok plugin install "$PLUGIN_DIR" --trust
  echo "==> MCP health check:"
  "$REPO_ROOT/scripts/engram-mcp-health.sh" || true
  if pgrep -f "engram.*mcp" >/dev/null 2>&1; then
    echo "    Note: skip 'grok mcp doctor' while a Grok session is open (lock contention)."
  else
    echo "==> grok mcp doctor (optional):"
    grok mcp doctor engram 2>&1 || true
  fi
else
  echo "==> grok CLI not found — plugin files ready at $PLUGIN_DIR"
  echo "    Manual: grok plugin install $PLUGIN_DIR --trust"
fi

echo ""
echo "Done. Open a NEW Grok Build session, then run /engram-wake or session_start."
echo "Version: $(engram --version 2>/dev/null || "$REPO_ROOT/target/debug/engram" --version)"