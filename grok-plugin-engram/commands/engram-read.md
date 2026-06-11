---
name: engram-read
description: Full untruncated body of one concept — after recall preview is insufficient
---

**Trigger:** `recall` or wake bundle preview is truncated; you need the complete text.

1. Get exact `concept` name from prior `recall`, `session_start` bundle, or user.
2. Call `mcp_engram_read_concept` with `concept`.
3. Summarize key fields for the user; cite the concept name.

Do **not** use for discovery — use `/engram-recall` or `/engram-momentum` first, then `/engram-read` on the hit.