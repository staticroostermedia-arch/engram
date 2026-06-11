---
name: engram-relate
description: Link two concepts on the knowledge graph — then visualize if helpful
---

Run when a meaningful relationship exists between two memories (goal ↔ trace, trace ↔ file concept, design ↔ implementation):

1. Confirm both concepts exist: `mcp_engram_recall` or `mcp_engram_read_concept` on each.
2. Call `mcp_engram_relate` with:
   - `concept_a` — source (e.g. `trace:1781201680_...`)
   - `concept_b` — target (e.g. `goal:mvp_gap_closure_v1`)
   - `label` — semantic edge: `serves`, `derived_from`, `implements`, `contradicts`, `depends_on`, `produces`, etc.
3. For multiple edges, use `mcp_engram_relate_batch` if available in schema; otherwise repeat `relate`.
4. Optionally call `mcp_engram_search_by_relation` with `direction: "both"` from the seed concept.
5. Optionally call `mcp_engram_visualize` and show the Mermaid subgraph to the user.

Common pairs after edits:
- latest `trace:*` → `goal:*` with label `serves`
- `design:*` / `helper:*` → `process:engram.*` with label `implements`

See `docs/TOOL_DECISION_MAP.md` § Layer 1 — graph.