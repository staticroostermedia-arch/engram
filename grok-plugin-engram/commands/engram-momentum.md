---
name: engram-momentum
description: Directional recall — what is trending or evolving in an arc (q+p blend)
---

Use when **anchors are insufficient** and the user asks what's *changing*, *trending*, or *evolving* — not just what statically matches a keyword.

1. Call `mcp_engram_recall` with `scope: "anchors"` first (lean default). If that answers the question, stop.
2. Call `mcp_engram_query_with_momentum` with:
   - `query` — user's topic or arc keywords
   - `k` — default 5 (max 20)
   - `zedos_filter` — optional (`training` for NREM-biased blocks)
3. Present results as: **direction of change**, top concepts with momentum signal, how they relate to the active goal.
4. For full text on a hit, follow with `mcp_engram_read_concept`.
5. For graph neighborhood, offer `/engram-relate` exploration via `mcp_engram_search_by_relation` + `mcp_engram_visualize`.

Do **not** use at wake — use after `recall(scope=anchors)` fails. Heavier than anchor recall.

See `docs/TOOL_DECISION_MAP.md` § Read escalation.