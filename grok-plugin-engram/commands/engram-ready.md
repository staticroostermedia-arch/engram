---
name: engram-ready
description: Probe backend readiness — recall mode, BVH, block count, memory mode
---

**Trigger:** Recall feels thin/sampled; wake seemed slow; deciding whether to escalate to deep tools.

1. Call `mcp_engram_get_backend_readiness`.
2. Report to user:
   - `memory_mode`, `profile`, `fully_initialized`
   - `recall_mode`, `bvh_ready`, `leg_block_count`
   - `defer_bvh`, `defer_watch_ingest`
3. If `recall_mode` is `sampled_bounded` and user needs quality recall → suggest `/engram-deep` (not auto `rebuild_bvh`).

Lightweight — safe to call anytime. Pairs with `/engram-recall` when anchors return empty.