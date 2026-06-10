# Agent Memory Contract — 8 Essential Tools

**Status:** Phase A target (Agent Memory MVP, 2026-06-06)  
**Audience:** Any AI agent using the Engram MCP server  
**Principle:** Lean by default, deep on demand. Eight tools cover wake → work → handoff on large stores (181k+ blocks) without ritual tax or RAM death.

> **55+ tools still exist.** Power tools (`query_with_momentum`, `visualize`, `thought_tile_create`, `verify_manifold_integrity`, …) remain available. This contract is the **minimal path** agents should follow unless deep mode or a specific task requires more.

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
- `continuation_bundle` (primary goal, last session_end preview, active artifacts, hydration cache flag)
- `backend_readiness` (bvh_ready, recall_mode, leg_block_count)
- Optional `spatial_delta` when `include_spatial=true` (incremental ingest summary, not full force)

**`context_for_edit(path)`** — Unified pre-edit recon. Returns memories whose AABB intersects the file, relevant praxis/traces, and line-range hints for `recall_in_file`-equivalent regions — **without** calling `watch_workspace` or scanning the whole store.

**`recall(query, k?, scope?)`** — Lexical similarity search. `scope` tiers results:
- `anchors` (default in lean) — `goal:`, `trace:`, `ritual:`, `helper:`, `praxis:` before episodic noise
- `spatial` — file/AABB-linked blocks
- `all` — full manifold search (deep mode default)

**`quick_trace(decision, why, …)`** — Same quality as `record_reasoning_trace` with fewer fields. Produces chained `trace:*` blocks the next wake surfaces first.

**`remember(concept, text)`** — New concept only. Always `recall` first; if score > 0.85 on an existing concept, use `update` instead (power tool, not in the 8).

**`session_end(summary, prepare_compression?)`** — Mandatory last call. Commits episodic terminal state and returns a **structured handoff packet** (JSON) for machine-readable continuation.

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
  "intent": "Continue Agent Memory MVP — document 8-tool contract (A6)",
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
| `mcp_engram_list` / `list_concepts` | Full store scan on 181k+ blocks | Never in lean; use `recall(scope=anchors)` |
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
- [design/agent_memory_mvp_plan.md](../design/agent_memory_mvp_plan.md) — implementation phases A1–A6
- [docs/MCP_TOOLS_REFERENCE.md](MCP_TOOLS_REFERENCE.md) — all 55+ tools

---

*This contract is the OS on top of the substrate. Follow it and the manifold compounds; skip it and every agent re-derives from flat context.*