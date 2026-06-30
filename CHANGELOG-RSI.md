# CHANGELOG-RSI — Recursive Self-Improvement Cycles

Companion to [CHANGELOG.md](CHANGELOG.md) for autonomous RSI iterations.

## [0.7.0-beta.6-rsi-1] — 2026-06-30

### Surprise-aware sentinel

- Hub-anchor `l2_norm_residual` → `surprise_pressure` modulates sentinel `effective_max_turns` (soft nudge only).
- Harness exposes `rsi_cycle_metrics` + extended `rsi_evolution.research_refs`.
- **Residual wiring (gap closure):** `engram_core::ops::prediction_residual` on `store::update`; ego/recent-trace prior on `remember`/`store` for hub anchors; `mcp::sentinel_turn_suffix` surprise-aware.

**Sources:** [arXiv:2508.05766](https://arxiv.org/abs/2508.05766), [arXiv:2504.09301](https://arxiv.org/abs/2504.09301)

**Scores:** CRS 0.84 · Lyapunov 0.82 · RSI-accel 0.88 · perf 0.90 · safety 0.88

**Trace:** `trace:1782838166_rsi-cycle-1--surprise-aware-sentinel---tighten-r`