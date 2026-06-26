---
name: engram-safe-edit
description: Verified composite edit — safe_edit_and_verify (context + trace + arc + lineage + tensor pattern)
---

Run the **safe code edit** ritual (`process:engram.ritual.safe-code-edit`):

1. Resolve an **absolute path** to the target file.
2. Call `mcp_engram_ack_wake_queue` if not already done this session.
3. Call `mcp_engram_safe_edit_and_verify` with `path`, `decision`, `why`, optional `arc_delta`, `goal_context`, `run_verify: true`.

**Few-shot examples (verbatim — copy exact JSON):**

(1) `{"path":"/home/user/Engram/crates/engram-server/src/mcp.rs","decision":"Add safe_edit composite tool","why":"Agent tool fidelity goal — one-shot verified edit path","arc_delta":"delta: registered mcp_engram_safe_edit_and_verify handler","goal_context":"goal:agent_tool_fidelity_v1"}`

(2) `{"path":"/home/user/Engram/docs/AGENT_MEMORY_CONTRACT.md","decision":"Refresh 8-tool examples","why":"Mirror hardened few-shots in docs","run_verify":true}`

4. Execute `reflection_suggested` from the response (quick_trace delta + verify_block_lawfulness).
5. For external editor changes, follow with `mcp_engram_update_with_tensor_bond` on `__arc`.
6. Clear edit-arc debt: `mcp_engram_ack_edit_arc` with `lineage_check: true` when appropriate.

Prefer this over raw `context_for_edit` alone for substantive edits.