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
3. No match → `mcp_engram_remember`
4. Verified fix → `mcp_engram_remember_solution`
5. Dead end / doom loop → `mcp_engram_scar`
6. Chain → `mcp_engram_relate` to goal/trace

Never `forget` + `remember` (annihilates p-tensor).

## Read escalation (when anchors fail)

| After `recall(scope=anchors)` empty or insufficient | Call |
|-----------------------------------------------------|------|
| Need arc direction | `query_with_momentum` |
| Need similarity | `query_pure` |
| Need graph | `search_by_relation` + `visualize` |
| Need full text | `read_concept` |

Full protocol: `docs/skills/engram-working-memory.md` · Map: `docs/TOOL_DECISION_MAP.md`