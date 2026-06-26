---
name: engram-update
description: Evolve an existing memory — recall first, then update (never forget+remember)
---

Run the Engram **Layer 1 write path** when refining an existing concept (`design:`, `progress:`, `helper:`, `ritual:`, goals, or any prior memory):

1. Call `mcp_engram_recall` with `scope: "anchors"` (or the target `concept` name) to find the existing block.
2. **Preferred:** `mcp_engram_update_with_tensor_bond` (recall-first + tensor bond + lineage).

   Few-shot (1): `{"concept":"mcp__fn__dispatch__arc","new_text":"delta: wired safe_edit handler","recall_query":"mcp dispatch edit arc","bond_label":"edit_fidelity"}`

   Few-shot (2): `{"concept":"design:agent_tool_fidelity_v1","new_text":"Phase 1: composite tools shipped","recall_query":"agent tool fidelity","scar_on_mismatch":true}`

   Or if match score **>0.85**, `mcp_engram_update`:

   Few-shot (1): `{"concept":"store__fn__update__arc","new_text":"delta: added verify_edit_lineage helper"}`

   Few-shot (2): `{"concept":"design:agent_tool_fidelity_v1","new_text":"Shipped composite safe_edit_and_verify","provlog_mode":"append"}`

3. If **no strong match**, call `mcp_engram_remember` instead (new concept).

   Few-shot remember (1): `{"concept":"harness:agent_tool_fidelity_v1","text":"Deterministic suite for edit/update tool fidelity >=95%."}`

   Few-shot remember (2): `{"concept":"user__prefers_absolute_paths","text":"Always pass absolute paths to context_for_edit and safe_edit_and_verify."}`

4. Call `mcp_engram_quick_trace` with `decision` = what changed and `why` = rationale.
5. Optionally `mcp_engram_relate` the concept to active `goal:*` or latest `trace:*`.

**Never** `forget` + `remember` on the same concept — that annihilates p-tensor history.

Report: concept updated, CRS/drift if returned, trace id.

See `docs/TOOL_DECISION_MAP.md` § Write path.