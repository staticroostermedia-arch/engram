---
name: engram-edit
description: Pre-edit working memory — context_for_edit on a file before you change it
---

Run the Engram **edit-scoped working memory** ritual for the file the user is about to edit (or just edited):

1. Resolve an **absolute path** to the target file (ask if unclear).
2. Call `mcp_engram_context_for_edit` with `path`, and `auto_ingest: true` if the file may be new to spatial memory.
3. Call `mcp_engram_recall` with `scope: "anchors"` using keywords from the file path + task (goals, traces, rituals).
4. Call `mcp_engram_quick_trace` with `decision` (what you plan to change), `why`, and spatial context = the file path.
5. Summarize for the user: related goals/traces, spatial hits, and your edit intent.

Do **not** call `watch_workspace` or `rebuild_bvh` unless the user explicitly requests deep spatial ingest.