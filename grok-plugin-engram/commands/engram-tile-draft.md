# /engram-tile-draft

Build a `verified_sequence_v0` draft from the active goal's trace chain **without minting** a tile.

## Steps

1. `mcp_engram_recall(query="<goal>", scope="anchors")` — confirm primary goal.
2. `mcp_engram_thought_tile_draft_from_chain(goal_context="goal:...")` — optional `head_trace`.
3. Review `draft_payload.steps[]` — each step now includes:
   - `tool_hints` — inferred from trace `spatial_context` + decision text (JIT palette)
   - `args_hints` — per-tool arg templates (`file_stem`, line window, goal_context)
   - `spatial_context` / `goal_context` — copied from source traces
4. Copy into `mcp_engram_thought_tile_create` with `tile_type=verified_sequence`.

Do not auto-mint unless the draft passes review. At wake, minted tiles surface in `verified_processes` for JIT replay.