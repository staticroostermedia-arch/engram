---
name: engram-update
description: Evolve an existing memory — recall first, then update (never forget+remember)
---

Run the Engram **Layer 1 write path** when refining an existing concept (`design:`, `progress:`, `helper:`, `ritual:`, goals, or any prior memory):

1. Call `mcp_engram_recall` with `scope: "anchors"` (or the target `concept` name) to find the existing block.
2. If match score **>0.85** (or concept name is known), call `mcp_engram_update` with:
   - `concept` — exact existing concept name
   - `new_text` — full revised content (append/refine, not a duplicate mint)
3. If **no strong match**, call `mcp_engram_remember` instead (new concept).
4. Call `mcp_engram_quick_trace` with `decision` = what changed and `why` = rationale.
5. Optionally `mcp_engram_relate` the concept to active `goal:*` or latest `trace:*`.

**Never** `forget` + `remember` on the same concept — that annihilates p-tensor history.

Report: concept updated, CRS/drift if returned, trace id.

See `docs/TOOL_DECISION_MAP.md` § Write path.