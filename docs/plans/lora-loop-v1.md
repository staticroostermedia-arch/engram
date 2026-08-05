# LoRA / weight loop v1

**Pipeline:** curated pack → train config → eval harness → promote or scar receipt

## Stages

1. **Export** — `scripts/export_experience_pack_v1.py` + quality gates
2. **Train** — adapter train on GPU1 preferred (or dry-run mock metrics)
3. **Eval** — pin: agent-memory suite, format/encode tests, protocol live, held-out goal replay
4. **Decide** — promote if metrics improve without CSF regression; else scar

## Receipt (required)

```json
{
  "schema": "lora_improvement_receipt_v1",
  "pack_hash": "...",
  "adapter_id": "dry_run|path",
  "before": {"csf_median": 0.94, "harness_pass": true},
  "after": {"csf_median": 0.94, "harness_pass": true},
  "decision": "scar|promote",
  "reason": "..."
}
```

Store as Engram `receipt:lora_*` or SCRATCH `lora-receipt.json`.
