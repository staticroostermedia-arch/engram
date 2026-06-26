//! Canonical few-shot JSON examples — MUST match grok-plugin-engram/commands/*.md and docs/skills verbatim.

pub const REMEMBER_EX1: &str = r#"{"concept":"harness:agent_tool_fidelity_v1","text":"Deterministic suite for edit/update tool fidelity >=95%."}"#;
pub const REMEMBER_EX2: &str = r#"{"concept":"user__prefers_absolute_paths","text":"Always pass absolute paths to context_for_edit and safe_edit_and_verify."}"#;

pub const ACK_EDIT_ARC_EX1: &str = r#"{"concepts":["store__fn__context_for_edit"],"skip":false,"note":"updated __arc via mcp_engram_update"}"#;
pub const ACK_EDIT_ARC_EX2: &str =
    r#"{"skip":true,"note":"read-only context_for_edit — no substantive edits"}"#;
pub const ACK_EDIT_ARC_EX3: &str = r#"{"concepts":["store__fn__context_for_edit"],"skip":false,"note":"updated __arc via mcp_engram_update","lineage_check":true,"trace_id":"trace:1780000000_post_edit"}"#;

pub const SAFE_EDIT_EX1: &str = r#"{"path":"/home/user/Engram/crates/engram-server/src/mcp.rs","decision":"Add safe_edit composite tool","why":"Agent tool fidelity goal — one-shot verified edit path","arc_delta":"delta: registered mcp_engram_safe_edit_and_verify handler","goal_context":"goal:agent_tool_fidelity_v1"}"#;
pub const SAFE_EDIT_EX2: &str = r#"{"path":"/home/user/Engram/docs/AGENT_MEMORY_CONTRACT.md","decision":"Refresh 8-tool examples","why":"Mirror hardened few-shots in docs","run_verify":true}"#;

pub const UPDATE_BOND_EX1: &str = r#"{"concept":"mcp__fn__dispatch__arc","new_text":"delta: wired safe_edit handler","recall_query":"mcp dispatch edit arc","bond_label":"edit_fidelity"}"#;
pub const UPDATE_BOND_EX2: &str = r#"{"concept":"design:agent_tool_fidelity_v1","new_text":"Phase 1: composite tools shipped","recall_query":"agent tool fidelity","scar_on_mismatch":true}"#;

pub const CONTEXT_FOR_EDIT_EX1: &str =
    r#"{"path":"/home/user/Engram/crates/engram-server/src/store.rs","auto_ingest":true}"#;
pub const CONTEXT_FOR_EDIT_EX2: &str = r#"{"path":"/home/user/Engram/crates/engram-server/src/mcp.rs","line_start":6200,"line_end":6350}"#;

pub const UPDATE_EX1: &str =
    r#"{"concept":"store__fn__update__arc","new_text":"delta: added verify_edit_lineage helper"}"#;
pub const UPDATE_EX2: &str = r#"{"concept":"design:agent_tool_fidelity_v1","new_text":"Shipped composite safe_edit_and_verify","provlog_mode":"append"}"#;

pub const REMEMBER_SOLUTION_EX1: &str = r#"{"error_pattern":"cargo test mcp mutex poison","solution":"Use mcp_test_guard() serializing MCP tests"}"#;
pub const REMEMBER_SOLUTION_EX2: &str = r#"{"error_pattern":"repeated context_for_edit blocked","solution":"mcp_engram_update on __arc or mcp_engram_ack_edit_arc before re-read","process_context":"process:engram.ritual.working-memory"}"#;

pub const QUICK_TRACE_EX1: &str = r#"{"decision":"Implement edit_fidelity module","why":"Composite tools need testable helpers","spatial_context":"crates/engram-server/src/edit_fidelity.rs:1","goal_context":"goal:agent_tool_fidelity_v1"}"#;
pub const QUICK_TRACE_EX2: &str = r#"{"decision":"Hardened MCP descriptions with few-shots","why":"Agents need copy-pasteable JSON","prev":"trace:1780000000_prior-step"}"#;

pub fn remember_description() -> String {
    format!(
        "Encode NEW facts only — persistent HolographicBlock (.leg3). Recall first; if match>0.85 use mcp_engram_update instead. CRS tiers: 1.0=pinned | >=0.74=grounded | <0.50=verify first. FEW-SHOT EXAMPLES: (1) New harness concept: {REMEMBER_EX1} (2) User preference: {REMEMBER_EX2}"
    )
}

pub fn ack_edit_arc_description() -> String {
    format!(
        "Acknowledge or skip pending edit-arc debt — unblocks repeat context_for_edit on the same path when ENGRAM_EDIT_ARC_GATE=hard. Prefer mcp_engram_update on *__arc after edits; use skip=true with an honest note only for read-only passes. FEW-SHOT EXAMPLES: (1) Post-edit arc update done elsewhere: {ACK_EDIT_ARC_EX1} (2) Read-only recon: {ACK_EDIT_ARC_EX2} (3) Post-edit with lineage verification: {ACK_EDIT_ARC_EX3}"
    )
}

pub fn safe_edit_description() -> String {
    format!(
        "SAFE composite for code edits: context_for_edit + quick_trace + optional __arc update + verify_manifold + lineage check + tensor edit_pattern bond. Prefer this over ad-hoc context_for_edit when changing crates/, docs/, or processes/. Returns trace_id, arc_concept, crs_delta, reflection_suggested. FEW-SHOT EXAMPLES: (1) Pre+post edit with arc delta: {SAFE_EDIT_EX1} (2) Intent-only trace before external editor edit: {SAFE_EDIT_EX2}"
    )
}

pub fn update_bond_description() -> String {
    format!(
        "SAFE composite for memory updates: recall-first + mcp_engram_update + tensor bond (edit_fidelity) + optional scar on mismatch. NEVER use forget+remember to mutate. Returns crs_delta, tensor_pattern, lineage. FEW-SHOT EXAMPLES: (1) Append arc delta after edit: {UPDATE_BOND_EX1} (2) Update design block with recall guard: {UPDATE_BOND_EX2}"
    )
}

pub fn context_for_edit_description() -> String {
    format!(
        "Code atlas v2 — pre-edit situated memory. Returns JSON: spatial_items (tree-sitter AABB + edit_arc per locus), traces_at_locus, scars_at_locus, harness_injection.post_edit_palette. Requires wake queue ack when ENGRAM_WAKE_QUEUE_GATE=hard. Prefer mcp_engram_safe_edit_and_verify for substantive edits. FEW-SHOT EXAMPLES: (1) Standard pre-edit: {CONTEXT_FOR_EDIT_EX1} (2) Line-bounded locus: {CONTEXT_FOR_EDIT_EX2}"
    )
}

pub fn update_description() -> String {
    format!(
        "CRITICAL: Use whenever you change an existing memory. NEVER forget+remember — destroys history. Superposes q + p-momentum + ProvLog splice. Prefer mcp_engram_update_with_tensor_bond for agent edits (recall-first + lineage bond). FEW-SHOT EXAMPLES: (1) Post-edit arc delta: {UPDATE_EX1} (2) Design evolution: {UPDATE_EX2}"
    )
}

pub fn remember_solution_description() -> String {
    format!(
        "Crystallized error→solution pair (ZEDOS_PRAXIS, CRS=1.0). Use after verified fixes, not for routine deltas (use update). FEW-SHOT EXAMPLES: (1) Build fix: {REMEMBER_SOLUTION_EX1} (2) Ritual fix: {REMEMBER_SOLUTION_EX2}"
    )
}

pub fn quick_trace_description() -> String {
    format!(
        "Low-friction trace capture → structured trace:* block with prev_in_trace chain. Use at every fork; chain prev from trace_chain.head. Post-edit: run reflection loop or mcp_engram_safe_edit_and_verify. FEW-SHOT EXAMPLES: (1) Edit fork: {QUICK_TRACE_EX1} (2) Post-edit delta: {QUICK_TRACE_EX2}"
    )
}
