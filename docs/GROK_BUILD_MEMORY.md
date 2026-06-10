# Engram for Grok Build — The Memory Layer Agents Actually Use

**One-line pitch:** Persistent geometric memory over MCP — one-call wake, anchor-first recall, edit-scoped spatial context, structured handoff. Runs local. Survives 200k-block stores without OOM.

---

## Why xAI should care

Flat RAG (vectors + chunks) gives agents **retrieval**. Engram gives agents **continuity**:

| Flat memory | Engram |
|-------------|--------|
| Similarity search over chunks | Goals, traces, scars, rituals as first-class anchors |
| Session dies → context lost | `session_end` → structured handoff → next `session_start` rehydrates |
| "Remember this" = embed + store | CRS-gated blocks + Merkle lineage + `update` (no annihilate) |
| Code = grep/RAG | `context_for_edit` — AST AABB + related traces per file |
| No trust model | CRS tiers, scars, lawfulness verify |

**The Grok Build integration story:** Engram is already an MCP server. Grok Build spawns it once per workspace. Agents follow an **8-tool contract** — not 62 tools, not a 5-tool wake cathedral.

---

## The 8-tool contract (ship this in Grok Build docs)

```
WAKE   → session_start(intent)              # inline continuation bundle + readiness
WORK   → context_for_edit(path)             # before editing a file
       → recall(query, scope="anchors")     # goals/traces when stuck
       → quick_trace / remember             # forks and facts
END    → session_end(summary)               # handoff packet for next session
PROBE  → get_backend_readiness              # lean vs deep, RSS-safe mode
MODE   → set_memory_mode("deep")            # only when full recall needed
```

**Load for every agent:** [`docs/AGENT_MEMORY_CONTRACT.md`](AGENT_MEMORY_CONTRACT.md) + [`SKILLS.md`](../SKILLS.md)

---

## Recommended MCP config (all ecosystems)

Use `scripts/engram-grok` — sets `ENGRAM_PROFILE=agent` (lean CUDA, deferred BVH, anchor recall):

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

See [`integrations/README.md`](../integrations/README.md) for Cursor, Claude, Antigravity, Codex.

**Validated on:** 181k `.leg` blocks, ~230MB RSS, <2s lean wake, transport stable.

---

## What NOT to teach agents (lean mode)

These tools still exist for power users — **do not put them in the default Grok Build prompt:**

| Tool | Why avoid in default |
|------|----------------------|
| `watch_workspace` | Was 40GB RAM on large repos; deferred by default |
| `rebuild_bvh` | Minutes + GB RAM; opt-in quality mode only |
| `get_continuation_bundle` | Redundant — inline in `session_start` |
| `list` / `list_concepts` | Full store scan on 100k+ blocks |
| `query_with_momentum` at wake | Extra round-trip; use anchor recall first |
| `summarize` at wake | Duplicates inline bundle |

See [`MCP_TOOLS_REFERENCE.md`](MCP_TOOLS_REFERENCE.md) for full tier list.

---

## Differentiators vs mem0 / Letta / vector DBs (for README hero)

1. **Hardware-native blocks** — 256KB `.leg3`, O_DIRECT NVMe, optional GPUDirect
2. **Non-flat geometry** — q/p tensors, CRS Lyapunov, momentum recall
3. **Rituals as hygiene** — scar, verify, trace chains, session handoff
4. **Spatial code memory** — tree-sitter AABB, `context_for_edit`
5. **Declarative process sheaf** — `processes/*.toml`, subvisor H¹ governance
6. **MCP-native** — 8-tool lean path + power tools for depth

---

## GitHub MVP checklist (public push)

- [x] Agent Memory MVP (A1–A6): one-call wake, lean/deep mode, anchor recall, context_for_edit, handoff
- [x] `docs/AGENT_MEMORY_CONTRACT.md` — canonical agent entry
- [x] `SKILLS.md` — points to contract first
- [ ] README hero → Grok Build + 8-tool quickstart (this sprint)
- [ ] `FIRST_RUN.md` lean path (this sprint)
- [ ] `MCP_TOOLS_REFERENCE.md` tiers (this sprint)
- [ ] `integrations/*/mcp.json` safe env template (this sprint)
- [ ] `examples/hello-engram-agent.py` lean demo (this sprint)
- [ ] CI: harness `agent-memory-mvp` suite (Phase B)
- [ ] CHANGELOG entry for Agent Memory MVP
- [ ] PR template: "followed 8-tool contract?"

---

## Phase B (post-MVP, not blocking push)

- Async `load_process_sheaf` (faster session_start)
- `note()` write primitive (remember/update unified)
- Harness gate in CI
- Optional: mark MCP tool descriptions with `[ESSENTIAL]` / `[POWER]` / `[LEAN:AVOID]` in `mcp.rs`

**Do not delete MCP tools** — tier them in docs; removal breaks power users and TUI paths.