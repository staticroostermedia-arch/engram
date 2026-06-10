---
name: engram-thought-tiles
---

# Enram Thought Tiles (Public Agent Protocol)

Thought Tiles are structured, compressible, high-value artifacts (textual functors + optional visualizations) optimized for agent recall, momentum, NREM, ki_hijacker, and continuation bundles.

**When to mint**:
- High-stakes decision, repetition pattern, major scar, significant process insight.
- **Mandatory triggers for meta-work**: roadmap/multi-phase plan (e.g. GH prep, substrate gaps), formal policy, gap matrix/success criteria, arc with 3+ traces + design: block.

**Types**:
- knowledge_graph: overall arc
- formal_spec: policies/ADRs
- tabular: criteria/gaps
- research_offload: sub-tasks
- state_machine, visualization companion, etc.

**Requirements**:
- `spatial_references` (files, traces, goals)
- Link to Primary Intent / current goal
- `mcp_engram_promote_hot(concept)` before compression or session_end (hot path for re-hydration)

**Provenance & Compression**: Tiles carry `compresses_path` relations. Used in bundles at wake-up.

**Recognition during work**: Multi-phase plan → knowledge_graph at boundaries. Policy → formal_spec. Gaps/comparisons → tabular. Interleave `mcp_engram_update` + `thought_tile_write_result`.

See `engram-working-memory.md` (Item 2 section) and docs/RITUALS.md for full context. Use `mcp_engram_thought_tile_create` + `create_visualization`.

(Exposed so other agents can adopt the same structured offload discipline we dogfood.)