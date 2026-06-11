---
name: engram-pure
description: Geometric similarity search — concepts like this intent (q tensor K-NN)
---

**Trigger:** Keyword recall misses; you need "things like X" geometrically, not text match.

1. Try `/engram-recall` with `scope: anchors` first — stop if sufficient.
2. Call `mcp_engram_query_pure` with:
   - `intent` — natural language description of what you're looking for
   - `k` — default 6 (max 20)
3. For full text on a hit → `/engram-read`.
4. For graph context → `/engram-graph` from top result.

Faster than momentum for **static similarity**. Use `/engram-momentum` when you care about *change*, not similarity.