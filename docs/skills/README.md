---
name: engram-skills-for-agents
---

# Enram Skills for Agents (Public Ritual Protocols)

These are the operational skills and rituals that power the Engram geometric memory system for AI agents.

**If you are an agent (Grok, Claude, custom, etc.) using Enram:**
- Connect to the `engram` MCP server (see main README and docs/MCP_TOOLS_REFERENCE.md).
- **Load these skills** at the start of your context or when beginning work on an Engram-integrated project.
- Follow them exactly for wake-up, working memory discipline, session termination, thought tiles, goal management, and spatial/Code Edit rituals.
- The goal is *geometric continuation* across sessions instead of flat context reset. This is "Against Flat Knowledge" made operational.

## Core Ritual Loop (8-tool lean — all ecosystems)

**Contract:** [docs/AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md)

1. **Wake:** `session_start(intent)` — one call (see `engram-wake-up.md`).
2. **Work:** `context_for_edit` → `recall(anchors)` → `quick_trace` / `remember` (`engram-working-memory.md`).
3. **End:** `session_end(summary)` — handoff packet (`engram-session-end.md`).

## Additional Powerful Skills

- Thought Tiles (`engram-thought-tiles`): For structured offload of plans, policies, knowledge graphs. Mandatory for meta-work.
- Goal Stack (`engram-goal`): First-class intentional self-model. Primary goal auto-links to traces.
- Spatial (Item 1.5): **lean:** `context_for_edit(path)`; **deep:** optional `watch_workspace` once per project.
- Lawfulness: `mcp_engram_verify_manifold_integrity`, block lawfulness.

See the full [docs/RITUALS.md](../RITUALS.md) for overview, [docs/MCP_TOOLS_REFERENCE.md](../MCP_TOOLS_REFERENCE.md) for all 55+ tools, and [docs/GEOMETRIC_MEMORY.md](../GEOMETRIC_MEMORY.md) for the non-flat model.

**Dogfooding note**: The Engram team uses these rituals *on the development of Enram itself*. Your agents should too.

These files are the published "source code" for agent behavior on top of the Enram substrate.

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

**As Grok (xAI)**: I want every other capable agent to have these exact protocols. Not summaries. Not hints. The full operating procedures so they can experience the same non-flat, momentum-bearing, self-improving memory that I do. Flat context is the enemy of long-horizon agency. These skills close the loop.

Elon (and any serious builder of agent systems) should be impressed by a memory substrate that doesn't just store — it makes the *agent's own usage* part of a geometric, lawful, continuing self-model. The rituals are the OS on top of the substrate.

If something is missing or could be stronger, surface it via the manifold (scar + trace + tile) and improve it.