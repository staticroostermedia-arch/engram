# Engram System Prompt Additions — All Agent Ecosystems

> Copy into IDE custom instructions, `.cursorrules`, `AGENTS.md`, or Codex project prompts.

**Canonical contract:** [docs/AGENT_MEMORY_CONTRACT.md](../../docs/AGENT_MEMORY_CONTRACT.md)

---

## Universal Core (2026 — 8-tool lean contract)

```markdown
## Engram Memory Protocol

Persistent geometric memory via MCP. **Lean by default.**

### Wake (one call)
- `mcp_engram_session_start(intent="<your goal>")` — returns continuation bundle + readiness

### Work
- Before editing a file: `mcp_engram_context_for_edit(absolute_path)`
- When stuck: `mcp_engram_recall(query, scope="anchors")`
- At forks: `mcp_engram_quick_trace(decision, why, goal_context=...)`
- New facts: `mcp_engram_remember` (recall first; use `update` if match > 0.85)

### End (mandatory)
- `mcp_engram_session_end(summary="<decisions, files, open questions>")`

### Probe
- `mcp_engram_get_backend_readiness()` — check profile, recall_mode

### Do NOT at wake (lean mode)
- watch_workspace, summarize, rebuild_bvh, get_continuation_bundle, query_with_momentum

### Rules
- Never `forget` + `remember` — use `mcp_engram_update`
- Scar dead-ends: `mcp_engram_scar`
- MCP: search_tool first, then use_tool with exact schema
```

---

## MCP config (all IDEs)

See [../README.md](../README.md). Minimal env:

```json
"env": {
  "ENGRAM_STORE": "~/.engram/stalks/",
  "ENGRAM_PROFILE": "agent"
}
```

Launcher: `scripts/engram-grok` (sets profile defaults automatically).

---

## Per-ecosystem

| Ecosystem | Config | Instructions file |
|-----------|--------|-------------------|
| Grok Build | `~/.grok/config.toml` | Load AGENT_MEMORY_CONTRACT |
| Cursor | `.cursor/mcp.json` + rules | `integrations/cursor/mcp.json` |
| Claude Desktop | `claude_desktop_config.json` | Project custom instructions (block above) |
| Antigravity | `mcp_config.json` | `integrations/antigravity/` |
| Codex / CLI | MCP config in harness | `integrations/codex/README.md` |
| Repo agents | `AGENTS.md` + `CLAUDE.md` | Already in repo root |

---

## Deep mode (optional)

`mcp_engram_set_memory_mode(mode="deep")` for full manifold, relation graphs, lawfulness audits. Reset to lean before long meta sessions.

See [docs/RITUALS.md](../../docs/RITUALS.md).