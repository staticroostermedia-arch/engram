# Engram Geometric Memory — Grok Build Plugin

**Ritual skills:** canonical source is `docs/skills/` in the repo root. Plugin copies under `grok-plugin-engram/skills/` should stay in sync at release time.

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
| `/engram-wake` | Start — slim continuation bundle (`session_start`) |
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
- [docs/TOOL_DECISION_MAP.md](../docs/TOOL_DECISION_MAP.md) — all 70 tools
- [docs/DEFORMATION_PLAYBOOKS.md](../docs/DEFORMATION_PLAYBOOKS.md) — JIT RSI
- [commands/README.md](commands/README.md) — agent moment → command map

**Tools must be called** — documentation alone does not persist memory.

## Troubleshooting

| Issue | Fix |
|-------|-----|
| MCP "not found" | Open a **new Grok session** after install |
| `grok mcp doctor` fails while TUI open | **Expected** — only one `engram mcp` per store. Run `./scripts/engram-mcp-health.sh` instead |
| Generic TUI commands like `/goal` not appearing in autocomplete | When the Engram plugin + MCP is active, the session toolset uses Engram's goal system (`/engram-goal` + `mcp_engram_goal_*` tools) for persistent, geometric goals. Generic TUI `/goal` (simple session goals via `update_goal`) may not show because it requires the plain `update_goal` builtin in the toolset. This is expected for Engram users. Use the Engram equivalents for work that should persist in the manifold. Generic TUI builtins should still be available for other features. See coexistence notes below and [docs/PERSONAL_KNOWLEDGE_WIKI.md](../docs/PERSONAL_KNOWLEDGE_WIKI.md). |
| Lock error on restart | Close Grok session or `pkill -f "engram.*mcp"` then new session |
| Binary missing | Run `scripts/install-engram-plugin.sh` |

## Coexistence with Generic TUI Features

Engram is designed to augment the Grok Build TUI with a superior geometric memory substrate, not to replace core TUI conveniences.

- **Slash commands**: Engram adds `/engram-*` (20+ commands for wake, edit, recall, goals, tiles, etc.). Generic TUI commands (`/goal`, `/loop`, `/btw`, `/theme`, `/feedback`, etc.) are intended to remain available.
- **Goals**: Use `/engram-goal` + Engram MCP goal tools for anything that should be persistent, traceable, and linked to tiles or your personal knowledge wiki. The generic TUI `/goal` is for lightweight, ephemeral session objectives.
- **Multi-chat / multi-agent use**: Many chats and agents can contribute to the same manifold. Engram's namespace, continuation, and sub-governor features support this pattern. See [docs/PERSONAL_KNOWLEDGE_WIKI.md](../docs/PERSONAL_KNOWLEDGE_WIKI.md) for setup and coexistence guidance.
- If a generic TUI feature disappears in your session, open a new chat or check with `/context`. Report it — we want full coexistence so using Engram doesn't mean losing built-in TUI power.

Coexistence behavior is actively improved; contributors are encouraged to trace issues and fixes in the manifold.

## License

Same as parent Engram repository.