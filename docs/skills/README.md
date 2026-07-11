---
name: engram-skills-for-agents
---

# Engram Skills for Agents (Public Ritual Protocols)

These are the operational skills and rituals that power the Engram geometric memory system for AI agents.

**If you are an agent (Grok, Claude, custom, etc.) using Engram:**
- Connect to the `engram` MCP server (see main README and docs/MCP_TOOLS_REFERENCE.md).
- **Load these skills** at the start of your context or when beginning work on an Engram-integrated project.
- Follow them exactly for wake-up, working memory discipline, session termination, thought tiles, goal management, and spatial/Code Edit rituals.
- The goal is *geometric continuation* across sessions instead of flat context reset. This is "Against Flat Knowledge" made operational.

## Core Ritual Loop (8-tool lean — all ecosystems)

**Contract:** [docs/AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md)  
**Full map:** [docs/TOOL_DECISION_MAP.md](../TOOL_DECISION_MAP.md) (Layers 0–4, write path, read escalation)

1. **Wake:** `session_start(intent)` — one call (see `engram-wake-up.md`).
2. **Work:** `context_for_edit` → `recall(anchors)` → `quick_trace` / `remember` (`engram-working-memory.md`).
3. **End:** `session_end(summary)` — handoff packet (`engram-session-end.md`).

## Additional Powerful Skills

- Thought Tiles (`engram-thought-tiles`): For structured offload of plans, policies, knowledge graphs. Mandatory for meta-work.
- Personal wiki (`engram-leg-wiki-starter`): Bootstrap and maintain a compounding knowledge wiki with LEG Browser. See [docs/PERSONAL_KNOWLEDGE_WIKI.md](../PERSONAL_KNOWLEDGE_WIKI.md).
- Glass-Box RSI (`engram-glassbox-rsi`): Hybrid fire goals + typed verify for scheduled Dual RSI / Ship / PR / Stale / Aliveness loops; LEG glass box. See [engram-glassbox-rsi.md](engram-glassbox-rsi.md); loop bodies in [loop-prompts/](loop-prompts/).
- Goal Stack (`engram-goal`): First-class intentional self-model. Primary goal auto-links to traces.
- Spatial (Item 1.5): **lean:** `context_for_edit(path)`; **deep:** optional `watch_workspace` once per project.
- Lawfulness: `mcp_engram_verify_manifold_integrity`, block lawfulness.

See the full [docs/RITUALS.md](../RITUALS.md) for overview, [docs/MCP_TOOLS_REFERENCE.md](../MCP_TOOLS_REFERENCE.md) for all 79 tools (8 essential), [docs/DEFORMATION_PLAYBOOKS.md](../DEFORMATION_PLAYBOOKS.md) for JIT RSI, and [docs/GEOMETRIC_MEMORY.md](../GEOMETRIC_MEMORY.md) for the non-flat model.

**Recommended for contributors:** Use these rituals on real task work so the substrate records traces and handoff for the next session.

These files are the published operating procedures for agent behavior on top of the Engram substrate.

## Quick Start for a New Agent Instance

```
# 1. Connect MCP (engram)
# 2. Call mcp_engram_session_start with rich intent
# 3. Load + follow docs/skills/engram-wake-up.md
# 4. Do work following docs/skills/engram-working-memory.md
# 5. End with docs/skills/engram-session-end.md
```

This produces real continuation via the manifold (agent_instance_continuation relations, hot paths, COMPRESS, etc.).

For the declarative process sheaf (rituals as first-class toml), see the committed `processes/` directory.

---

These skills are the full operating procedures — not summaries. They turn flat context reset into geometric continuation: traces, goals, and handoff packets that the next session can rehydrate.

If something is missing or unclear, record it in the manifold (`scar` + `trace` + tile) and improve the docs.