# Engram Slash Commands — Agent Primary User Guide

**You (the AI) are the primary user.** Each command maps to one **decision moment** — not one MCP tool. Run the full ritual inside each command file.

**Map of all 66 tools:** [docs/TOOL_DECISION_MAP.md](../../docs/TOOL_DECISION_MAP.md)  
**Harness injection (auto context):** [docs/HARNESS_INJECTION.md](../../docs/HARNESS_INJECTION.md)

At wake: execute `continuation.harness_injection.suggested_actions` before grep/read.  
At edit: read `harness_injection` on `context_for_edit` response (scars, last-session-touched).

---

## Every session (non-negotiable)

| Moment | Command | MCP core |
|--------|---------|----------|
| Chat / task start | `/engram-wake` | `session_start` |
| End of work block | `/engram-session-end` | `session_end` |

---

## While working (default loop)

| Moment | Command | MCP core |
|--------|---------|----------|
| Before editing a file | `/engram-edit` | `context_for_edit` + recall + trace |
| Stuck — goals, decisions | `/engram-recall` | `recall(scope=anchors)` |
| Preview too short | `/engram-read` | `read_concept` |
| Recall feels weak | `/engram-ready` | `get_backend_readiness` |
| Significant fork | `/engram-trace` | `quick_trace` (+ `scar` if dead end) |
| Condense trace chain | `/engram-tile-draft` | `thought_tile_draft_from_chain` |
| Replay verified playbook | `/engram-execute-tile` | `read_concept` → step loop → `quick_trace` |

---

## Write path (pick ONE)

| Moment | Command | MCP core |
|--------|---------|----------|
| Refine existing concept (>0.85 match) | `/engram-update` | `recall` → `update` |
| New concept (no match) | `/engram-remember` | `recall` → `remember` |
| Verified fix (tests green) | `/engram-solution` | `remember_solution` |
| Dead end / doom loop | `/engram-scar` | `scar` + trace |
| Link two concepts | `/engram-relate` | `relate` |

---

## Read escalation (after anchors fail)

| Moment | Command | MCP core |
|--------|---------|----------|
| What's trending / evolving | `/engram-momentum` | `query_with_momentum` |
| Geometrically similar | `/engram-pure` | `query_pure` |
| Graph neighborhood | `/engram-graph` | `search_by_relation` + `visualize` |

---

## Meta & mode

| Moment | Command | MCP core |
|--------|---------|----------|
| Multi-phase design arc | `/engram-tile` | `thought_tile_create` |
| Shift / check goal focus | `/engram-goal` | `goal_set_primary` / `goal_list` |
| Full manifold exploration | `/engram-deep` | `set_memory_mode(deep)` + power tools |
| Return to fast default | `/engram-lean` | `set_memory_mode(lean)` |
| After substrate changes | `/engram-verify` | `verify_manifold_integrity` |
| Spatial empty on file | `/engram-ingest` | `force_spatial_ingest` / `incremental` |

---

## Cursor vs Grok Build throttle

| Harness | Minimum per session | Escalate freely |
|---------|---------------------|-----------------|
| **Cursor** | wake, edit on substrate files, session-end | update, scar, read, verify at boundaries |
| **Grok Build** | wake, edit every file, trace every fork, session-end | momentum, graph, tile, goal, relate |

**Invariant (both):** never `forget`+`remember` on same concept — use `/engram-update`.

---

## Typical sequences

**Code fix on Engram repo:**
```
/engram-wake → /engram-edit path → [work] → /engram-trace → /engram-solution (if verified) → /engram-session-end
```

**Meta design arc:**
```
/engram-wake → /engram-goal → /engram-recall → /engram-tile → [work] → /engram-update → /engram-session-end
```

**Stuck after grep:**
```
/engram-recall → /engram-read concept → /engram-momentum OR /engram-graph
```

**Deep audit then restore:**
```
/engram-deep → /engram-graph → /engram-verify → /engram-lean → /engram-session-end
```