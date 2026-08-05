# Cognitive CSF residual closeout v1

**Parent:** `goal:engram_cognitive_substrate_completion_v1`  
**Branch:** `feat/cognitive-csf-residual-closeout`  
**Date:** 2026-08-04

## Acceptance (parent close)

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Format/stub gaps finished (HBRD2, linguistic extract, ZEDOS unique, unit-phase, relation reseal, protocol TOML, geosphere) | **done** (PR #221) | master `13c16fed` |
| CSF median ≥ 0.90 over 10 live cold wakes | **PASS** | `docs/evidence/csf-median-proof-2026-08-04.txt` median≈0.9445; 10 unique scores/timestamps via live `cold_start_fidelity` after session_end boundaries |
| CLAIMS honesty for shipped vs residual | **this PR** | `CLAIMS_LEDGER.md` CSF + residual rows; protocol MCP description fixed |
| Incompletes tracked with residual failers | **this PR** | `#[ignore]` residual tests + honesty baselines |

## Residual children (open after parent complete)

1. **`goal:residual_block_tier_physical`**  
   - Ship distinct physical byte layouts for `BlockTier::Small` / `Large` (or document permanent logical-only with CLAIMS `aspirational` if deferred forever).  
   - Un-ignore `residual_block_tier_physical_distinct_layouts` until green.

2. **`goal:residual_rocm_hip_dispatch`**  
   - Phase 10 HIP→Rust FFI query dispatch on AMD.  
   - Un-ignore `residual_rocm_hip_query_dispatch_shipped` until green.

3. **OptiX / cuFile / full Merkle history** — hardware/CI-gated; remain `partial` in CLAIMS (not silent stubs).

## Explicit non-goals this closeout

- Do **not** change `BLOCK_SIZE` / O_DIRECT 256 KiB invariant for Std.  
- Do **not** invent full ROCm kernels or OptiX CI on GitHub runners.  
- Do **not** bulk-forget `~/.engram`.

## Verification plan

```bash
cargo test -p engram-core block_tier_physical -- --nocapture
cargo test -p engram-core residual_block_tier -- --ignored  # expect FAIL until residual ships
cargo test -p engram-gpu rocm_hip -- --nocapture
cargo test -p engram-server solid_state_tensor_verification_harness -- --nocapture
cargo fmt --check
```

## CSF series (live stalk)

See `docs/evidence/cold-start-fidelity-10wakes-2026-08-04.txt`.
