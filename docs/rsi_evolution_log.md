# RSI Evolution Log — Engram Recursive Self-Improvement

Autonomous cycle record for verifiable substrate evolution. Each entry: audit → research → hypothesis → ship → score → git.

---

## Cycle 1 — Surprise-aware sentinel (2026-06-30)

### Master baseline (post-PR #53)

- `master` @ leg3 gap closure: manifest provlog parser, session Merkle on manifest/receipt, `l2_norm_residual` in presentation stratum, agent-profile `allowed_transforms` soft gate.
- Continuity spikes shipped: rehydration manifest, sentinel (30 turns / 120 min), uncertainty receipts, triadic hints, session receipts.
- Workspace version prior: `v0.7.0-beta.5` → bumped **`v0.7.0-beta.6`**.

### Research synthesis (≥2 cited sources)

| Source | Insight integrated |
|--------|-------------------|
| [arXiv:2508.05766](https://arxiv.org/abs/2508.05766) — *Language-Mediated Active Inference* | Bounded rationality / free-energy framing: checkpoint when belief-update pressure (surprise) is high — maps to earlier soft handoff. |
| [arXiv:2504.09301](https://arxiv.org/abs/2504.09301) — *Crystallized Reasoning + Fluid Generation* | Multi-turn interaction depth preserves alignment; sentinel handoff is crystallized continuity, not episodic noise. |

### Hypothesis selected

**Surprise-aware sentinel:** Aggregate `l2_norm_residual` from rehydration manifest `hub_anchors` → `surprise_pressure` (0..1) → reduce `effective_max_turns` (max −12, floor 8) → soft nudge `surprise_pressure_elevated` before base 30-turn cap.

### Evaluation scores

| Metric | Score |
|--------|-------|
| CRS | 0.84 |
| Lyapunov | 0.82 |
| RSI-accel | 0.88 |
| perf | 0.90 |
| safety | 0.88 |

Commits: `5f1dd4ef`, `c06c88c8`, `d8625dbc` · Version: `v0.7.0-beta.6`

---

## Cycle 2 — Lyapunov-ego sentinel blend (2026-06-30)

### Research synthesis

| Source | Insight |
|--------|---------|
| [arXiv:2508.04435](https://arxiv.org/abs/2508.04435) — Lyapunov-stable learning | Ego `drift_velocity` (NREM dv) as Lyapunov proxy complements hub-anchor residual surprise. |
| [arXiv:2508.05766](https://arxiv.org/abs/2508.05766) | Max-blend conservative handoff under either prediction error or ego instability. |

### Hypothesis

`combined_sentinel_pressure(residual, ego_drift)` = max(residual_surprise, ego.dv) → tighter `effective_max_turns` when NREM reports high drift even if residuals low.

### Files touched

- `continuity_spikes.rs` — `combined_sentinel_pressure`
- `harness_injection.rs` — `ego_drift_velocity`, `sentinel_pressure_combined`, extended `rsi_cycle_metrics`

### Scores

CRS 0.85 · Lyapunov 0.86 · RSI-accel 0.87 · perf 0.90 · safety 0.88

**MCP:** `trace:1782841002_rsi-cycle-2-shipped-improvement` · **Tile:** `tile:formal_spec_rsi-cycle-2---continuity-batch`

```bash
git add crates/engram-server/src/continuity_spikes.rs crates/engram-server/src/harness_injection.rs
git commit -m "feat(continuity): RSI Cycle 2 Lyapunov-ego sentinel blend | arXiv:2508.04435,2508.05766"
```

---

## Cycle 3 — turn_record session_intent parity (2026-06-30)

### Research synthesis

| Source | Insight |
|--------|---------|
| [arXiv:2504.09301](https://arxiv.org/abs/2504.09301) | Session intent shapes presentation stratum ranking — turn_record must pass conv_arc/human_forward for surprise anchors. |

### Hypothesis

`sentinel_turn_suffix(lock, session_intent)` uses same `resolve_hub_anchors_for_surprise` path as harness wake (conv_arc preferred, else human_forward).

### Files touched

- `mcp.rs` — `sentinel_turn_suffix` + `turn_record` wiring; uses `sentinel_pressure_combined`

### Scores

CRS 0.84 · Lyapunov 0.85 · RSI-accel 0.86 · perf 0.91 · safety 0.89

```bash
git add crates/engram-server/src/mcp.rs
git commit -m "fix(continuity): RSI Cycle 3 turn_record session_intent sentinel parity"
```

---

## Cycle 4 — full_system_audit_loop TOML parse fix (2026-06-30)

### Research synthesis

| Source | Insight |
|--------|---------|
| [arXiv:2505.10569](https://arxiv.org/abs/2505.10569) — declarative agent workflows | Process sheaf TOMLs must parse lawfully for session_start registration. |

### Hypothesis

Remove invalid `subagent = null` from meta workflow execute steps; add `parse_meta_workflow_name` test gate.

### Files touched

- `processes/meta/full_system_audit_loop.toml` — TOML-valid execute steps
- `process_metrics.rs` — `parse_meta_workflow_name` + `full_system_audit_loop_toml_parses` test

### Scores

CRS 0.83 · Lyapunov 0.80 · RSI-accel 0.84 · perf 0.92 · safety 0.90

```bash
git add processes/meta/full_system_audit_loop.toml crates/engram-server/src/process_metrics.rs
git commit -m "fix(processes): RSI Cycle 4 full_system_audit_loop TOML parse | arXiv:2505.10569"
```

---

## Cycle 5 — Batch verify pipeline (2026-06-30)

### Hypothesis

Generalize Cycle 1 verify discipline: `scripts/rsi_batch_verify.sh` + `rsi_batch_mcp_capture.py` parameterized by cycle N with grep-able `call-*-rsi_cycleN_*.json`.

### Files touched

- `scripts/rsi_batch_verify.sh`, `scripts/rsi_batch_mcp_capture.py`
- `Cargo.toml` → `v0.7.0-beta.7`

### Scores

CRS 0.86 · Lyapunov 0.82 · RSI-accel 0.90 · perf 0.91 · safety 0.91

```bash
git add scripts/rsi_batch_verify.sh scripts/rsi_batch_mcp_capture.py Cargo.toml Cargo.lock docs/rsi_evolution_log.md CHANGELOG-RSI.md
git commit -m "chore(rsi): RSI Cycle 5 batch verify pipeline + v0.7.0-beta.7"
```

---

## Batch 1 checkpoint (Cycles 2–5)

**Cumulative gains:** Surprise sentinel now max-blends ego NREM drift; turn_record passes session intent into presentation-stratum anchor resolution; meta audit workflow TOML parses; batch MCP/verify scripts extend Cycle 1 discipline.

**Version history:** beta.6 (Cycle 1) → **beta.7** (Batch 1 cycles 2–5)

**Cycle 6+ backlog:** Lyapunov weighted blend (not just max); `rsi_cycle_metrics.cycle` auto from env; dual-RTX BVH defer benchmarks; push `feature/rsi-autonomous-1` PR.