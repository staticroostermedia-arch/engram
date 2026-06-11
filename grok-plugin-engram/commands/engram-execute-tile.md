# /engram-execute-tile

Mechanically replay a `verified_sequence` thought tile.

## Steps

1. `mcp_engram_read_concept(concept="tile:...")` — load payload.
2. For each step in `payload.steps` (in `order`):
   - Execute `tool_hints` if present.
   - `mcp_engram_quick_trace(decision=..., why=..., prev=<last trace>, goal_context=...)`.
3. On full success: `mcp_engram_remember_solution` for the arc.

Prefer tiles with `tile_type: verified_sequence` and `version: verified_sequence_v0`.