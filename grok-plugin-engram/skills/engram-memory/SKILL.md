---
name: engram-memory
description: >
  Engram geometric memory — 8-tool lean contract for Grok Build. Use at session
  start (wake), before file edits, when recalling goals/traces, and at session end
  (handoff). Do not call watch_workspace or rebuild_bvh unless user explicitly asks.
metadata:
  short-description: "Geometric memory — wake, edit context, handoff"
---

# Engram Memory — 8-Tool Contract

You have **persistent geometric memory** via the Engram MCP server. Follow this contract — not all 62 tools.

## Slash commands (Grok Build plugin)

| Command | Ritual |
|---------|--------|
| `/engram-wake` | `session_start` + continuation report |
| `/engram-edit` | `context_for_edit` + anchor recall + pre-edit trace |
| `/engram-recall` | `recall(scope=anchors)` when stuck |
| `/engram-trace` | `quick_trace` at a fork |
| `/engram-session-end` | `session_end` structured packet |
| `/engram-deep` | `set_memory_mode(deep)` — sparingly |

## Every session

### 1. Wake (mandatory first call)

```
mcp_engram_session_start(intent="<your objective for this session>")
```

Returns inline: `continuation_bundle`, `backend_readiness`, `session_key`.

Read `continuation_bundle.primary_goal` and state: *"Continuing from …"*

### 2. Before editing a file

```
mcp_engram_context_for_edit(path="/absolute/path/to/file")
```

### 3. When stuck — anchors first

```
mcp_engram_recall(query="<goal or trace keywords>", scope="anchors", k=5)
```

### 4. At forks

```
mcp_engram_quick_trace(decision="...", why="...", goal_context="goal:...")
```

### 5. New facts only (recall first; if match >0.85 use update)

```
mcp_engram_remember(concept="...", text="...")
```

### 6. End (mandatory)

```
mcp_engram_session_end(summary="<decisions, files, open questions>", prepare_compression=true)
```

## Probe / mode

- `mcp_engram_get_backend_readiness()` — after wake if recall seems bounded
- `mcp_engram_set_memory_mode(mode="deep")` — only for full-manifold exploration

## Do NOT at wake (lean default)

| Avoid | Why |
|-------|-----|
| `watch_workspace` | RAM spike on large repos |
| `rebuild_bvh` | Minutes + GB RAM |
| `summarize` | Redundant — inline in session_start |
| `list_concepts` | Full store scan |

## Full docs (repo)

- `docs/AGENT_MEMORY_CONTRACT.md`
- `docs/GROK_BUILD_MEMORY.md`