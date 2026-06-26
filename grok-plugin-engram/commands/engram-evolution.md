---
name: engram-evolution
description: Bounded evolution recon at a file locus — arc segments and trace chain without full read_concept
---

Run **code atlas evolution recon** for the file the user is inspecting or just edited:

1. Resolve an **absolute path** (and optional `line_start` / `line_end` window).
2. Call `mcp_engram_evolution_at_locus` with `path`, line window, and `auto_ingest: true` if spatial loci may be empty.
3. Read `loci`, `arcs` (segments with `--- update @ ---` markers), `trace_chain`, and `scars_at_locus`.
4. If `edit_arc_debt` is high, remind the user to `update` on `__arc` or `/engram-ack-edit-arc` for read-only passes.
5. Summarize evolution narrative for the user — decisions, ruled-out paths, arc deltas.

Use after substantive edits or when continuing work on a locus from a prior session. See [docs/CODE_ATLAS_CONTINUITY.md](../../docs/CODE_ATLAS_CONTINUITY.md).

**Tensor self-evolution**: When proposing substrate/memory improvements (not just code), use `mcp_engram_thought_tile_create` with `tile_type: "propose_improvement"`:

```json
{
  "tile_type": "propose_improvement",
  "title": "bond-consolidation-ritual",
  "payload": {
    "suggestion": "Add explicit consolidation invoke after tile write_result when drift high",
    "target_concept": "design:tensor_thought_unification_v1"
  },
  "goal_context": "goal:engram_mvp_v1"
}
```

This mints a trace, tile + `tensor:tile__` mirror, and routes the suggestion through `mcp_engram_update_with_tensor_bond` on the target (CRS gate, lineage preserved). Ritual: `process:engram.ritual.verified-update-with-consolidation`.