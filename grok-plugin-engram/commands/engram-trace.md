---
name: engram-trace
description: Record a decision fork — quick_trace at a significant choice point
---

Capture a geometric reasoning fork (user asked to remember a decision, or you hit a real alternative):

1. Call `mcp_engram_quick_trace` with:
   - `decision` — the choice made (one sentence)
   - `why` — justification + what was ruled out
   - `goal_context` — active goal if known (e.g. `goal:mvp_gap_closure_v1`)
   - `context` — file path or subsystem if applicable
2. Report the returned `trace:*` id to the user.
3. If the approach was ruled out permanently, also call `mcp_engram_scar` on the dead-end concept.

For high-stakes architecture forks, offer `mcp_engram_record_reasoning_trace` (full A/D/R) instead.