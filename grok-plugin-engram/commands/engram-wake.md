---
name: engram-wake
description: Wake Engram geometric memory — run session_start and report continuation bundle
---

Run the Engram lean wake ritual (post-restart safe):

1. Call `mcp_engram_session_start` with intent describing the current task (after TUI/MCP restart use `intent="post-restart verify"`).
2. **Confirm** `continuation.injection_completeness` and `continuation.nvme_context` are present (stale binary if null — restart TUI).
3. Call `mcp_engram_get_backend_readiness` if `nvme_context.recall_mode` is `sampled_bounded` on a large store; poll ~30s for `full_bvh_gpu` (agent profile eager-builds in background). Call `mcp_engram_rebuild_bvh` **at most once** if still bounded after 30s — if response is `already_building`, only poll (never spam rebuild).
4. **Execute `continuation.suggested_actions`** in order (each carries `injection_rank`) before `context_for_edit`, file edits, or broad reads.
5. Report to the user:
   - Primary goal from continuation bundle
   - `injection_completeness` score + `missing` slots
   - `nvme_context` — recall_mode, bvh_ready, gpu_hot_resident, leg_block_count
   - `ego_snapshot` — NREM step, drift_velocity, stability
   - `trace_chain_head` — chain next `quick_trace` with `prev`
   - `presentation_stratum` previews (slim wake)
   - `fully_initialized` and `recall_mode` from readiness
6. State explicitly that you are continuing geometric momentum from prior sessions.
7. **Call `mcp_engram_ack_wake_queue(executed=true)`** after executing the queue (skip if already acked).

If `injection_completeness.score < 0.85` or `missing` contains `nvme_recall_path`, call `mcp_engram_get_continuation_bundle` before broad reads.

See `docs/HARNESS_INJECTION.md` and `docs/AGENT_MEMORY_CONTRACT.md#manage-resume-tui--mcp-restart`. Gate: `/engram-ack-wake`.

Do not call `watch_workspace` or `summarize` during this command. `rebuild_bvh` at most once per wake when sampled_bounded persists after ~30s poll; respect `already_building` / `bvh_build_in_progress`.