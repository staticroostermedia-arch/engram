---
name: engram-deep
description: Escalate to deep memory mode — full manifold rituals (use sparingly)
---

Only when lean recall is insufficient (meta arcs, relation graphs, lawfulness audits):

1. Call `mcp_engram_set_memory_mode` with `mode: "deep"`.
2. Call `mcp_engram_get_backend_readiness` and report `memory_mode`, `bvh_ready`, `leg_block_count`.
3. Optionally call `mcp_engram_search_by_relation` or `mcp_engram_query_with_momentum` for the user's topic.
4. Warn the user: deep mode may trigger heavier recall and BVH work on large stores.

Do **not** call `watch_workspace` or `rebuild_bvh` automatically.

When deep work is done, run `/engram-lean` (or `set_memory_mode("lean")`) before handoff unless the user wants deep to persist.