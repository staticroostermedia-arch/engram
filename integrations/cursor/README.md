# Cursor + Engram

## Install MCP

1. Copy or symlink [`.cursor/mcp.json`](../../.cursor/mcp.json) (committed) — points at `scripts/engram-grok` with `ENGRAM_PROFILE=agent`.
2. Enable the **engram** MCP server in Cursor Settings → MCP.
3. Load rules: [`.cursor/rules/engram.md`](../../.cursor/rules/engram.md).

## Auto-wake (WS-1)

Cursor agents do not see MCP until they call it. Two mitigations:

| Path | When |
|------|------|
| **Ambient file** | Run `./scripts/cursor-engram-preflight.sh` → writes `.cursor/engram-wake.md` |
| **KI bake** | With `ENGRAM_KI_ARTIFACTS_DIR=.cursor/engram-ki`, `ki_hijacker` also writes `.cursor/engram-wake.md` on serve |

**Turn-1 contract:** read `.cursor/engram-wake.md` if present → else `session_start` → execute `harness_injection.suggested_actions`.

Do **not** call `watch_workspace` at wake. Use `context_for_edit` per file.

## Optional: workspace open task

Add a Cursor task or manual hook to run `scripts/cursor-engram-preflight.sh` when opening the repo.

## Docs

- [docs/HARNESS_INJECTION.md](../../docs/HARNESS_INJECTION.md)
- [docs/AGENT_MEMORY_CONTRACT.md](../../docs/AGENT_MEMORY_CONTRACT.md)