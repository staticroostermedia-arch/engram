# Experience export schema v1 (local-primary data factory)

**Version:** `experience_pack_v1`  
**Goal:** filtered self-generated data — never unfiltered stalk dump.

## Pack layout

```
pack_id/
  manifest.json          # schema_version, pack_hash, created_at, filters
  trajectories/*.jsonl   # edit arcs + session boundaries
  preferences/*.jsonl    # A/D/R preference pairs from traces
  negatives/*.jsonl      # scars, uncertainty receipts
  harness/*.json         # test receipts, CSF snapshots
  reference/*.json       # frozen high-CRS anchors (optional)
```

## Quality gates (required for positives)

1. CRS ≥ 0.74 (prefer ≥ 0.85 for reference)
2. Receipt or trace chain present for training positives
3. Exclude thin/unguarded blocks (no decision, no falsifiers-only noise)
4. No raw local_only secrets in export (host profile: scrub store paths optional)
5. Held-out eval set reserved before any promote

## Filters (doctrine)

- Unfiltered self-train **forbidden**
- Scars are negatives, not positives
- Protocol `tools_bound` alone is not a success trajectory unless live steps ran

## CLI / script

`scripts/export_experience_pack_v1.py` — deterministic export from concept list / prefixes.
