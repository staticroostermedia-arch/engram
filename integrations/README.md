# Engram — Agent Ecosystem Integrations

**One memory substrate, one agent contract.** Every IDE gets the same 8-tool lean path.

**Read first:** [docs/AGENT_MEMORY_CONTRACT.md](../docs/AGENT_MEMORY_CONTRACT.md)

---

## Canonical MCP config (all ecosystems)

Use `scripts/engram-grok` as the launcher — it sets `ENGRAM_PROFILE=agent` and resolves the binary automatically.

```json
{
  "mcpServers": {
    "engram": {
      "command": "/path/to/Engram/scripts/engram-grok",
      "args": ["mcp"],
      "env": {
        "ENGRAM_STORE": "~/.engram/stalks/",
        "ENGRAM_PROFILE": "agent"
      }
    }
  }
}
```

**Only two env vars.** Do not copy legacy 8-var blocks (`ENGRAM_DEFER_BVH`, `ENGRAM_KI_DISABLE`, …) — the profile layer sets them.

Template with placeholder: [`mcp.engram.template.json`](mcp.engram.template.json)

---

## Per-ecosystem setup

| Ecosystem | Config file | Notes |
|-----------|-------------|-------|
| **Grok Build / TUI** | `~/.grok/config.toml` | See [grok-build/mcp.json](grok-build/mcp.json) — TOML `command` + `env` |
| **Cursor** | `.cursor/mcp.json` or project copy | [cursor/mcp.json](cursor/mcp.json) — `${workspaceFolder}` when Engram is the workspace |
| **Claude Desktop** | `claude_desktop_config.json` | [claude-desktop/](claude-desktop/) |
| **Antigravity** | `mcp_config.json` | [antigravity/mcp_config.json](antigravity/mcp_config.json) — engram block unified; other servers optional |
| **OpenAI Codex / CLI agents** *(advanced)* | Project instructions | [codex/README.md](codex/README.md) |
| **Local / custom** | Any MCP client | `cargo install --path crates/engram-server` + `engram-grok` on PATH |

After any config change: **restart the IDE or TUI** so MCP respawns.

---

## Agent instructions (all ecosystems)

Load into system prompt / project rules / `AGENTS.md`:

1. [SKILLS.md](../SKILLS.md) — entry point
2. [docs/AGENT_MEMORY_CONTRACT.md](../docs/AGENT_MEMORY_CONTRACT.md) — 8 tools
3. [docs/skills/engram-wake-up.md](../docs/skills/engram-wake-up.md) — one-call wake
4. [integrations/system-prompts/README.md](system-prompts/README.md) — copy-paste blocks
5. [processes/](../processes/) — declarative ritual/harness/monitor sheaf (auto-loaded on `session_start` via `ENGRAM_PROFILE=agent`)

**Lean wake (mandatory):**

```
session_start(intent) → ack_wake_queue → context_for_edit(path) → recall(scope=anchors) → quick_trace / remember → session_end(summary)
```

With `ENGRAM_PROFILE=agent`, wake gate defaults to **hard** — `ack_wake_queue` is required before `context_for_edit` (empty queue auto-acks at wake).

**Do not** at wake: `watch_workspace`, `summarize`, `rebuild_bvh`. Slim `session_start` is the default wake payload; call `get_continuation_bundle` only when you need the full harness inline.

---

## Install paths

| Mode | `command` value |
|------|-----------------|
| **Dev (this repo)** | `<repo>/scripts/engram-grok` |
| **Global launcher** | `~/.local/bin/engram-grok` after `cp scripts/engram-grok ~/.local/bin/` |
| **cargo install** | `engram-grok` on PATH, or `engram` with `ENGRAM_PROFILE=agent` in env |

Build: `cargo build -p engram-server` → binary at `target/debug/engram`.

---

## Workflows (optional deep path)

- [workflows/wake_up.md](workflows/wake_up.md) — aligned to 8-tool contract
- [workflows/session_end.md](workflows/session_end.md)
- [workflows/working_memory.md](workflows/working_memory.md)

Deep rituals: [docs/RITUALS.md](../docs/RITUALS.md)

---

## Validation

```bash
cargo build -p engram-server
STABLE_BIN=target/debug/engram tools/test-harness/bin/engram-harness.sh --suite agent-memory
```

First agent call: `mcp_engram_session_start(intent="…")` — expect `profile: agent` in readiness.