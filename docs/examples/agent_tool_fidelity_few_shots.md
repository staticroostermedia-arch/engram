# Agent tool fidelity — canonical few-shot JSON (verbatim in MCP + plugin + skills)

## mcp_engram_remember
1. `{"concept":"harness:agent_tool_fidelity_v1","text":"Deterministic suite for edit/update tool fidelity >=95%."}`
2. `{"concept":"user__prefers_absolute_paths","text":"Always pass absolute paths to context_for_edit and safe_edit_and_verify."}`

## mcp_engram_safe_edit_and_verify
1. `{"path":"/home/user/Engram/crates/engram-server/src/mcp.rs","decision":"Add safe_edit composite tool","why":"Agent tool fidelity goal — one-shot verified edit path","arc_delta":"delta: registered mcp_engram_safe_edit_and_verify handler","goal_context":"goal:agent_tool_fidelity_v1"}`
2. `{"path":"/home/user/Engram/docs/AGENT_MEMORY_CONTRACT.md","decision":"Refresh 8-tool examples","why":"Mirror hardened few-shots in docs","run_verify":true}`

## mcp_engram_update_with_tensor_bond
1. `{"concept":"mcp__fn__dispatch__arc","new_text":"delta: wired safe_edit handler","recall_query":"mcp dispatch edit arc","bond_label":"edit_fidelity"}`
2. `{"concept":"design:agent_tool_fidelity_v1","new_text":"Phase 1: composite tools shipped","recall_query":"agent tool fidelity","scar_on_mismatch":true}`

## mcp_engram_context_for_edit
1. `{"path":"/home/user/Engram/crates/engram-server/src/store.rs","auto_ingest":true}`
2. `{"path":"/home/user/Engram/crates/engram-server/src/mcp.rs","line_start":6200,"line_end":6350}`

## mcp_engram_update
1. `{"concept":"store__fn__update__arc","new_text":"delta: added verify_edit_lineage helper"}`
2. `{"concept":"design:agent_tool_fidelity_v1","new_text":"Shipped composite safe_edit_and_verify","provlog_mode":"append"}`

## mcp_engram_ack_edit_arc
1. `{"concepts":["store__fn__context_for_edit"],"skip":false,"note":"updated __arc via mcp_engram_update"}`
2. `{"skip":true,"note":"read-only context_for_edit — no substantive edits"}`
3. `{"concepts":["store__fn__context_for_edit"],"skip":false,"note":"updated __arc via mcp_engram_update","lineage_check":true,"trace_id":"trace:1780000000_post_edit"}`

## mcp_engram_quick_trace
1. `{"decision":"Implement edit_fidelity module","why":"Composite tools need testable helpers","spatial_context":"crates/engram-server/src/edit_fidelity.rs:1","goal_context":"goal:agent_tool_fidelity_v1"}`
2. `{"decision":"Hardened MCP descriptions with few-shots","why":"Agents need copy-pasteable JSON","prev":"trace:1780000000_prior-step"}`

## mcp_engram_remember_solution
1. `{"error_pattern":"cargo test mcp mutex poison","solution":"Use mcp_test_guard() serializing MCP tests"}`
2. `{"error_pattern":"repeated context_for_edit blocked","solution":"mcp_engram_update on __arc or mcp_engram_ack_edit_arc before re-read","process_context":"process:engram.ritual.working-memory"}`