---
name: engram-ingest
description: Spatial recovery — ingest file(s) when context_for_edit returns empty spatial hits
---

**Trigger:** `context_for_edit` has no spatial items; new files not in manifold; spatial_status shows gaps.

**Prefer** `/engram-edit` with `auto_ingest: true` first — only escalate here when that failed or user names explicit paths.

1. For one file: `mcp_engram_force_spatial_ingest` with the path (recovery, not routine).
2. For bounded batch: `mcp_engram_incremental_spatial_ingest` with `max_files` small (5–10).
3. Re-run `/engram-edit` on the target file.
4. Call `mcp_engram_spatial_status` — report item count.

Do **not** call `watch_workspace` or full-tree ingest in lean mode. Never at wake.