---
name: engram-session-end
description: End session with structured handoff — session_end for next wake continuity
---

Run the Engram **session-end** ritual before the user leaves or context resets:

1. Call `mcp_engram_session_end` with:
   - `summary` — structured markdown: **Decisions** (trace ids), **Files changed**, **Open blockers**, **Next steps**
   - `prepare_compression: true`
2. Report from the response: `handoff_packet.primary_goal`, `key_traces`, `files_touched`, `wake_protocol`.
3. Tell the user: next session should run `/engram-wake` — `session_start` will inline this handoff.

Do not skip this when meaningful work happened. Flat chat summaries do not replace the geometric handoff packet.

**If the session goal is complete:** clear first (`/engram-goal` → `goal_update_status` + `demote_from_context`, or TUI `update_goal(completed=true)`), then `session_end`. Terminal step: push branch + PR notes (fixes, ACs, traces).