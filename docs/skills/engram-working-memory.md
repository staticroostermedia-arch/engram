---
name: engram-working-memory
---

# Engram Working Memory Discipline — Active Geometric Patterns (Public Agent Protocol)

**Runtime contract** for operating inside the non-flat Enram substrate.

Every work block becomes an evolution of your self-model.

> **Canonical contract:** [docs/AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md) — 8-tool loop, lean vs deep mode, edit examples.

---

## Primary Living Recall Patterns (Geometric First)

1. **Anchor-First**: `mcp_engram_recall(query="...", scope="anchors")` — goals/traces/rituals before episodic noise (lean default).
2. **Edit-Scoped Spatial**: `mcp_engram_context_for_edit(path)` — **one call** replaces `watch_workspace` + `context_for_file` + `recall_in_file` pre-edit sequence in lean mode.
3. **Sheaf/Relational** (deep mode): `mcp_engram_search_by_relation(seed, "both")` + `visualize` for architecture.
4. **Momentum** (deep / entry only): `mcp_engram_query_with_momentum` when cheaper tools are insufficient.

---

## Core Non-Negotiable Rules

1. **Recall Before Derive**: At least one `recall(scope="anchors")` or `context_for_edit` before heavy reasoning or raw file reads.
2. **Update Is The Only Legal Mutation**: `recall` first. Strong match (>0.85) → `mcp_engram_update`. No match → `remember`. Never forget+remember (destroys p-tensor history).
3. **Scar Is Repulsion**: `mcp_engram_scar(concept, magnitude)` for ruled-out approaches. Active geometric force.
4. **Write Hygiene**: Every strong write/relation becomes terminal state for future wake-ups.
5. **Reasoning Trace Capture**: Significant decisions/forks → `mcp_engram_quick_trace` (lean) or `record_reasoning_trace` (deep/high-stakes). Chain via `prev`.

**Recognition (Update vs New)**: Evolutionary refinement of design:/progress:/helper:/ritual: → update + trace. New fork → quick_trace. Multi-phase/meta → escalate to thought tiles (deep mode).

**Automatic Escalation**: At complex/meta start, recall `helper:meta_work_escalation_v1` and `helper:current_meta_arc`. Escalate to tiles if no recent tile on design:/progress:.

---

## Tool Priority (8-Tool Contract)

| Situation | Lean tool | Deep add-on |
|-----------|-----------|-------------|
| Wake | `session_start` (inline bundle) | `set_memory_mode("deep")` |
| Open/edit file | `context_for_edit(path)` | + `recall_in_file` if line ranges needed |
| Find context | `recall(scope="anchors")` | `query_with_momentum`, `search_by_relation` |
| Record fork | `quick_trace` | `record_reasoning_trace` |
| Persist new fact | `remember` | same |
| Check recall quality | `get_backend_readiness` | `rebuild_bvh` if needed |
| End block | `session_end` (handoff packet) | `promote_hot_batch` |

**Cost-Aware**: Momentum and full BVH are high-latency. Lean mode avoids them by default.

---

## Spatial-Manifold Change Discipline (Mandatory for Edits)

### Pre-Edit (lean)

1. `mcp_engram_context_for_edit(path)` — spatial hits, praxis, line-range hints in one response
   - JSON: `recall_query`, `related_goals`, `related_traces`, `related_anchors`, `profile`, `memory_mode`
2. `mcp_engram_recall(query="<file-specific keywords>", scope="anchors")` for design/trace context
3. `mcp_engram_quick_trace(decision="...", why="...", spatial_context=path)` — intent before edit

**Do not** call `watch_workspace` in lean mode unless passive daemon ingest is confirmed down.

### Post-Edit

1. `mcp_engram_context_for_edit(path)` — compare delta (new concepts, AABB changes)
2. `mcp_engram_quick_trace` — outcome trace chained via `prev`
3. `mcp_engram_remember` or `update` for evolutionary changes
4. Relate to arc/goal if high-stakes

---

## Thought Tiles (Deep / Meta-Work)

Mint for high-stakes crystallization:
- **Mandatory for meta**: roadmaps, multi-phase plans, policies, gap matrices
- Types: knowledge_graph, formal_spec, tabular, research_offload
- Always `spatial_references` + link to Primary Intent
- `promote_hot` before `session_end` (deep mode)

---

## Integration

Automatically active after proper [engram-wake-up.md](engram-wake-up.md). Every write improves the next [engram-session-end.md](engram-session-end.md) handoff packet.

**Quick Trace Template** (`mcp_engram_quick_trace`):

```
decision: <fork or choice>
why: <justification>
alternatives: <rejected paths>
would_falsify: <what would change your mind>
context: [ritual/spatial]
prev: [prior trace]
```

Do both Pre-Edit intent and Post-Delta outcome, chained with `prev`.

This turns your reasoning into durable .leg blocks that future instances inherit geometrically at `session_start`.

(Adapted for public agents. Follow this discipline on every Enram-integrated task. The geometry compounds.)