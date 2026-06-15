# /engram-execute-tile

Mechanically replay a `verified_sequence` thought tile.

## Steps

1. `mcp_engram_read_concept(concept="tile:...")` — load payload.
2. For each step in `payload.steps` (in `order`):
   - Use `tool_hints` + `args_hints` as **suggestions** (auto-filled at condensation from trace `spatial_context`).
   - Resolve `spatial_context` to absolute path; construct MCP args JIT (do not blind-replay stale paths).
   - `mcp_engram_quick_trace(decision=..., why=..., prev=<last trace>, spatial_context=..., goal_context=...)`.
3. On full success: `mcp_engram_remember_solution` for the arc.

Prefer tiles with `tile_type: verified_sequence` and `version: verified_sequence_v0`.