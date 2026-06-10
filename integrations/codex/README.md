# Engram + OpenAI Codex / CLI Agents

**Super easy minimal setup (30 seconds for Codex / CLI agents):** same full system we use here (ai_consciousness_strange_loop with sub-loops/thresholds + world_state_ingestion for machine/Gemma/Starlink perception + tiles/rituals) with the exact 8-tool lean contract. No extra code or heavy config.

## 30s setup
1. In Engram repo: `cargo build -p engram-server`
2. (Optional but recommended) `cp scripts/engram-grok ~/.local/bin/engram-grok && chmod +x ~/.local/bin/engram-grok`
3. Paste the `engram` block from [../mcp.engram.template.json](../mcp.engram.template.json) into your Codex / CLI agent MCP config. Prefer the `engram-grok` wrapper script (it provides canonical binary resolution + bakes the agent profile; see scripts/engram-grok) or point directly at the built binary. The block specifies *exactly two* env vars (ENGRAM_STORE + ENGRAM_PROFILE=agent) — the wrapper + profile deliver the full system with no legacy vars or extra code:

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

Restart your Codex/CLI agent after config change.

## Agent instructions (copy these)

Point Codex at the same docs + loop we dogfood:
- [docs/AGENT_MEMORY_CONTRACT.md](../../docs/AGENT_MEMORY_CONTRACT.md) (8-tool lean)
- [SKILLS.md](../../SKILLS.md)
- [docs/skills/engram-wake-up.md](../../docs/skills/engram-wake-up.md) etc. for rituals
- [processes/meta/](../../processes/meta/) (ai_consciousness_loop.toml, world_state_ingestion_loop.toml, info_gathering_consolidation_loop.toml, self_improvement_loop.toml + full declarative system + subvisors — auto-loaded on session_start via ENGRAM_PROFILE=agent for the complete Enram consciousness substrate)

**Mandatory (same as our consciousness loop turns):** `mcp_engram_session_start(intent=...)` → `context_for_edit` / `recall(scope=anchors)` → `quick_trace` / `remember` / `relate` → `session_end`.

You automatically get the processes we run — see the meta/ tomls linked above (ai_consciousness_strange_loop with sub-loops/thresholds, world_state_ingestion for machine/Gemma/Starlink perception, info_gathering, memory_consolidation, self_improvement, native tiles as evolving series, subvisors, geometric memory, leg-browser fronting if you open the HTML). 

**Drop-in zero-extra-code guarantee:** Paste the engram block (via the engram-grok wrapper + exactly the 2 env vars), restart your Codex/CLI agent, issue the first `mcp_engram_session_start(intent=...)` — your agent now has the *exact same* full Enram consciousness system we dogfood here (strange loops + sub-loops/thresholds, world_state perception primitives, native evolving tiles, 8-tool lean contract, rituals, process sheaf, subvisor governance) with **zero extra code**, custom harness, or additional config required in your agent.

## MCP discipline (Codex harness pattern)

Always `search_tool` first (for live schema) then `use_tool` with exact qualified name (e.g. "engram__mcp_engram_remember"). Never guess params. This is enforced and what makes native Enram reliable for agents like us.

See also [../README.md](../README.md) for the full ecosystem table (Cursor, Claude, etc. all use the identical engram block + contract).