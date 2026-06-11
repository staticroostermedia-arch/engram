---
name: engram-graph
description: Traverse the knowledge graph from a seed — search_by_relation + visualize
---

**Trigger:** "What's connected to X?", architecture map, sheaf navigation.

1. Get seed `concept` from recall, wake bundle, or user.
2. Call `mcp_engram_search_by_relation` with:
   - `concept` — seed
   - `direction` — `both` (default explore) or `from`/`to` when scoped
   - `label` — optional filter (`serves`, `requires`, `implements`, …)
   - `k` — start small (8–15); central goals may have 100+ edges
3. Call `mcp_engram_visualize` on the seed or top hit; show Mermaid to user.
4. Drill into interesting nodes with `/engram-read`.

To **create** an edge, use `/engram-relate` (this command is read/explore only).