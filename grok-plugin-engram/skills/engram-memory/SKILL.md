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

You have **persistent geometric memory** via the Engram MCP server. Follow this contract — not all 66 tools (62 `mcp_engram_*` + 4 linguistic).

## Agent discipline (non-negotiable)

**Calling tools is the product.** Reading this skill without invoking MCP leaves no geometric record.

| Trigger | You MUST call |
|---------|---------------|
| Session / task start | `mcp_engram_session_start` or `/engram-wake` |
| Before any file edit | `mcp_engram_context_for_edit` or `/engram-edit` |
| Stuck / need goals | `mcp_engram_recall(scope=anchors)` or `/engram-recall` |
| Decision fork | `mcp_engram_quick_trace` or `/engram-trace` |
| End of block | `mcp_engram_session_end` or `/engram-session-end` |

Per-ritual skills (`engram-wake-up`, `engram-working-memory`, `engram-session-end`) expand each row. Use slash commands when you would otherwise skip the tool call.

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

### 5. Persist — write path (recall first)

```
mcp_engram_recall(query="...", scope="anchors")   # always first
# score >0.85 on existing concept → mcp_engram_update (NOT remember)
# no match → mcp_engram_remember
# verified fix → mcp_engram_remember_solution
# dead end → mcp_engram_scar
```

### 5b. Escalate read (when anchors fail)

| Situation | Tool |
|-----------|------|
| Preview too short | `read_concept` |
| Arc direction / trend | `query_with_momentum` |
| Geometric similarity | `query_pure` |
| Graph neighborhood | `search_by_relation` → `visualize` |
| Meta multi-phase arc | `thought_tile_create` |
| CRS / lawfulness doubt | `verify_manifold_integrity` |

**Full map:** `docs/TOOL_DECISION_MAP.md` (all 66 tools, mermaid diagrams)

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

- `docs/AGENT_MEMORY_CONTRACT.md` — 8-tool highway
- `docs/TOOL_DECISION_MAP.md` — **all 66 tools**, write path, Grok vs Cursor throttle
- `docs/GROK_BUILD_MEMORY.md`