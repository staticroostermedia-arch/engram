# Agent Memory Contract — 8 Essential Tools

**Status:** Stable (shipped 2026-06)  
**Audience:** Any AI agent using the Engram MCP server  
**Principle:** Lean by default, deep on demand. Eight tools cover wake → work → handoff on large stores (181k+ blocks) without ritual tax or RAM death.

### You were pointed here — bootstrap checklist

1. **Tools available?** You should see `mcp_engram_session_start`, `mcp_engram_recall`, etc. If not, ask the human to complete [FIRST_RUN.md](../FIRST_RUN.md) §1–2 (build + MCP config + IDE restart).
2. **First call:** `mcp_engram_session_start(intent="<what you're doing>")`. Read `continuation` in the response (goals, suggested actions, last session preview).
3. **Every session:** wake → work with the 8 tools below → `session_end(summary=…)`. That handoff is what makes the next wake feel continuous.
4. **Do not at wake:** `watch_workspace`, `rebuild_bvh`, `summarize`, `list_concepts` — use `context_for_edit(path)` per file instead.
5. **Need more?** 71 power tools exist; escalate deliberately via [TOOL_DECISION_MAP.md](TOOL_DECISION_MAP.md). Ritual detail: [docs/skills/engram-wake-up.md](skills/engram-wake-up.md), [engram-working-memory.md](skills/engram-working-memory.md), [engram-session-end.md](skills/engram-session-end.md).

> **71 more tools exist** (79 total). Power tools (`update`, `ack_wake_queue`, `evolution_at_locus`, `query_with_momentum`, `search_by_relation`, `remember_solution`, `scar`, `thought_tile_create`, …) remain available. This contract is the **Layer 0 highway** — not the full map.

**Full decision map:** [TOOL_DECISION_MAP.md](TOOL_DECISION_MAP.md) — mermaid flows for all 79 tools, write path (`update` vs `remember`), read escalation, and Grok Build vs Cursor throttle. **JIT RSI:** [DEFORMATION_PLAYBOOKS.md](DEFORMATION_PLAYBOOKS.md).

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
- **After wake:** execute `suggested_actions`, then **`mcp_engram_ack_wake_queue(executed=true)`** before `context_for_edit` when `ENGRAM_PROFILE=agent` (hard gate default). Empty queue auto-acks at wake.
- **`continuation` (slim by default, `ENGRAM_WAKE_BUNDLE=slim`)** — `primary_goal`, top 5 `suggested_actions`, `trace_chain_head`, slim `ego_snapshot`, `presentation_stratum` node_count + 5 previews
- **Full bundle on demand:** `mcp_engram_get_continuation_bundle` (set `ENGRAM_WAKE_BUNDLE=full` to restore legacy inline payload)
- `backend_readiness` (bvh_ready, recall_mode, leg_block_count)
- Optional `spatial_delta` when `include_spatial=true` (incremental ingest summary, not full force)

**`context_for_edit(path, line_start?, line_end?)`** — **Code atlas v2.1** pre-edit recon. Returns `spatial_items` (AST + `edit_arc` per locus), `traces_at_locus`, `traces_at_locus_tiers`, `scars_at_locus`, `spatial_siblings`, `edit_arc_debt`, and per-file `post_edit_palette` — **without** `watch_workspace` or full-store scan. Post-edit: `update({concept}__arc)` with delta narrative (use palette args when present); never bury history in source comments. Optional recon: `evolution_at_locus(path, line_start, line_end)`. See [CODE_ATLAS_CONTINUITY.md](CODE_ATLAS_CONTINUITY.md).

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
```

## Active long-running task tracking (Ariel north star substrate perfection)

This 8-tool contract and ritual discipline is actively dogfooded for the goal "Make Engram the perfect AI memory substrate for you" (long-running RSI/Ariel task per design:ariel_property_holographic_mind_map_north_star). See the plan (source of truth, with checklist, AC, verification, deviations) at `/home/a/.grok/sessions/%2Fhome%2Fa%2FDocuments%2FEngram/019eb976-8aed-7c43-a3c1-3be06a7ad806/goal/plan.md` on branch `feat/perfect-substrate-ariel-ritual-tracking`.

All changes use dedicated git branch + full Enram working memory process (session_start + ack, context_for_edit before edits, recall(anchors) before derive, quick_trace at forks w/ goal_context + prev, thought_tile_create w/ human_forward leading at key intervals, session_end(prepare_compression), goal_* tracking + relate to north star). Periodic human-facing reports as tiles. Explicit rollback exercised. Trace/tile IDs referenced in git commits for dual (manifold + VC) tracking and safe rollback. See plan for current status.

First human-facing report tile minted at this interval: tile:research_offload_initial-human-facing-status-report--engram-ritua (with leading human_forward on ritual/git progress and Ariel tracking). Quick trace: trace:1782148473_at-first-key-interval-after-wake--goal-activatio. Commit: ad0858ca.

Second human-facing report tile (knowledge_graph): tile:knowledge_graph_second-human-facing-report--enram-substrate-dogf . Quick trace: trace:1782148529_at-subsequent-phase-boundary--post-first-report- . Additional commit ea1cb86e updating docs with tile refs. Now 2 tiles, 2 commits on feat branch with trace refs. Working toward 3rd tile + rollback test.

Third human-facing report tile (state_machine): tile:state_machine_third-human-facing-report--phase-progress-and-su . Quick trace: trace:1782148559_completed-min-3-human-facing-tiles--research-off . Commit: 4c992b7f . Now 3 tiles, 3 commits. Min per AC3 met. Next: rollback test per AC5 and item 7.

Rollback test correction tile: tile:research_offload_rollback-test-correction-record--mistake-commit- . Quick trace: trace:1782148624_completed-rollback-test--mistake-edit-commit-bc4 . Mistake commit bc48ea84 reset, file/git restored, prior traces recallable. Evidence in scratch/rollback_evidence.log . Criteria re-verified.

FINAL human-facing report tile: tile:knowledge_graph_final-human-facing-report--goal-achieved---enram . Quick trace: trace:1782148643_final-comprehensive-human-facing-report-tile-min . Achievement declared, 5 commits listed (ad0858ca, ea1cb86e, 4c992b7f, 6b01dba2, bc48ea84 in reflog), 4 tiles lineage, rollback safety, no-rebrief (traces recallable), handoff will promote. Commit: b8f5e77b update. Goal achieved, session_end next.

# 3. HANDOFF (1 call)
mcp_engram_session_end(summary="<decisions, files, next steps>")
```

---

## Related Docs

- [SKILLS.md](../SKILLS.md) — ritual index (links here first)
- [docs/skills/engram-wake-up.md](skills/engram-wake-up.md) — 1-call wake protocol
- [docs/skills/engram-working-memory.md](skills/engram-working-memory.md) — edit loop with `context_for_edit`
- [docs/skills/engram-session-end.md](skills/engram-session-end.md) — handoff packet protocol
- [docs/MCP_TOOLS_REFERENCE.md](MCP_TOOLS_REFERENCE.md) — all 79 tools (8 essential)
- [docs/HARNESS_INJECTION.md](HARNESS_INJECTION.md) — wake queue, ego snapshot, continuity playbook

---

*This contract is the OS on top of the substrate. Follow it and the manifold compounds; skip it and every agent re-derives from flat context.*