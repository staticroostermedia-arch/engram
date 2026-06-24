# Relational-First Lean v2 — Implementation Plan

> **Goal:** `goal:relational_lean_v2` — graph-walk as default lean read; BVH+cuFile for discovery only; mandatory write breadcrumbs.

**Architecture:** Reuse `presentation_stratum` graph gather (primary_goal → serves → handoff → hot/recent) as the candidate pool for `recall(scope=anchors)`. Geometric scoring runs only within that pool. Fall back to `sampled_bounded` only while BVH warms; fall back to BVH+surface-filter for orphan discovery. Auto-relate every `remember`; auto-chain `quick_trace` to latest trace head.

**Tech stack:** Rust `engram-server`, `presentation_stratum`, `profile.rs`, MCP handlers, agent-memory harness.

---

## Phase 1 — Relational recall path (P0)

- [x] Extract `gather_surface_ranked(..., use_intent_recall)` from presentation stratum (break recall cycle)
- [x] `navigable_concept_names()` for relation-first candidate pool
- [x] `recall_scoped(anchors)` uses relational pool when `ENGRAM_RELATIONAL_RECALL=1` (agent default)
- [x] `sampled_bounded` only when BVH not ready on large stores
- [x] BVH discovery fallback filters to `is_surface_eligible`
- [x] `last_recall_path()` metadata for MCP (`relational` | `sampled_warmup` | `bvh_discovery`)

## Phase 2 — Write breadcrumbs (P0)

- [x] `auto_relate_after_write()` on `remember` → `primary_goal --documents--> concept`
- [x] `quick_trace` auto `prev_in_trace` from `latest_trace_head()` when `prev` omitted

## Phase 3 — Contract + profile (P1)

- [x] `ENGRAM_RELATIONAL_RECALL=1` in agent profile
- [x] Update `AGENT_MEMORY_CONTRACT.md`, `engram-wake-up.md`, `TOOL_DECISION_MAP.md`
- [x] MCP `recall` meta includes `recall_path`

## Phase 4 — cuFile milestone (P2 — stub documented)

- [ ] `ENGRAM_CUFILE_HOT=1` detection gate in `device_residency` (requires cuFile driver)
- [x] Document cuFile as discovery-path accelerator in plan + readiness hint

## Phase 5 — Auto-extraction sidecar (P3 — follow-up goal)

- [ ] `mcp_engram_turn_record` hook or daemon mirror → episodic mint + proposed edges
- [ ] Re-enable lightweight NREM in agent profile (`ENGRAM_NREM_DISABLE=0` opt-in)

## Verification

```bash
cargo test -p engram-server relational presentation_stratum recall
STABLE_BIN=target/debug/engram tools/test-harness/bin/engram-harness.sh --suite agent-memory
```