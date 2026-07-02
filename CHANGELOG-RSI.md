# CHANGELOG-RSI — Recursive Self-Improvement Cycles

Companion to [CHANGELOG.md](CHANGELOG.md) for autonomous RSI iterations.

## [0.7.0-beta.6-rsi-1] — 2026-06-30

### Surprise-aware sentinel

- Hub-anchor `l2_norm_residual` → `surprise_pressure` modulates sentinel `effective_max_turns` (soft nudge only).
- Harness exposes `rsi_cycle_metrics` + extended `rsi_evolution.research_refs`.
- **Residual wiring (gap closure):** `engram_core::ops::prediction_residual` on `store::update`; ego/recent-trace prior on `remember`/`store` for hub anchors; `mcp::sentinel_turn_suffix` surprise-aware; `resolve_hub_anchors_for_surprise` presentation-stratum fallback pre-handoff.

**Sources:** [arXiv:2508.05766](https://arxiv.org/abs/2508.05766), [arXiv:2504.09301](https://arxiv.org/abs/2504.09301)

**Scores:** CRS 0.84 · Lyapunov 0.82 · RSI-accel 0.88 · perf 0.90 · safety 0.88

**Verify:** `scripts/rsi_cycle1_verify.sh` (OVERALL_EXIT=0) · **Trace:** `trace:1782839919_rsi-cycle-1-verification--surprise-aware-sentine` · **Tile:** `tile:formal_spec_rsi-cycle-1---surprise-aware-sentinel-v0-7-0-bet`

## [0.7.0-beta.7-rsi-batch1] — 2026-06-30 (Cycles 2–5)

### Cycle 2 — Lyapunov-ego sentinel blend
- `combined_sentinel_pressure` max-blends hub residual surprise + ego `drift_velocity`.
- Sources: [arXiv:2508.04435](https://arxiv.org/abs/2508.04435), [arXiv:2508.05766](https://arxiv.org/abs/2508.05766)

### Cycle 3 — turn_record session_intent parity
- `sentinel_turn_suffix` receives conv_arc/human_forward for presentation-stratum anchors.

### Cycle 4 — full_system_audit_loop TOML parse
- Removed invalid `subagent = null`; test `full_system_audit_loop_toml_parses`.

### Cycle 5 — Batch verify pipeline
- `scripts/rsi_batch_verify.sh`, `scripts/rsi_batch_mcp_capture.py`

**Batch scores:** CRS 0.85 · Lyapunov 0.85 · RSI-accel 0.87 · perf 0.91 · safety 0.89

**MCP Cycle 2:** `trace:1782841433_rsi-cycle-2-lyapunov-ego-sentinel-blend-shipped` · `tile:formal_spec_rsi-cycle-2---lyapunov-ego-blend`
**MCP Cycle 3:** `trace:1782841436_rsi-cycle-3-turn-record-session-intent-sentinel-` · `tile:formal_spec_rsi-cycle-3---session-intent-parity`
**MCP Cycle 4:** `trace:1782841440_rsi-cycle-4-full-system-audit-loop-toml-parse-fi` · `tile:formal_spec_rsi-cycle-4---audit-loop-toml`
**MCP Cycle 5:** `trace:1782841443_rsi-cycle-5-batch-verify-pipeline-v0-7-0-beta-7` · `tile:formal_spec_rsi-cycle-5---batch-verify`

## [0.7.0-beta.8-rsi-batch2] — 2026-06-30 (Cycles 6–9)

### Cycle 6 — Weighted Lyapunov sentinel blend
- `weighted_sentinel_pressure` + `ENGRAM_SENTINEL_RESIDUAL_WEIGHT` (default 0.65).
- Sources: [arXiv:2508.04435](https://arxiv.org/abs/2508.04435), [arXiv:2508.05766](https://arxiv.org/abs/2508.05766)

### Cycle 7 — ENGRAM_RSI_CYCLE harness metrics
- `resolve_rsi_cycle_number()` wires `rsi_cycle_metrics.cycle`; exposes `sentinel_residual_weight`.

### Cycle 8 — meta_workflow_registry harness exposure
- `meta_workflow_ok` in `rsi_cycle_metrics`; `meta_workflow_registry_in_harness_bundle` test.

### Cycle 9 — Batch verify extended
- `rsi_batch_verify_all.sh` supports `RSI_CYCLE_MIN/MAX` through cycle 9.

**Batch scores:** CRS 0.86 · Lyapunov 0.86 · RSI-accel 0.88 · perf 0.92 · safety 0.91

**MCP Cycle 6:** `trace:1782841559_rsi-cycle-6-weighted-lyapunov-sentinel-blend` · `tile:formal_spec_rsi-cycle-6---weighted-blend`
**MCP Cycle 7:** `trace:1782841563_rsi-cycle-7-engram-rsi-cycle-harness-metrics` · `tile:formal_spec_rsi-cycle-7---rsi-cycle-env`
**MCP Cycle 8:** `trace:1782841567_rsi-cycle-8-meta-workflow-registry-harness-expos` · `tile:formal_spec_rsi-cycle-8---meta-workflow-registry`
**MCP Cycle 9:** `trace:1782841570_rsi-cycle-9-batch-verify-extended-cycles-6-9-v0-` · `tile:formal_spec_rsi-cycle-9---batch-verify-6-9`

## [0.7.0-beta.9-rsi-batch3] — 2026-07-02 (Cycles 10–11)

### Cycle 10 — AutoMem turn_protocol harness (no filesystem change)
- `build_turn_protocol()` PLAN/ACT/LOG phases in wake harness (arXiv:2607.01224).
- `agent_discipline.turn_protocol` + `metamemory_kpis` in harness injection.
- Source: [arXiv:2607.01224](https://arxiv.org/abs/2607.01224) — memory as trainable skill.

### Cycle 11 — Session metamemory KPIs + MCP hooks
- `SessionMetamemoryCounters`: recalls, empty recall rate, writes/recall ratio, consult-before-write violations.
- `note_metamemory_tool` wired in `mcp::handle_tool_call`; snapshot in `rsi_cycle_metrics` + handoff packet + session receipt.

**Batch scores:** CRS 0.87 · Lyapunov 0.86 · RSI-accel 0.89 · perf 0.92 · safety 0.92

**Verify:** `RSI_CYCLE_MIN=10 RSI_CYCLE_MAX=11 scripts/rsi_batch_verify_all.sh`

## [0.7.0-beta.10-rsi-batch4] — 2026-07-02 (Cycles 12–13)

### Cycle 12 — consult-before-write gate
- `ENGRAM_CONSULT_BEFORE_WRITE` soft/hard/off gate on `remember`/`update`/`update_with_tensor_bond`.
- Harness exposes `consult_before_write_gate` in `rsi_cycle_metrics`.

### Cycle 13 — trajectory meta-review
- `build_trajectory_meta_review` aggregates metamemory across `receipt:session_*` sidecars.
- `scripts/rsi_trajectory_meta_review.sh` verify pipeline.

**Verify:** `RSI_CYCLE_MIN=12 RSI_CYCLE_MAX=13 scripts/rsi_batch_verify_all.sh`