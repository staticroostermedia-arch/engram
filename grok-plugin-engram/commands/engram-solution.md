---
name: engram-solution
description: Crystallize a verified error→fix pair as praxis (CRS 1.0, never decays)
---

**Trigger:** Bug fixed and verified (tests pass, user confirms, CI green).

1. Call `mcp_engram_remember_solution` with:
   - `error_pattern` — the failure signature (error text, symptom, concept)
   - `solution` — what actually worked (steps, file, approach)

   Few-shot (1): `{"error_pattern":"cargo test mcp mutex poison","solution":"Use mcp_test_guard() serializing MCP tests"}`

   Few-shot (2): `{"error_pattern":"repeated context_for_edit blocked","solution":"mcp_engram_update on __arc or mcp_engram_ack_edit_arc before re-read","process_context":"process:engram.ritual.working-memory"}`
2. Call `mcp_engram_relate` linking the praxis block to active `goal:*` with label `serves`.
3. Call `mcp_engram_quick_trace` noting the verified fix.
4. Report praxis concept name to user.

Use **after** verification — not for hypotheses. For evolving docs/helpers use `/engram-update` instead.