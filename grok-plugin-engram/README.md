# Engram Geometric Memory — Grok Build Plugin

**Not another vector database.** Local geometric memory with structured session handoff, CRS-gated blocks, and edit-scoped code context.

**Primary user: the AI agent.** Slash commands map decision moments → MCP rituals. See [commands/README.md](commands/README.md).

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

## Slash commands (20)

### Session boundary
| Command | When |
|---------|------|
| `/engram-wake` | Start — continuation bundle |
| `/engram-session-end` | End — structured handoff |

### Work loop
| Command | When |
|---------|------|
| `/engram-edit` | Before editing a file |
| `/engram-recall` | Stuck — goals/traces first |
| `/engram-read` | Full concept body after recall |
| `/engram-ready` | Probe recall mode / BVH |
| `/engram-trace` | Decision fork |

### Write path
| Command | When |
|---------|------|
| `/engram-update` | Refine existing concept |
| `/engram-remember` | New concept (no match) |
| `/engram-solution` | Verified fix → praxis |
| `/engram-scar` | Dead end repulsion |
| `/engram-relate` | Graph edge between concepts |

### Read escalation
| Command | When |
|---------|------|
| `/engram-momentum` | What's trending |
| `/engram-pure` | Geometric similarity |
| `/engram-graph` | Graph walk + visualize |

### Meta & mode
| Command | When |
|---------|------|
| `/engram-tile` | Multi-phase meta arc |
| `/engram-goal` | Goal stack / primary |
| `/engram-deep` | Full manifold (sparingly) |
| `/engram-lean` | Return to fast default |
| `/engram-verify` | Lawfulness check |
| `/engram-ingest` | Spatial recovery |

Run `/engram-wake` first in every session.

## Why Engram vs flat memory

| Flat (markdown / vectors) | Engram |
|---------------------------|--------|
| Similarity search | Goals, traces, scars as anchors |
| Session dies → lost | `session_end` handoff → next `session_start` rehydrates |
| Grep/RAG for code | `context_for_edit` — AST spatial + related traces |
| No trust model | CRS tiers + lawfulness verify |

## Skills & docs

**Skills:** `engram-memory` (index) · `engram-wake-up` · `engram-working-memory` · `engram-session-end`

- [docs/AGENT_MEMORY_CONTRACT.md](../docs/AGENT_MEMORY_CONTRACT.md) — 8-tool highway
- [docs/TOOL_DECISION_MAP.md](../docs/TOOL_DECISION_MAP.md) — all 66 tools
- [commands/README.md](commands/README.md) — agent moment → command map

**Tools must be called** — documentation alone does not persist memory.

## Troubleshooting

| Issue | Fix |
|-------|-----|
| MCP "not found" | Open a **new Grok session** after install |
| `grok mcp doctor` fails while TUI open | **Expected** — only one `engram mcp` per store. Run `./scripts/engram-mcp-health.sh` instead |
| Lock error on restart | Close Grok session or `pkill -f "engram.*mcp"` then new session |
| Binary missing | Run `scripts/install-engram-plugin.sh` |

## License

Same as parent Engram repository.