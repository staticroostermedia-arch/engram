# /engram-tile-draft

Build a `verified_sequence_v0` draft from the active goal's trace chain **without minting** a tile.

## Steps

1. `mcp_engram_recall(query="<goal>", scope="anchors")` — confirm primary goal.
2. `mcp_engram_thought_tile_draft_from_chain(goal_context="goal:...")` — optional `head_trace`.
3. Review `draft_payload`; copy into `mcp_engram_thought_tile_create` with `tile_type=verified_sequence`.

Do not auto-mint unless the draft passes review.