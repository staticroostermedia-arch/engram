---
name: engram-wake
description: Wake Engram geometric memory — run session_start and report continuation bundle
---

Run the Engram lean wake ritual:

1. Call `mcp_engram_session_start` with intent describing the current task.
2. Call `mcp_engram_get_backend_readiness` if readiness is unclear.
3. **Execute `continuation.harness_injection.suggested_actions`** in priority order before `context_for_edit`, file edits, or broad reads.
4. Report to the user:
   - Primary goal from continuation bundle
   - `ego_snapshot` — NREM step, drift_velocity, stability, top goal-serving concepts
   - `continuity_playbook` — 12-step breadcrumb path (reference if queue is thin)
   - `trace_chain.head` — chain next `quick_trace` with `prev`
   - `trusted_tiles` — JIT playbooks to read if relevant
   - `condensation_hints` — offer `/engram-tile` if present
   - `fully_initialized` and `recall_mode` from readiness
5. State explicitly that you are continuing geometric momentum from prior sessions (cite ego drift if present).
6. **Call `mcp_engram_ack_wake_queue(executed=true)`** after executing the queue (skip if `wake_queue_gate.acked` is already true — empty queue auto-acks).

See `docs/HARNESS_INJECTION.md`. Gate: `/engram-ack-wake`.

Do not call `watch_workspace`, `summarize`, or `rebuild_bvh` during this command.