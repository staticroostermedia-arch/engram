---
name: engram-edit
description: Pre-edit working memory — context_for_edit on a file before you change it
---

Run the Engram **edit-scoped working memory** ritual for the file the user is about to edit (or just edited):

1. Resolve an **absolute path** to the target file (ask if unclear).
2. **Preferred:** `mcp_engram_safe_edit_and_verify` with `path`, `decision`, `why`, optional `arc_delta` — or `mcp_engram_context_for_edit` with `path` and `auto_ingest: true`.

   Few-shot `context_for_edit` (1): `{"path":"/home/user/Engram/crates/engram-server/src/store.rs","auto_ingest":true}`

   Few-shot `context_for_edit` (2): `{"path":"/home/user/Engram/crates/engram-server/src/mcp.rs","line_start":6200,"line_end":6350}`

   Few-shot `safe_edit_and_verify` (1): `{"path":"/home/user/Engram/crates/engram-server/src/mcp.rs","decision":"Add safe_edit composite tool","why":"Agent tool fidelity goal — one-shot verified edit path","arc_delta":"delta: registered mcp_engram_safe_edit_and_verify handler","goal_context":"goal:agent_tool_fidelity_v1"}`

   Few-shot `safe_edit_and_verify` (2): `{"path":"/home/user/Engram/docs/AGENT_MEMORY_CONTRACT.md","decision":"Refresh 8-tool examples","why":"Mirror hardened few-shots in docs","run_verify":true}`

3. Call `mcp_engram_recall` with `scope: "anchors"` using keywords from the file path + task (goals, traces, rituals).
4. Call `mcp_engram_quick_trace` with `decision` (what you plan to change), `why`, and spatial context = the file path.
5. Read `harness_injection` in the response: if `last_session_touched` or `open_scars`, follow `suggested_actions`.
6. After substantive edits, use `post_edit_palette` from harness to `mcp_engram_update_with_tensor_bond` on `__arc` concepts, then `mcp_engram_quick_trace` (post-edit delta).
7. Optional: `mcp_engram_evolution_at_locus` for arc segments + trace chain at the locus.
8. Summarize for the user: related goals/traces, spatial hits, scars, and edit intent.

Do **not** call `watch_workspace` or `rebuild_bvh` unless the user explicitly requests deep spatial ingest.