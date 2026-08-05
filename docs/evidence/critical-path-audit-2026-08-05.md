# A1 Critical-path alloc/latency audit (a-monad)

**Goal:** `goal:engram_local_primary_critical_path_v1`  
**Date:** 2026-08-05

## Hot functions (wake → select → deliver → edit)

| Function | Module | Role | Alloc notes | Guard |
|----------|--------|------|-------------|-------|
| `build_wake_digest` | `wake_digest.rs` | Compact agent-facing wake summary | Pure JSON; no store I/O; takes pre-built action/scar slices | `build_wake_digest_latency_hook` (avg &lt;5ms over 200 iters) |
| `build_suggested_actions_ultra_lean` | `harness_injection.rs` | Lean wake queue | Zero extra store I/O when manifest pre-resolved; caps queue at 8 | existing ultra-lean tests + A3 cognitive bias |
| `backend_readiness` | `store.rs` | Readiness object | TTL cache; static flags once | soft-stale path |
| `recall_scoped` / `score_recall_candidates` | `store.rs` | Hot recall | Candidate-bounded; hierarchy tier recorded per satisfaction | `hierarchy_hit_rates_on_recall_sequence` |
| `context_for_edit` | `store.rs` | Pre-edit atlas | Single-file locus; must not scan full stalk | `context_for_edit_hot_path_latency_hook` (&lt;30s soft bound) |
| `upload_hot_q_to_device` / `measure_h2d_q_stage_ms` | `cuda_dispatch.rs` | H2D stage | Device buffer + one memcpy; free after measure | `measure_h2d_q_stage_reports_ms` |

## Measurable improvements this wave

1. Hierarchy hit recording moved off pure `is_hot` probes onto **recall satisfaction** (no false inflate).
2. Soft capacity wake queue no longer monopolizes top slots with dry_run+apply (A3).
3. Large geometric payloads: path-token + mmap, not multi-MB JSON (A2).
4. Explicit latency hooks on `build_wake_digest` and `context_for_edit` for regression detection.

## Still out of band (named, not silent)

- Full arena pre-size for entire `session_start` assemble (already lean-flagged; further cuts need flamegraph pass).
- Real cuFile DMA success path when GDS unavailable (taxonomy only).
