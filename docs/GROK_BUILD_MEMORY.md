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
| Code = grep/RAG | **Code atlas v2** — `context_for_edit`: AST + `__arc` + traces/scars at locus |
| No trust model | CRS tiers, scars, lawfulness verify |

**The Grok Build integration story:** Engram is already an MCP server. Grok Build spawns it once per workspace. Agents follow an **8-tool contract** — not all 79 tools every session, not a 5-tool wake cathedral.

Native tools (e.g. `scheduler_create` for Grok's `/loop` recurring prompts) must be called **bare/direct** (never through `use_tool` or Engram MCP wrappers). Use `/engram-loop` or the equivalent ritual for Engram-aware `/loop` handling: parse per the spec, bare native call, immediate Engram `quick_trace` + `remember`/`relate` (job id to goal/tile/process), subvisor governance, honest confirmation or scar on native format error. See `grok-plugin-engram/commands/engram-loop.md`. This prevents the historical "doom loop" of misrouted native calls + false success claims.

---

## The 8-tool contract (ship this in Grok Build docs)

```
WAKE   → session_start(intent)              # inline continuation + harness_injection.suggested_actions
WORK   → context_for_edit(path)             # before editing a file
       → recall(query, scope="anchors")     # goals/traces when stuck
       → quick_trace / remember             # forks and facts
END    → session_end(summary)               # handoff packet for next session
PROBE  → get_backend_readiness              # lean vs deep, RSS-safe mode
MODE   → set_memory_mode("deep")            # only when full recall needed
```

**Load for every agent:** [`docs/AGENT_MEMORY_CONTRACT.md`](AGENT_MEMORY_CONTRACT.md) (8-tool highway) + [`docs/TOOL_DECISION_MAP.md`](TOOL_DECISION_MAP.md) (full 79-tool map) + [`docs/CODE_ATLAS_CONTINUITY.md`](CODE_ATLAS_CONTINUITY.md) (situated edit memory) + [`docs/DEFORMATION_PLAYBOOKS.md`](DEFORMATION_PLAYBOOKS.md) (JIT RSI) + [`SKILLS.md`](../SKILLS.md)

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

**Validated on:** 183k+ `.leg` blocks, ~230MB RSS, <600ms lean wake (harness), transport stable. `agent-memory` suite green including `harness_injection` queue.

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
4. **Situated code atlas** — structure + `update(__arc)` edit continuity + traces at `file:line` ([CODE_ATLAS_CONTINUITY.md](CODE_ATLAS_CONTINUITY.md))
5. **Declarative process sheaf** — `processes/*.toml`, subvisor H¹ governance
6. **MCP-native** — 8-tool lean path + power tools for depth

---

## Shipped in Agent Memory MVP

- **Core loop (A1–A6):** one-call wake, lean/deep mode, anchor recall, `context_for_edit`, structured handoff
- **`docs/AGENT_MEMORY_CONTRACT.md`** — canonical 8-tool agent entry
- **`SKILLS.md`** — points to contract first
- **Grok Build plugin** — 20+ `/engram-*` slash commands + MCP spawn per workspace
- **`docs/TOOL_DECISION_MAP.md`** — full 79-tool decision map
- **`docs/DEFORMATION_PLAYBOOKS.md`** — JIT RSI playbooks
- **`integrations/`** — MCP config templates for Cursor, Claude, Codex, Antigravity

---

## Roadmap (post-MVP)

- Async `load_process_sheaf` (faster session_start)
- `note()` write primitive (remember/update unified)
- Harness gate in CI
- Optional: mark MCP tool descriptions with `[ESSENTIAL]` / `[POWER]` / `[LEAN:AVOID]` in `mcp.rs`

**Do not delete MCP tools** — tier them in docs; removal breaks power users and TUI paths.