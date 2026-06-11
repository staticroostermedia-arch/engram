# Integration Rules (redirect)

**Start here:** [integrations/README.md](integrations/README.md) — MCP configs for Grok, Cursor, Claude, Codex, Antigravity.

**Agent contract:** [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md) — recall before remember, `update` not forget+remember, `session_end` every block.

**Lean loop:**

```
session_start → context_for_edit → recall(anchors) → quick_trace / remember → session_end
```