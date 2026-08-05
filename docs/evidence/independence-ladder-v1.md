# Independence ladder metrics v1 (Wave I)

**Schema:** `independence_ladder_v1`  
**Goal:** `goal:engram_local_primary_critical_path_v1`

## Stages

| Stage | Definition | Local-primary claim |
|-------|------------|---------------------|
| **0** | Bootstrapped only; CSF flaky or primary unset; online-first | Not local-primary |
| **1** | CSF median ≥0.90; path truth + hierarchy + critical path measured; residuals named | **Stage-1 baseline (a-monad target)** |
| **2** | ≥80% sessions local-only success on recurring agent tasks; online-call rate tracked & low | Local-primary majority |
| **3** | Filtered experience→LoRA improve receipt promotes adapter; online optional by policy | Local-primary default |

## Counters (versioned block)

```json
{
  "schema_version": "independence_ladder_v1",
  "stage": 1,
  "host": "a-monad",
  "counters": {
    "local_only_session_success_pct": null,
    "residual_open_count": 3,
    "csf_median": 0.948,
    "csf_n": 5,
    "hierarchy_hit_rates": { "frac_hot": null, "frac_warm": null, "frac_cold": null, "note": "filled from readiness after recall sequence" },
    "online_call_rate": null,
    "online_call_rate_note": "not instrumented in MCP yet — counter reserved"
  },
  "residuals_named": [
    "cufile_dma_success_pending",
    "lora_weight_train_not_run",
    "rocm_parked_no_amd"
  ],
  "baseline_captured_at": "2026-08-05"
}
```

## Stage-1 exit criteria (met when)

- [x] Path truth: cuFile taxonomy + empty-path evidence with real methods
- [x] Hierarchy hit rates logged on recall path
- [x] Critical-path audit + latency hooks
- [x] CSF median ≥0.90 on ≥5 unique live samples
- [x] Residuals named (not silent stubs)
- [ ] Stage-2 local-only % (requires multi-session counter — future)
