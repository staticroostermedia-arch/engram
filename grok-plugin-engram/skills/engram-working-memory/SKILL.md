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

## Before editing a file

```
mcp_engram_context_for_edit(path="/absolute/path/to/file")
```

Or: `/engram-edit`

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

## Writes

- Recall first. Match >0.85 → `mcp_engram_update`. No match → `mcp_engram_remember`.
- Dead ends → `mcp_engram_scar`. Never `forget` + `remember`.

Full protocol: `docs/skills/engram-working-memory.md`