# CLAIMS_LEDGER — Public claim → code → test map

**Status:** living  
**Last truth pass:** 2026-08-03 (honesty-closure wave A)  
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
| BLAKE3 Merkle footer `sig_0`…`sig_5` | README.md L131; PATENT-NOTICE.md L25; MANIFESTO.md ~105–109 | partial | `types.rs` `LegFooter`; `store.rs` update advances `sig_1←sig_0`, `sig_0←BLAKE3(q)`; `encode.rs` sets `sig_0` from text hash | update/relate provenance tests in `store.rs` | Chain is **shallow in practice** (mostly `sig_0`/`sig_1`, scar uses `sig_2`). Not a full 6-deep temporal crystal on every write. Whole-block seal of header+body → `sig_5` is hardening separately (see block-integrity work) |
| Footer = cryptographic hash of header+body | PATENT-NOTICE.md L25 | implemented | `engram_core::seal_whole_block` / `sig_5`; `write_block` reseals after Toryx PBC | `block_integrity` tests; `honest_lawfulness_integrity_tests` | Chain slots still shallow (`sig_0`/`sig_1`); seal is whole-block |
| `merkle_sub_root` links relation provenance | PATENT-NOTICE.md L26; store relate | implemented | `store.rs` relate: `merkle_sub_root = BLAKE3(sig_0_a \|\| sig_0_b)` | relation store tests | Stale if endpoint `sig_0` later advances without relation re-seal — verification must report lineage |
| Self-contained verification without external registry | PATENT-NOTICE.md L29–31 | partial | local block fetch + verify tools | `verify_block_lawfulness` / manifold integrity MCP | Lineage reconstruction across historical states is limited without extra logs |
| Solid-State Tensor / NVMe as context extension | README.md L133–147 | partial | `solid_state_tensor.rs`; `tensor_upsert` / `tensor_recall` MCP | `solid_state_tensor_verification_harness`; suite `tensor-thought-unification` | Requires warm BVH/`nvme_recall_ready`; GPUDirect/cuFile are **optional** hardware paths, not default on all machines |
| cuFile / GPUDirect / `full_bvh_gpu` | README.md L135; readiness fields | partial | `engram-gpu`, daemon readiness, `get_backend_readiness` | GPU tests feature-gated; CI often cpu-only | Ordinary GitHub runners have no GPU; claims are **capability-gated**, not always-on |
| OptiX RT-core BVH | build scripts / ENV docs | partial | `engram-gpu` OptiX path | local OptiX builds | Not available on standard CI |
| 8 essential MCP tools / 87 registered | README.md L166; AGENT_MEMORY_CONTRACT | implemented | `mcp.rs` `tool_list()`; lean tier in `tool_tier.rs` | harness tool matrix / agent-memory | Counts can drift — re-count `tool_list()` when editing MCP surface |
| Lean wake: one-call `session_start` + handoff | README L98; docs/AGENT_MEMORY_CONTRACT | implemented | `session_start` / `wake_bundle.rs` / `session_end` | wake_bundle tests; agent-memory suite | Slim packet still large; intent-vs-primary_goal mismatch is operational friction, not a broken claim |
| Code atlas / `context_for_edit` | README L58, L164 | implemented | `store.rs` / mcp `context_for_edit` | edit fidelity / agent-tool-fidelity harness | — |
| Sheaf / `processes/*.toml` rituals | README L131; processes/ | implemented | process sheaf load at wake; `processes/` | sheaf-related server tests | “Sheaf H¹” for agent graphs is specialized; not every TOML is cohomology |
| Categorical linguistic calculus (diff/integrate/operadic) | README L171–195; CHANGELOG | partial | `engram-core/src/ops.rs` linguistic ops; MCP linguistic tools | `test_linguistic_full_p1_p5_pipeline_*` | Real ops exist; marketing “synthetic calculus” exceeds everyday agent usage |
| Hybrid wire serialization (HBRD) | CHANGELOG P0–P5 | partial | `encode.rs` `to_hybrid_wire` / `from_hybrid_wire` | hybrid encode tests if present | `from_hybrid_wire` is a **minimal stub** (does not fully restore q/p); O_DIRECT .leg remains primary |
| Homomorphic + transform attestation (historically misnamed ZK) | CHANGELOG; encode.rs | partial | `apply_homo_op`, `generate_zk_proof` (attestation), `verify_zk_proof` | encode tests | **Not** zk-SNARK. BLAKE3 cookie of dsl+crs+sig0+op. Prefer “attestation” in docs. |
| Protocol execution / process subvisor H¹ | AGENTS.md; processes/monitor | partial | subvisor / process load | process/harness tests | Governance exists; full OP_INVERT/H¹ agent-graph theory is deeper than runtime enforcement |
| Lawfulness: `verify_manifold_integrity` / `verify_block_lawfulness` | AGENT_INTEGRATION; lawfulness | implemented (seal-aware sample) / partial (full history) | `get_block_lawfulness_summary` + `verify_block_integrity`; manifold seal counts | `honest_lawfulness_integrity_tests` | Full historical Merkle walk still not present; chain_slots_nonzero is depth-present only |
| NREM / ego.leg3 long-horizon continuity | README; MANIFESTO | implemented | daemon NREM path; ego.leg3 | NREM stack / profile tests (see dogfood PR #209) | Large-stack NREM needs dedicated thread (PR #209) |
| Trust residual / mutual morning packet | PR #210; wake path | implemented | `store.rs` `build_trust_residual`; `wake_bundle`; mcp wake hoist | `trust_residual_v1_bootstrap_and_handoff`; wake_bundle tests | Merges when PR #210 greens |
| REST recall returns empty under lean | historical bug | fixed on PR #209 | `serve.rs` `recall_scoped` default `scope=all` | REST dogfood path; serve path | MCP lean anchors intentionally different |
| PRAXIS hard contract | store update | implemented (opt-in) | `ENGRAM_PRAXIS_CONTRACT=hard` | `praxis_contract_hard_tests` | Default soft for compat |
| Automatic Autophagy GC daemon | MANIFESTO (historical) | **removed** | Daemon is watcher-only; no auto-evict | — | Use `mcp_engram_forget_old` for **explicit** low-CRS eviction only |
| Pinned CRS=1.0 immortal blocks | AGENT_INTEGRATION_GUIDE | implemented | pin / praxis promotion | pin/remember_solution paths | — |
| No neural embeddings in recall path | AGENT_INTEGRATION_GUIDE | implemented | `encode.rs` BLAKE3 spiral / unit-phase | encode determinism tests | — |
| Benchmarks / large-store wake “seconds on 192k” | README L165 | partial | readiness + lean wake | harness timing (env-specific) | Hardware-dependent; not a CI guarantee |

---

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
