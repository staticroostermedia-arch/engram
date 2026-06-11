---
name: engram-wake
description: Wake Engram geometric memory — run session_start and report continuation bundle
---

Run the Engram lean wake ritual:

1. Call `mcp_engram_session_start` with intent describing the current task.
2. Call `mcp_engram_get_backend_readiness` if readiness is unclear.
3. **Execute `continuation.harness_injection.suggested_actions`** in priority order before broad reads.
4. Report to the user:
   - Primary goal from continuation bundle
   - `harness_injection.trace_chain.head` — chain next `quick_trace` with `prev`
   - `trusted_tiles` — JIT playbooks to read if relevant
   - `condensation_hints` — offer `/engram-tile` if present
   - `fully_initialized` and `recall_mode` from readiness
5. State explicitly that you are continuing geometric momentum from prior sessions.

See `docs/HARNESS_INJECTION.md`.

Do not call `watch_workspace`, `summarize`, or `rebuild_bvh` during this command.