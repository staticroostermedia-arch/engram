# Agent Memory Contract — 8 Essential Tools

**Status:** Stable (shipped 2026-06)  
**Audience:** Any AI agent using the Engram MCP server  
**Principle:** Lean by default, deep on demand. Eight tools cover wake → work → handoff on large stores (181k+ blocks) without ritual tax or RAM death.

### You were pointed here — bootstrap checklist

1. **Tools available?** You should see `mcp_engram_session_start`, `mcp_engram_recall`, etc. If not, ask the human to complete [FIRST_RUN.md](../FIRST_RUN.md) §1–2 (build + MCP config + IDE restart).
2. **First call:** `mcp_engram_session_start(intent="<what you're doing>")`. Read `continuation` in the response (goals, suggested actions, last session preview).
3. **Every session:** wake → work with the 8 tools below → `session_end(summary=…)`. That handoff is what makes the next wake feel continuous.
4. **Do not at wake:** `watch_workspace`, `rebuild_bvh`, `summarize`, `list_concepts` — use `context_for_edit(path)` per file instead.
5. **Need more?** 62 power tools exist; escalate deliberately via [TOOL_DECISION_MAP.md](TOOL_DECISION_MAP.md). Ritual detail: [docs/skills/engram-wake-up.md](skills/engram-wake-up.md), [engram-working-memory.md](skills/engram-working-memory.md), [engram-session-end.md](skills/engram-session-end.md).

> **62 more tools exist** (70 total). Power tools (`update`, `query_with_momentum`, `search_by_relation`, `remember_solution`, `scar`, `thought_tile_create`, …) remain available. This contract is the **Layer 0 highway** — not the full map.

**Full decision map:** [TOOL_DECISION_MAP.md](TOOL_DECISION_MAP.md) — mermaid flows for all 70 tools, write path (`update` vs `remember`), read escalation, and Grok Build vs Cursor throttle. **JIT RSI:** [DEFORMATION_PLAYBOOKS.md](DEFORMATION_PLAYBOOKS.md).

---

## The 8 Essential Tools

| # | Tool | Role |
|---|------|------|
| 1 | `mcp_engram_session_start` | **Wake.** Bind thermodynamic context; return inline continuation bundle + backend readiness in one call. |
| 2 | `mcp_engram_context_for_edit` | **Edit prep.** File-scoped spatial + memory context in one call (no whole-repo watch). |
| 3 | `mcp_engram_recall` | **Read.** Anchor-first search with optional `scope` tiering. |
| 4 | `mcp_engram_quick_trace` | **Decide.** Low-friction structured `trace:*` capture at forks. |
| 5 | `mcp_engram_remember` | **Write.** New concepts only (after recall check). |
| 6 | `mcp_engram_session_end` | **Handoff.** Terminal state + structured handoff packet for next wake. |
| 7 | `mcp_engram_get_backend_readiness` | **Probe.** BVH/GPU/recall-mode status without heavy side effects. |
| 8 | `mcp_engram_set_memory_mode` | **Mode.** Switch `lean` ↔ `deep` at runtime (mirrors `ENGRAM_MEMORY_MODE`). |

### Tool summaries

**`session_start(intent, include_spatial?)`** — Mandatory first call. Mints `session_start_*` episodic block, loads process sheaf, and returns:
- **`continuation` (slim by default, `ENGRAM_WAKE_BUNDLE=slim`)** — `primary_goal`, top 5 `suggested_actions`, `trace_chain_head`, slim `ego_snapshot`, `presentation_stratum` node_count + 5 previews
- **Full bundle on demand:** `mcp_engram_get_continuation_bundle` (set `ENGRAM_WAKE_BUNDLE=full` to restore legacy inline payload)
- `backend_readiness` (bvh_ready, recall_mode, leg_block_count)
- Optional `spatial_delta` when `include_spatial=true` (incremental ingest summary, not full force)

**`context_for_edit(path, line_start?, line_end?)`** — **Code atlas v2** pre-edit recon. Returns `spatial_items` (AST + `edit_arc` per locus), `traces_at_locus`, `scars_at_locus`, `spatial_siblings`, and anchor goals/traces — **without** `watch_workspace` or full-store scan. Post-edit: `update({concept}__arc)` with delta narrative; never bury history in source comments. See [CODE_ATLAS_CONTINUITY.md](CODE_ATLAS_CONTINUITY.md).

**`recall(query, k?, scope?)`** — Lexical similarity search. `scope` tiers results:
- `anchors` (default in lean) — `goal:`, `trace:`, `ritual:`, `helper:`, `praxis:` before episodic noise
- `spatial` — file/AABB-linked blocks
- `all` — full manifold search (deep mode default)

**`quick_trace(decision, why, …)`** — Same quality as `record_reasoning_trace` with fewer fields. Produces chained `trace:*` blocks the next wake surfaces first.

**`remember(concept, text)`** — New concept only. Always `recall` first; if score > 0.85 on an existing concept, use `update` instead (Layer 1 — see [write path](TOOL_DECISION_MAP.md#write-path-non-negotiable)).

**`session_end(summary, minimal?, prepare_compression?)`** — End-of-block handoff. Use **`minimal=true`** for fast fix loops (thin block + boundary trace + `helper:session_handoff_latest`, no compression ritual). Full path (default) runs compression handoff + rich boundary trace. MCP disconnect without `session_end` auto-emits a thin handoff.

**`get_backend_readiness()`** — Read-only status. Use after wake or when recall quality seems sampled/bounded.

**`set_memory_mode(mode)`** — `lean` or `deep`. Env default: `ENGRAM_MEMORY_MODE=lean`.

---

## Lean vs Deep Mode

| Aspect | **Lean** (default) | **Deep** |
|--------|-------------------|----------|
| Env / runtime | `ENGRAM_MEMORY_MODE=lean` or `set_memory_mode("lean")` | `ENGRAM_MEMORY_MODE=deep` or `set_memory_mode("deep")` |
| Wake | **1 call:** `session_start` (inline bundle) | `session_start` + optional `get_continuation_bundle`, `query_pure`, `summarize` |
| Recall default scope | `anchors` — goals/traces/rituals before episodic | `all` — full manifold |
| Spatial | `context_for_edit(path)` per file touched | May add `watch_workspace` once per project if daemon passive ingest needed |
| BVH / GPU | Use `sampled_bounded` recall; **do not** `rebuild_bvh` unless user asks | May call `rebuild_bvh` + poll `get_backend_readiness` for `full_bvh_gpu` |
| Traces | `quick_trace` at forks | `quick_trace` + `record_reasoning_trace` for high-stakes |
| Handoff | `session_end` → structured packet | Same + explicit `promote_hot_batch` on tiles (power tool) |
| Target | <500MB RSS, <2s wake on 181k store | Quality recall, full geometric navigation |

Set mode at session open if the task needs deep exploration:

```
mcp_engram_set_memory_mode(mode="deep")
```

Reset to lean before ending long meta sessions to protect the next agent's wake latency.

---

## One-Call Wake Example

**Lean contract:** a single `session_start` replaces the old 5+ tool wake sequence (`get_continuation_bundle`, `query_pure`, `incremental_spatial_ingest`, `promote_hot_batch`, `summarize`).

### Request

```json
{
  "intent": "Implement feature X in my project — lean wake",
  "include_spatial": false
}
```

### Response (planned inline bundle)

```json
{
  "status": "started",
  "session_key": "session_start_1749225600",
  "elapsed_to_ack_secs": 0.08,
  "memory_mode": "lean",
  "continuation_bundle": {
    "primary_goal": "goal:agent_memory_mvp",
    "last_session_end": {
      "concept": "session_end_1749222000",
      "age_secs": 3600,
      "preview": "A5 handoff JSON wired in store.rs. Next: A6 docs + skill update. Files: mcp.rs, store.rs. Blockers: none."
    },
    "hydration_cache_present": true,
    "active_artifacts": [
      {
        "concept": "helper:session_hydration_cache",
        "crs": 0.95,
        "hot": true,
        "source": "hydration_cache",
        "preview": "SESSION HYDRATION CACHE … wake_protocol: session_start → read CONTINUATION BUNDLE → recall_first …"
      },
      {
        "concept": "ritual:engram.working-memory",
        "crs": 1.0,
        "hot": true,
        "source": "hot_set",
        "preview": "Working memory discipline — recall before derive, update-preferred, trace at forks …"
      },
      {
        "concept": "trace:1749221000_agent_memory_mvp_plan",
        "crs": 0.91,
        "hot": true,
        "source": "goal_serves_lineage",
        "preview": "decision_point: 8-tool contract over 60-tool surface …"
      }
    ],
    "recall_hint": "Use recall(scope=anchors) on artifact concepts for full payload."
  },
  "backend_readiness": {
    "fully_initialized": true,
    "bvh_ready": false,
    "recall_mode": "sampled_bounded",
    "backend_kind": "cuda",
    "gpu_accel_available": true,
    "leg_block_count": 181432,
    "defer_bvh": true
  },
  "next_steps": [
    "Work using context_for_edit + recall(scope=anchors) + quick_trace + remember",
    "End with session_end to mint handoff_packet",
    "Escalate to deep mode only if anchor recall is insufficient"
  ]
}
```

**Agent action after wake:** Read `continuation_bundle.primary_goal` and `last_session_end.preview`. You are geometrically continuing — not starting fresh.

---

## Edit Loop Example

Lean mode edit discipline uses **`context_for_edit`** once per file, then **`recall`** for gaps, **`quick_trace`** at forks, **`remember`**/`update` for writes.

### 1. Pre-edit — single spatial call

```
mcp_engram_context_for_edit(path="crates/engram-server/src/mcp.rs")
```

```json
{
  "path": "crates/engram-server/src/mcp.rs",
  "file_stem": "mcp",
  "spatial_hits": [
    { "concept": "praxis:mcp_session_start_fast_path", "aabb": [2160, 2227], "crs": 1.0 },
    { "concept": "trace:1749200000_session_start_inline_bundle", "aabb": [2166, 2200], "crs": 0.89 }
  ],
  "recall_suggestions": [
    "session_start inline bundle",
    "continuation_bundle cache invalidate"
  ],
  "ingest_status": "passive_daemon_ok",
  "mode": "lean"
}
```

### 2. Anchor recall for design context

```
mcp_engram_recall(query="session_start inline bundle readiness", k=5, scope="anchors")
```

### 3. Fork — quick trace before editing

```
mcp_engram_quick_trace(
  decision="Return bundle inline in session_start response instead of separate get_continuation_bundle call",
  why="One-call wake hits <2s target on 181k store; eliminates 4 post-start round-trips",
  alternatives="Keep separate bundle tool for TUI 63-65% compression boundary only",
  would_falsify="Harness shows wake >2s or client drops MCP registration on fat response",
  context="ritual:engram.working-memory",
  prev="trace:1749221000_agent_memory_mvp_plan"
)
```

### 4. Post-edit — remember outcome (if no existing concept)

```
mcp_engram_remember(
  concept="progress:agent_memory_a1_inline_bundle",
  text="session_start returns continuation_bundle + backend_readiness inline. include_spatial optional."
)
```

**Do not** call `watch_workspace` in lean mode unless the passive daemon is not ingesting saves (rare). **Do not** call `rebuild_bvh` during routine edits.

---

## End Handoff Example

### Request

```
mcp_engram_session_end(
  summary="A6 complete: AGENT_MEMORY_CONTRACT.md + skill updates. Decisions: 8-tool lean path documented, 1-call wake examples, context_for_edit in edit loop. Next: harness agent-memory-mvp suite (Phase B).",
  prepare_compression=true
)
```

### Response — structured handoff packet

```json
{
  "status": "ended",
  "session_end_key": "session_end_1749229200",
  "avg_crs_touched": 0.87,
  "handoff_packet": {
    "primary_goal": "goal:agent_memory_mvp",
    "terminal_summary": "A6 complete: AGENT_MEMORY_CONTRACT.md + skill updates …",
    "open_blockers": [],
    "next_actions": [
      "Phase B: async load_process_sheaf",
      "Harness suite agent-memory-mvp"
    ],
    "key_traces": [
      "trace:1749228000_a6_docs_contract",
      "trace:1749221000_agent_memory_mvp_plan"
    ],
    "files_touched": [
      "docs/AGENT_MEMORY_CONTRACT.md",
      "SKILLS.md",
      "docs/skills/engram-wake-up.md",
      "docs/skills/engram-session-end.md",
      "docs/skills/engram-working-memory.md"
    ],
    "compression_handoff_key": "compression_handoff_1749229200",
    "hydration_cache_refreshed": true,
    "hot_promoted_count": 6,
    "continuation_relation": "provides_continuation_for",
    "wake_protocol": "Next agent: session_start(intent) → read handoff_packet + continuation_bundle inline"
  },
  "protocol_gaps": []
}
```

The next instance's `session_start` surfaces `handoff_packet` fields inside `continuation_bundle.last_session_end` and `active_artifacts`.

---

## What NOT to Call in Lean Mode

| Avoid (unless needed) | Why | When it's OK |
|------------------------|-----|--------------|
| `mcp_engram_watch_workspace` | Binds full-repo watcher; memory/RAM cost on large trees | Deep mode, or passive daemon confirmed down |
| `mcp_engram_rebuild_bvh` | Minutes + RAM spike on 100k+ blocks | User requests quality recall; deep mode + poll readiness |
| `mcp_engram_get_continuation_bundle` | Redundant — inline in `session_start` | TUI 63–65% compression boundary (pre-compression snapshot) |
| `mcp_engram_query_pure` / `query_with_momentum` | Extra wake round-trips | Deep mode or anchor recall returned empty |
| `mcp_engram_summarize` | Duplicates bundle artifacts | Deep mode orientation pass |
| `mcp_engram_incremental_spatial_ingest` | Bundled via `session_start(include_spatial=true)` | Explicit path bootstrap |
| `mcp_engram_list_concepts` | Full store scan on 181k+ blocks | Never in lean; use `recall(scope=anchors)` |
| `mcp_engram_force_spatial_ingest` | Full force ingest (81+ files) | Cold bootstrap only, deep mode |

**Rule of thumb:** If the 8-tool path answers your question, stop. Escalate to power tools deliberately, then return to lean.

---

## Full Session Loop (Copy-Paste)

```
# 1. WAKE (1 call)
mcp_engram_session_start(intent="<your goal>", include_spatial=false)

# 2. WORK
mcp_engram_context_for_edit(path="<file you're editing>")
mcp_engram_recall(query="<keywords>", scope="anchors")
# … edit …
mcp_engram_quick_trace(decision="...", why="...")
mcp_engram_remember(concept="...", text="...")  # or update if match >0.85

# 3. HANDOFF (1 call)
mcp_engram_session_end(summary="<decisions, files, next steps>")
```

---

## Related Docs

- [SKILLS.md](../SKILLS.md) — ritual index (links here first)
- [docs/skills/engram-wake-up.md](skills/engram-wake-up.md) — 1-call wake protocol
- [docs/skills/engram-working-memory.md](skills/engram-working-memory.md) — edit loop with `context_for_edit`
- [docs/skills/engram-session-end.md](skills/engram-session-end.md) — handoff packet protocol
- [docs/MCP_TOOLS_REFERENCE.md](MCP_TOOLS_REFERENCE.md) — all 70 tools (8 essential)
- [docs/HARNESS_INJECTION.md](HARNESS_INJECTION.md) — wake queue, ego snapshot, continuity playbook

---

*This contract is the OS on top of the substrate. Follow it and the manifold compounds; skip it and every agent re-derives from flat context.*