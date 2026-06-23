# Context Injection + NVMe Bypass — Execution Plan (goal 019ec286)

**Branch:** `feat/perfect-context-injection-nvme-bypass`  
**Primary goal:** `goal:engram_mvp_v1`  
**Session plan:** `~/.grok/sessions/%2Fhome%2Fa%2FDocuments%2FEngram/019ec286-85aa-79d1-849b-c4a08298456a/goal/plan.md`

## Objective

Make Engram the best agent memory substrate by ensuring **complete, timely context injection** so NVMe + GPU paths bypass the Von Neumann bottleneck and deliver the right information at the right time.

## Review findings (2026-06-22)

| Area | State | Gap |
|------|-------|-----|
| Wake | Slim bundle default | Missing injection_completeness + nvme_context in slim tier |
| Ranking | CRS-only sort in bundle | No momentum/recency/hot/scar composite rank |
| NVMe | BVH + LegView on T700 | Readiness must surface `full_bvh_gpu`; Sheaf delegation fixed (d661db54) |
| Harness | suggested_actions queue | Scars front but no completeness metric |
| Perf | leg_block_count, sheaf cache | Fixed cd047ba0 |

## Execution plan (built)

1. **`injection_priority.rs`** — pure `prioritize_artifacts` + `compute_injection_completeness` (strict nvme/gpu slots)
2. **`build_continuation_bundle`** — composite rank via `prioritize_artifacts`; emit `injection_completeness` + `nvme_context`
3. **`harness_injection::build_suggested_actions`** — composite `injection_rank` on wake queue (not hardcoded priority)
4. **`slim_continuation_bundle`** — pass completeness + nvme_context; sort by `injection_rank`
5. **`backend_readiness`** — surfaces `nvme_direct_io` + `nvme_recall_ready`
6. **Harness** — agent-memory + continuation-bundle assertions updated for slim wake shape
7. **Integration test** — `store::build_continuation_bundle_emits_injection_observables`

## Key traces

- `trace:1782153784_begin-goal-019ec286-execution-on-feat-perfect-co`
- Prior substrate work: `d661db54`, `cd047ba0`

## Agent ritual

Lean 8-tool loop + `get_continuation_bundle` when slim completeness &lt; 0.85 or `nvme_context.bvh_ready` is false after ~30s.

## Manage resume (post TUI/MCP restart)

1. Restart TUI so MCP loads rebuilt `target/debug/engram`.
2. `session_start(intent="post-restart verify")` — verify `injection_completeness` + `nvme_context` in slim `continuation`.
3. Execute `suggested_actions` (composite `injection_rank`) → `ack_wake_queue`.
4. Poll `get_backend_readiness` until `full_bvh_gpu` on large store (~25–30s) or escalate to full bundle.
5. Harness sim: `STABLE_BIN=target/debug/engram tools/test-harness/bin/engram-harness.sh --suite agent-memory`.
6. **Goal complete:** all ACs pass → clear via `goal_update_status(completed)` + `demote_from_context` (Engram) or `update_goal(completed=true)` (TUI `/goal`); then `session_end(prepare_compression=true)`.
7. **Terminal:** push branch + PR notes per [CONTRIBUTING.md § Commit Message & Versioning Discipline](../CONTRIBUTING.md#commit-message--versioning-discipline) — full refs, file paths, ACs (not shorthand). Example: `Branch: feat/perfect-context-injection-nvme-bypass | Fixes: injection_completeness+nvme_context (store.rs, injection_priority.rs), injection_rank composite, BVH dedup | ACs: manage-resume + NVMe bypass pass | Refs: trace:... goal:manage_resume_019ec286`.