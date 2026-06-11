# Engram Geometric Memory — Grok Build Plugin

**Not another vector database.** Local geometric memory with structured session handoff, CRS-gated blocks, and edit-scoped code context.

## Install

### From this repo (development)

```bash
# 1. Build or install the binary
cargo build -p engram-server
# or: cargo install --path crates/engram-server

# 2. Install the plugin
grok plugin install /path/to/Engram/grok-plugin-engram --trust

# 3. Open a new Grok session (MCP spawns at session start)
```

Or use the installer script from repo root:

```bash
./scripts/install-engram-plugin.sh
```

### Ritual slash commands

| Command | When |
|---------|------|
| `/engram-wake` | Start of session — continuation bundle |
| `/engram-edit` | Before editing a file — spatial + anchor context |
| `/engram-recall` | Stuck — goals/traces/rituals first |
| `/engram-trace` | Significant decision fork |
| `/engram-handoff` | End of session — structured handoff for next wake |
| `/engram-deep` | Rare — full manifold / relation exploration |

In a Grok session, run `/engram-wake` first (or `mcp_engram_session_start`).

Expected: continuation bundle + `fully_initialized: true` within ~30s on first cold boot.

## Why Engram vs flat memory

| Flat (markdown / vectors) | Engram |
|---------------------------|--------|
| Similarity search | Goals, traces, scars as anchors |
| Session dies → lost | `session_end` handoff → next `session_start` rehydrates |
| Grep/RAG for code | `context_for_edit` — AST spatial + related traces |
| No trust model | CRS tiers + lawfulness verify |

## 8-tool contract

Wake → `session_start` · Work → `context_for_edit` + `recall` + `quick_trace` · End → `session_end`

See skill `engram-memory` and [docs/AGENT_MEMORY_CONTRACT.md](../docs/AGENT_MEMORY_CONTRACT.md).

## Troubleshooting

| Issue | Fix |
|-------|-----|
| MCP "not found" | Open a **new Grok session** after install |
| `grok mcp doctor` fails while TUI open | **Expected** — only one `engram mcp` per store. Run `./scripts/engram-mcp-health.sh` instead |
| Lock error on restart | Close Grok session or `pkill -f "engram.*mcp"` then new session |
| Binary missing | Run `scripts/install-engram-plugin.sh` |

## License

Same as parent Engram repository.