# CLAIMS_LEDGER — Public claim → code → test map

**Status:** living  
**Last truth pass:** 2026-08-05 (local-primary critical path — cuFile taxonomy, protocol live, BlockTier permanent logical)  
**Purpose:** Every notable public claim maps to implementation status so agents and reviewers do not treat aspirational text as shipped fact.

**Status values:** `implemented` | `partial` | `aspirational` | `removed`

When status is `implemented`, named tests should exist in-repo. Prefer **softening docs** over inventing features.

---

## How to maintain

1. Add a row when shipping a new public claim (README / MANIFESTO / PATENT / guides / CHANGELOG).
2. On every release, re-spot-check rows marked `partial` or `aspirational`.
3. Overstatement notes must be honest: CRS is a local score, not cryptographic proof of truth; “ZK” in encode is hash attestation, not a zk-SNARK.

---

## Ledger

| Claim (short) | Source | Status | Primary code path(s) | Primary test(s) | Overstatement notes |
|---|---|---|---|---|---|
| Persistent geometric memory for AI agents via MCP | README.md L10–18 | implemented | `crates/engram-server/src/mcp.rs` (`tool_list`, `session_start`); `scripts/engram-grok` | harness `--suite agent-memory`; MCP agent-memory CI job | — |
| Fixed-size 256KB HolographicBlocks (.leg3) with 8192D q/p | README.md L131; MANIFESTO.md; types.rs layout | implemented | `crates/engram-core/src/types.rs` (`HolographicBlock`, `DIMENSION=8192`); `storage.rs` | `engram-core` layout / encode unit tests | Layout is real; not all product paths use O_DIRECT always |
| CRS lawfulness gate (grounded ≈ 0.74) | README.md L57; encode.rs CRS init | implemented | `encode.rs` (default 0.74); `store.rs` update Lyapunov path; `verify_manifold_integrity` | manifold integrity tests in `store.rs`; encode CRS defaults | CRS is a **local thermodynamic score**, not external truth or crypto attestation of correctness |
| BLAKE3 Merkle footer `sig_0`…`sig_5` | store update + block_integrity | implemented (chain depth) / partial (full history walk) | `advance_merkle_chain_slots` on update/scar; `sig_5` independent seal | `multi_slot_chain_after_three_advances`; `multi_update_merkle_chain_depth_and_valid_seal` | Multi-slot shift on update; not full historical reconstruction API |
| Footer = cryptographic hash of header+body | PATENT-NOTICE.md L25 | implemented | `engram_core::seal_whole_block` / `sig_5`; `write_block` reseals after Toryx PBC | `block_integrity` tests; `honest_lawfulness_integrity_tests` | Chain slots still shallow (`sig_0`/`sig_1`); seal is whole-block |
| `merkle_sub_root` links relation provenance | PATENT-NOTICE.md L26; store relate | implemented | `store.rs` relate: `merkle_sub_root = BLAKE3(sig_0_a \|\| sig_0_b)` | relation store tests | Stale if endpoint `sig_0` later advances without relation re-seal — verification must report lineage |
| Self-contained verification without external registry | PATENT-NOTICE.md L29–31 | partial | local block fetch + verify tools | `verify_block_lawfulness` / manifold integrity MCP | Lineage reconstruction across historical states is limited without extra logs |
| Solid-State Tensor / NVMe as context extension | README.md L133–147 | partial | `solid_state_tensor.rs`; `tensor_upsert` / `tensor_recall` MCP | `solid_state_tensor_verification_harness`; suite `tensor-thought-unification` | Requires warm BVH/`nvme_recall_ready`; GPUDirect/cuFile are **optional** hardware paths, not default on all machines |
| cuFile / GPUDirect / `full_bvh_gpu` | README.md L135; readiness fields | partial | `engram-gpu`, daemon readiness, `get_backend_readiness` | GPU tests feature-gated; CI often cpu-only | Ordinary GitHub runners have no GPU; claims are **capability-gated**, not always-on |
| OptiX RT-core BVH | build scripts / ENV docs | partial | `engram-gpu` OptiX path | local OptiX builds | Not available on standard CI |
| 8 essential MCP tools / 87 registered | README What’s new; AGENT_MEMORY_CONTRACT | implemented | `mcp.rs` `tool_list()`; lean tier in `tool_tier.rs` | `tool_list_count_matches_docs_contract_numbers` (asserts 87 = 83 mcp_engram + 4 linguistic) | Update assert + docs together when adding tools |
| Lean wake: one-call `session_start` + handoff | README; docs/AGENT_MEMORY_CONTRACT | implemented | `session_start` / `wake_bundle.rs` / `wake_digest.rs` / `session_end` | wake_digest + wake_bundle tests; agent-memory suite | Slim still multi-k tokens; `ENGRAM_WAKE_DIGEST_ONLY=1` for minimal packet; primary rebind reduces sticky-goal friction |
| `wake_digest_v1` + intent-shaped queue | CHANGELOG beta.12; harness | implemented | `wake_digest.rs` `build_wake_digest`; suggested_actions demote on intent mismatch | `wake_digest_v1_shape_and_sampled_warning`; `intent_mismatch_demotes_scar_and_boosts_handoff` | Digest is additive unless `ENGRAM_WAKE_DIGEST_ONLY=1` |
| Primary-goal rebind on intent mismatch | CHANGELOG beta.12; profile agent | implemented | `ENGRAM_PRIMARY_GOAL_REBIND`; session_start rebind path | rebind unit tests | Default `auto` under agent profile; `off` restores warn-only |
| Proof harness CI | dogfood #213 | implemented | `crates/engram-proof-harness`; CI job | proof harness tests | Seal/lawfulness regression — not a SNARK prover |
| Code atlas / `context_for_edit` | README L58, L164 | implemented | `store.rs` / mcp `context_for_edit` | edit fidelity / agent-tool-fidelity harness | — |
| Sheaf / `processes/*.toml` rituals | README L131; processes/ | implemented | process sheaf load at wake; `processes/` | sheaf-related server tests | “Sheaf H¹” for agent graphs is specialized; not every TOML is cohomology |
| Categorical linguistic calculus (diff/integrate/operadic) | README L171–195; CHANGELOG | partial | `engram-core/src/ops.rs` linguistic ops; MCP linguistic tools | `test_linguistic_full_p1_p5_pipeline_*` | Real ops exist; marketing “synthetic calculus” exceeds everyday agent usage |
| Hybrid wire serialization (HBRD) | encode.rs HBRD2 | implemented | `to_hybrid_wire` / `from_hybrid_wire` (HBRD2 full fidelity; HBRD1 legacy) | `hybrid_wire_full_roundtrip_fidelity` | cosine(q)>0.999, CRS equal, footer/payload restored; O_DIRECT .leg remains primary on-disk |
| Homomorphic + transform attestation | encode.rs | implemented | `generate_transform_attestation` / `verify_transform_attestation`; `generate_zk_proof` deprecated alias | `p2_homo_attestation_proof_verify` | **Not** zk-SNARK. Public name is attestation |
| Protocol execution / process subvisor H¹ | AGENTS.md; processes/monitor | partial | subvisor / process load | process/harness tests | Governance exists; full OP_INVERT/H¹ agent-graph theory is deeper than runtime enforcement |
| Lawfulness: `verify_manifold_integrity` / `verify_block_lawfulness` | AGENT_INTEGRATION; lawfulness | implemented (seal-aware sample) / partial (full history) | `lawfulness.rs` pure summary + store wrappers; manifold seal sample | `lawfulness::tests`; `honest_lawfulness_integrity_tests` | Full historical Merkle walk still not present; chain_slots_nonzero is depth-present only |
| NREM / ego.leg3 long-horizon continuity | README; MANIFESTO | implemented | daemon NREM path; ego.leg3 | NREM stack / profile tests (see dogfood PR #209) | Large-stack NREM needs dedicated thread (PR #209) |
| Trust residual / mutual morning packet | PR #210; wake path | implemented | `store.rs` `build_trust_residual`; `wake_bundle`; mcp wake hoist | `trust_residual_v1_bootstrap_and_handoff`; wake_bundle tests | Merged on master |
| REST recall returns empty under lean | historical bug | fixed (PR #209) | `serve.rs` `recall_scoped` default `scope=all` | REST dogfood path; serve path | MCP lean anchors intentionally different |
| PRAXIS hard contract | store update; agent profile | implemented | `ENGRAM_PRAXIS_CONTRACT`; agent profile `set_default(..., "hard")` | `praxis_contract_hard_tests`; profile tests | Agent default **hard**; other profiles soft unless set; override with `soft` for legacy |
| Automatic Autophagy GC daemon | MANIFESTO (historical) | **removed** | Daemon is watcher-only; no auto-evict | — | Use `mcp_engram_forget_old` for **explicit** low-CRS eviction only |
| Pinned CRS=1.0 immortal blocks | AGENT_INTEGRATION_GUIDE | implemented | pin / praxis promotion | pin/remember_solution paths | — |
| No neural embeddings in recall path | AGENT_INTEGRATION_GUIDE | implemented | `encode.rs` BLAKE3 spiral / unit-phase | encode determinism tests | — |
| Benchmarks / large-store wake “seconds on 192k” | README L165 | partial | readiness + lean wake | harness timing (env-specific) | Hardware-dependent; not a CI guarantee |

---

| Lawfulness module extract (`lawfulness.rs`) | store/mcp narrow extract | implemented | `crates/engram-server/src/lawfulness.rs` (`summarize_block_lawfulness`, seal tally helpers); store wrappers | `lawfulness::tests`; `honest_lawfulness_integrity_tests` | Narrow extract only — not a full store/mcp split |
| BVH quality path hint + QUALITY_MODE force | readiness / profile | implemented | `profile.rs` QUALITY_MODE forces DEFER_BVH=0; readiness `bvh_quality_path_hint` | profile quality_mode test; lawfulness bvh_quality_hint test | CPU agent still defers by default; no RAM bomb unless quality mode |
| Protocol invoke MCP | mcp tool list + store | partial (live whitelist) | bind default `tools_bound`; `live_steps=true` runs readiness/CSF whitelist → `executed` | `protocol_invoke_runs_real_toml_and_emits_receipt` | Full MCP graph not executed; only whitelisted safe tools |
| Linguistic extract real parse | types.rs mint_linguistic | implemented | `Leg3Pointer::extract_linguistic_bundle` serde_json linguistic/v1 | `test_linguistic_block_mint_roundtrip_crs_preserve` | Preserves words/coeffs/patches/functor_metadata; q leading reals match coeffs |
| ZEDOS tag uniqueness | types.rs registry | implemented | unique `ZEDOS_*` constants; FIBERED=0x5E | `zedos_tag_constants_are_unique` | NREM_CENTROID remains 0x4E |
| Unit-phase unbind path | encode from_text_unit_phase | implemented | `from_text_unit_phase` + op_bind/unbind | `unit_phase_unbind_recovery_above_0_95` | Recovery >0.95; spiral free-text remains default remember |
| Relation re-seal on endpoint update | store.rs | implemented | `reseal_relations_touching` after update | `relation_reseal_after_endpoint_update` | Recomputes merkle_sub_root for rel__a__b |
| Protocol invoke real TOML | store invoke_protocol | implemented (bind+receipt) | loads `processes/*.toml`, binds tools, emits receipt; `status=tools_bound` | `protocol_invoke_runs_real_toml_and_emits_receipt` | Never returns stub_dispatch or overclaim `executed` for declare-only steps |
| Geosphere frame persistence | store set/restore | implemented | `geosphere:latest_frame` ZEDOS_GEOSPHERE; restore on warm_wake | geosphere frame tests + restore hook | Runtime SymplecticState + durable snapshot |
| Cold-start fidelity median ≥0.90 (10 live wakes) | goal cognitive completion | implemented | live `mcp_engram_cold_start_fidelity` after each `session_end`+`session_start` boundary | SCRATCH/docs/evidence `csf-median-proof-2026-08-04.txt` median≈0.9445; **10 unique scores + 10 unique timestamps** (span ~306s) | Method requires live tool recompute (session_start alone may soft-stale-cache identical scores) |
| BlockTier physical layouts (Small/Large distinct sizes) | types.rs BlockTier | aspirational (permanent logical-only) | `is_permanent_logical_only`; all tiers `BLOCK_SIZE` on disk | `block_tier_permanent_logical_only_policy` | Explicit 2026-08 decision: no alternate physical sizes on current 256KB stalk |
| ROCm HIP query dispatch (Phase 10) | engram-gpu rocm_backend | partial / parked (no AMD on a-monad) | probe + CPU BVH; `hip_query_dispatch_shipped()==false` | `rocm_hip_dispatch_honest_baseline` | AMD-only when hardware present; not exercised on a-monad |
| cuFile transfer path taxonomy | engram-gpu cufile | implemented (reason enum) | `cufile_path_reason` + readiness fields; DMA only if success | `cufile_path_reason_hot_not_requested`; transfer_path labels | Vague `unavailable` alone replaced by structured reason |
| Hierarchy OS dual-GPU + hit rates | readiness | partial (labels + counters) | hierarchy_gpu0/1_role, hierarchy_hit_rates | `hierarchy_hit_rate_increments` | Policy documented; promote/demote still capacity-path driven |
| Local large-payload IPC (mmap/UDS) | local_ipc.rs | implemented | path-token + LegView mmap; UDS one-shot token | `mmap_leg_preview_returns_token_not_full_body`; `uds_path_token_roundtrip` | Prefer tokens over multi-MB JSON on-box |
| Agent suggested_actions cognitive bias | harness_injection ultra-lean | implemented | soft_elevated: dry_run p0, apply p2; agent pins recall/context_for_edit/quick_trace | `a3_agent_cognitive_bias_pins_in_ultra_lean_queue` | Hard elevated_hot_set still dry_run+apply at p0 |
| Hierarchy hit rates on recall path | hierarchy_metrics + score_recall_candidates | implemented | hot/warm/cold on recall satisfaction; promote/demote counters | `hierarchy_hit_rates_on_recall_sequence` | Not recorded on pure is_hot probes |
| Critical-path latency hooks (A1) | wake_digest + context_for_edit tests | implemented | build_wake_digest + context_for_edit timed hooks | `build_wake_digest_latency_hook`; `context_for_edit_hot_path_latency_hook` | Audit in docs/evidence/critical-path-audit-2026-08-05.md |
| H2D empty-path measure (C2-b) | cuda_dispatch measure_h2d_q_stage_ms | implemented | real upload_hot_q_to_device timing | `measure_h2d_q_stage_reports_ms` | Requires device_residency + CUDA |
| Experience pack export non-empty | scripts/export_experience_pack_v1.py | implemented | --bodies-json quality gates; refuses empty | docs/evidence/reference-pack-v1 (concept_count≥1) | Unfiltered stalk dump forbidden |
| Independence ladder Stage 0–3 | docs/evidence/independence-ladder-v1.md | implemented (schema+baseline) | counters + Stage-1 a-monad baseline | independence-baseline-2026-08-05.txt | Stage-2 local-only % reserved |
| OptiX RT-core product path on CI | engram-gpu OptiX | partial / aspirational on CI | local OptiX builds only | feature-gated local tests | Standard CI has no OptiX; capability-gated claim |

## Source index (files scanned this pass)

| File | Role |
|------|------|
| `README.md` | Public product claims |
| `MANIFESTO.md` | Theory + Merkle/CRS narrative |
| `PATENT-NOTICE.md` | Format claim table |
| `AGENT_INTEGRATION_GUIDE.md` | Agent-facing deep contract |
| `CHANGELOG.md` | Shipped feature claims |
| `docs/LAWFULNESS_VERIFICATION_PRIMITIVES.md` | Spec vs runtime |
| Major comments: `types.rs`, `encode.rs` footer/ZK notes |

---

## Corrections applied with this ledger

See git diff on this branch for doc softens that align overstated language with the table above (without inventing features).
