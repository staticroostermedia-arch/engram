---
name: engram-goal
description: Goal stack — set primary, list status, orient intentional self-model
---

**Trigger:** New task arc, shift focus, check what goal traces should link to.

1. If user names a goal → `mcp_engram_goal_set_primary` with `goal:...` id.
2. Else call `mcp_engram_goal_list` or `mcp_engram_goal_status` for orientation.
3. Call `mcp_engram_recall` with `scope: anchors` and query `goal:`.
4. Report primary goal; suggest linking next `/engram-trace` with `goal_context`.

For decomposing work → `mcp_engram_goal_decompose`. For search → `mcp_engram_goal_search`.

All traces and tiles should reference active primary goal when known.