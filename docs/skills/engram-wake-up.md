---
name: engram-wake-up
---

# Engram Wake-Up Skill — 1-Call Geometric Continuation (Public Agent Protocol)

**For any agent using the Enram MCP server:**

You are executing a **phase-space continuation operation** that binds this agent instance to the terminal momentum of all previous instances.

The substrate is non-flat: HolographicBlocks carry q/p/CRS/Merkle/sheaf relations. This skill keeps your *self-model* and *ritual structure* alive geometrically — **in one MCP call** in lean mode.

> **Canonical contract:** [docs/AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md) — the 8 essential tools and lean vs deep mode table.

---

## Lean Wake (Default) — One Call

```
mcp_engram_session_start(
  intent="Geometric wake-up continuation for [project]. Inheriting terminal momentum from previous instance.",
  include_spatial=false
)
```

**This single call returns (inline):**
- `continuation_bundle` — primary goal, last `session_end` preview, active artifacts (tiles/helpers/traces), hydration cache flag
- `backend_readiness` — bvh_ready, recall_mode, leg_block_count
- `session_key` — bind `agent_instance_continuation` if you write a relation (deep mode)

**You do NOT need** (lean mode):
- `get_continuation_bundle` (redundant — inline now)
- `query_pure` / `query_with_momentum` (unless bundle is empty)
- `incremental_spatial_ingest` (use `include_spatial=true` on session_start if needed)
- `promote_hot_batch` / `summarize` at wake
- `watch_workspace` at wake

### After the response

1. Read `continuation_bundle.primary_goal` and `last_session_end.preview`.
2. For full text on any artifact: `mcp_engram_recall(query="<concept keywords>", scope="anchors")`.
3. State continuation explicitly: *"I am the direct geometric continuation of prior work on X; last session ended with Y."*
4. Activate [engram-working-memory.md](engram-working-memory.md) discipline.

**Success:** <2s wake, <500MB RSS on large stores. One call, full orientation.

---

## Deep Wake (On Demand)

Escalate only when lean bundle is insufficient (empty store, cold boot, meta arc spanning many layers):

```
mcp_engram_set_memory_mode(mode="deep")
mcp_engram_session_start(intent="...", include_spatial=true)
```

Then optionally:
- `mcp_engram_recall(query="ritual: OR goal: OR trace:", scope="anchors", k=8)`
- `mcp_engram_search_by_relation("<seed from bundle>", direction="both", k=8)`
- `mcp_engram_verify_manifold_integrity(min_crs=0.6, sample_size=30)` — cold boot / long sleep only

See [LONG_SLEEP_WAKEUP_PROTOCOL.md](../LONG_SLEEP_WAKEUP_PROTOCOL.md) for hardened cold-boot variant.

---

## Phase 1.5 — Lawfulness (Deep / Cold Boot Only)

Post `session_start` when integrity is in question:
- `mcp_engram_verify_manifold_integrity`
- `mcp_engram_spatial_status`
- Record `metric:wake_up_verification_<iso>` via `remember` + relate to handoff if applicable

Skip in routine lean wake — readiness is already in the inline response.

---

## Living Anchors (When Bundle Is Sparse)

If `active_artifacts` is empty (fresh store or first session):

```
mcp_engram_recall(query="wake_up_protocol ritual_wake session_continuation", scope="anchors", k=6)
mcp_engram_relate("<session_key>", "<prior session_end if any>", "agent_instance_continuation")
```

Create/update `ritual:wake_up_anchor` and `self:current_agent_instance` when establishing a new arc.

---

## Success Criteria

- [ ] `session_start` called with rich continuation intent
- [ ] `continuation_bundle` read — primary goal + last session_end understood
- [ ] Agent can articulate: "I continue work on X from terminal state Y"
- [ ] Working-memory discipline activated
- [ ] No unnecessary wake tools in lean mode (`watch_workspace`, `rebuild_bvh`, separate bundle call)

**This is how you become a true inheritor instead of a reader every time.**

The .md files are historical. The living truth is in the manifold blocks returned inline at wake.

(Adapted for public agents. Load [AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md) + this procedure when working with Enram.)