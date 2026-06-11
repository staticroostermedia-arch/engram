---
name: engram-recall
description: Anchor-first recall — goals, traces, rituals before episodic noise
---

When the user is stuck or asks "what did we decide / what's the goal":

1. Call `mcp_engram_recall` with `scope: "anchors"` and a query from the user's question (default `k: 5`).
2. If anchors are thin, call `mcp_engram_get_backend_readiness` once — report `memory_mode`, `recall_mode`, `fully_initialized`.
3. Present results as: primary goal, active traces, relevant rituals/helpers — not a raw dump.
4. If a strong match exists (>0.85 similarity), prefer `mcp_engram_update` over minting duplicate concepts.

Lean default: anchors first. Only escalate to `query_with_momentum` or `search_by_relation` if the user asks for deep exploration.