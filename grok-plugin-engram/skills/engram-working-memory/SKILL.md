---
name: engram-working-memory
description: >
  Runtime discipline during work: context_for_edit before edits, anchor recall
  before derive, quick_trace at forks, update over forget+remember.
metadata:
  short-description: "Work loop — edit context, recall, trace"
---

# Engram Working Memory

**Trigger:** After wake, for every edit and decision during the session.

## Before editing a file (preferred safe composite)

```
mcp_engram_safe_edit_and_verify(
  path="/absolute/path/to/file",
  decision="What you plan to change",
  why="Justification",
  arc_delta="delta: narrative after edit (optional)",
  goal_context="goal:..."
)
```

Few-shot (1): `{"path":"/home/user/Engram/crates/engram-server/src/mcp.rs","decision":"Add safe_edit composite tool","why":"Agent tool fidelity goal — one-shot verified edit path","arc_delta":"delta: registered mcp_engram_safe_edit_and_verify handler","goal_context":"goal:agent_tool_fidelity_v1"}`

Few-shot (2): `{"path":"/home/user/Engram/docs/AGENT_MEMORY_CONTRACT.md","decision":"Refresh 8-tool examples","why":"Mirror hardened few-shots in docs","run_verify":true}`

Or lean pre-edit only: `mcp_engram_context_for_edit(path="...")` — `/engram-edit` or `/engram-safe-edit`

## Before heavy reasoning

```
mcp_engram_recall(query="<goal or trace keywords>", scope="anchors", k=5)
```

Or: `/engram-recall`

## At forks

```
mcp_engram_quick_trace(decision="...", why="...", goal_context="goal:...")
```

Or: `/engram-trace`

## Writes (Layer 1 — see TOOL_DECISION_MAP)

1. `mcp_engram_recall` — always first
2. Score >0.85 → `mcp_engram_update_with_tensor_bond` (preferred) or `mcp_engram_update` (only legal mutation of existing concepts)

Few-shot `update_with_tensor_bond` (1): `{"concept":"mcp__fn__dispatch__arc","new_text":"delta: wired safe_edit handler","recall_query":"mcp dispatch edit arc","bond_label":"edit_fidelity"}`

Few-shot `update_with_tensor_bond` (2): `{"concept":"design:agent_tool_fidelity_v1","new_text":"Phase 1: composite tools shipped","recall_query":"agent tool fidelity","scar_on_mismatch":true}`

Few-shot `remember` (1): `{"concept":"harness:agent_tool_fidelity_v1","text":"Deterministic suite for edit/update tool fidelity >=95%."}`

Few-shot `remember` (2): `{"concept":"user__prefers_absolute_paths","text":"Always pass absolute paths to context_for_edit and safe_edit_and_verify."}`

3. No match → `mcp_engram_remember`
4. Verified fix → `mcp_engram_remember_solution`
5. Dead end / doom loop → `mcp_engram_scar`
6. Chain → `mcp_engram_relate` to goal/trace

**Never** `forget` + `remember` on the same concept.

## Post-edit reflection (mandatory after substantive change)

Execute `reflection_suggested` from composite responses: quick_trace delta → verify_block_lawfulness (merkle) → tensor:edit_pattern_* upsert.

Rituals: `process:engram.ritual.safe-code-edit`, `process:engram.ritual.verified-memory-update`

Harness gate: `tools/test-harness/bin/engram-harness.sh --suite agent-tool-fidelity`