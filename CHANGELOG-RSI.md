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

**MCP Cycle 2:** `trace:1782841002_rsi-cycle-2-shipped-improvement` · `tile:formal_spec_rsi-cycle-2---continuity-batch`