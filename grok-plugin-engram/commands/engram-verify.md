---
name: engram-verify
description: Lawfulness health check — CRS sample, manifold integrity after substantive changes
---

**Trigger:** After substrate edits, before marketplace push, cold boot doubt, post large remember/update batch.

1. Call `mcp_engram_verify_manifold_integrity` with `min_crs: 0.74`, `sample_size: 100` (defaults OK).
2. Call `mcp_engram_spatial_status` if spatial/code edits were involved.
3. Report pass/fail summary — do not dump raw JSON unless user asks.
4. On failure → `/engram-scar` the approach + trace; do not proceed silently.

Optional deep audit: `mcp_engram_verify_block_lawfulness` on specific concepts, `mcp_engram_genesis` for cold-store check.