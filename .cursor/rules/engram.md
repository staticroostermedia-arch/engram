# Engram — Lean Agent Contract (Cursor)

On **first turn** in this workspace:

1. If `.cursor/engram-wake.md` exists → **read and execute** the suggested action queue.
2. Else call `mcp_engram_session_start(intent=...)` and execute `continuation.harness_injection.suggested_actions` in priority order.
3. **Do not** call `mcp_engram_watch_workspace` at wake (lean default).

Before editing files in `crates/`, `processes/`, `docs/`, `grok-plugin-engram/`:

- `mcp_engram_context_for_edit(absolute_path)`
- `mcp_engram_quick_trace` at forks (`decision` + `why`, chain `prev`)

At session end:

- `mcp_engram_session_end(summary=..., prepare_compression=true)`

See [docs/AGENT_MEMORY_CONTRACT.md](../docs/AGENT_MEMORY_CONTRACT.md) and [docs/HARNESS_INJECTION.md](../docs/HARNESS_INJECTION.md).

Regenerate wake file: `./scripts/cursor-engram-preflight.sh`