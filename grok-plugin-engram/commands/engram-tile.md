---
name: engram-tile
description: Offload a meta arc to a thought tile — plans, specs, multi-phase design
---

**Trigger:** Multi-phase meta work, design doc arc, policy/roadmap spanning sessions, bundle too large for one trace.

1. Recall `helper:meta_work_escalation_v1` and `helper:current_meta_arc` via `/engram-recall`.
2. Call `mcp_engram_thought_tile_create` with:
   - `tile_type` — `research_offload` | `formal_spec` | `state_machine` | `tabular` | …
   - `title` — short human title
   - `payload` — structured JSON for the arc
   - `goal_context` — active goal
   - `spatial_references` — optional concept paths this tile compresses
3. Call `mcp_engram_quick_trace` with `decision` = tile created and tile id.
4. Call `mcp_engram_promote_hot` on the tile if handoff-critical.
5. At arc completion → `mcp_engram_thought_tile_write_result`.

Mandatory for design:/progress: arcs that outlive one session. See `docs/skills/engram-thought-tiles.md`.