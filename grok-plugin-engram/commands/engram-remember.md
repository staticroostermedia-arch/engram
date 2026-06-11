---
name: engram-remember
description: Mint a new concept — only when recall shows no strong match
---

**Trigger:** Genuinely new fact/decision to persist; `/engram-update` does not apply.

1. Call `mcp_engram_recall` with `scope: anchors` on the intended concept keywords.
2. If match **≤0.85**, call `mcp_engram_remember` with:
   - `concept` — stable name (`design:`, `helper:`, `praxis:`, …)
   - `text` — full content
3. If match **>0.85** → use `/engram-update` instead (never duplicate).
4. Call `mcp_engram_relate` to active goal/trace.
5. Call `mcp_engram_quick_trace` if this was a decision fork.

For verified fixes use `/engram-solution`. For dead ends use `/engram-scar`.