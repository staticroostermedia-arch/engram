---
name: engram-wake
description: Wake Engram geometric memory — run session_start and report continuation bundle
---

Run the Engram lean wake ritual:

1. Call `mcp_engram_session_start` with intent describing the current task.
2. Call `mcp_engram_get_backend_readiness` if readiness is unclear.
3. Report to the user:
   - Primary goal from continuation bundle
   - Last session handoff preview (if any)
   - `fully_initialized` and `recall_mode` from readiness
4. State explicitly that you are continuing geometric momentum from prior sessions.

Do not call `watch_workspace`, `summarize`, or `rebuild_bvh` during this command.