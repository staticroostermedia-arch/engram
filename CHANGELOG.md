# Changelog

All notable changes to Engram (geometric non-flat memory substrate).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-06-10

### Added
- **Categorical linguistic calculus (P1–P6):** Native reasoning over words/discourse bundles with synthetic differentiate/integrate/operadic ops, homotopy coherence, and fibered CRS guards — inside the same geometric sheaf as numeric phases and code ASTs.
- **Mixed number/word support:** Bridge linguistic coefficients and numeric phase tensors under `mixed_class_mixing_guard` (CRS ≥ 0.74; scar on violation).
- **Real agent workflow integration:** `processes/meta/*.toml` workflow fixtures, MCP tests for self-improvement loop simulation, process sheaf load + dispatch integration tests.
- **`docs/CATEGORICAL_LINGUISTIC_CALCULUS.md`:** Full P1–P6 surface, beginner walkthrough, and lifecycle diagram.

### Changed
- **Public polish:** README “What is EngramGrok?” intro, copy-paste calculus examples, Mermaid memory lifecycle; CONTRIBUTING quick checklist; CI fixes for wgpu-only runners (GitHub Actions green on ubuntu).
- Version bump from 0.4.0 → 0.5.0 for public sharing readiness.

### Fixed
- CI: committed missing `processes/meta/` test fixtures; clippy/fmt gates for Rust 1.96; wgpu backend selection on no-CUDA runners.

## [0.5.0-prep] - 2026-06 (feat/mvp-github-prep branch work, now released as 0.5.0)

### Added
- **Agent Memory MVP (Phase A):** 8-tool lean contract — `docs/AGENT_MEMORY_CONTRACT.md`, `docs/GROK_BUILD_MEMORY.md`, `design/agent_memory_mvp_plan.md`.
- `mcp_engram_context_for_edit`, `mcp_engram_set_memory_mode`, inline `session_start` bundle, structured `session_end` handoff packet.
- `integrations/grok-build/mcp.json` — safe MCP defaults for large stores; `scripts/engram-grok` launcher.
- Lean perf flags: `ENGRAM_MEMORY_MODE`, `ENGRAM_DEFER_BVH`, `ENGRAM_DEFER_WATCH_INGEST`, `mcp_lock.rs` for duplicate MCP safety.
- **Categorical Linguistic Calculus + Mixed Number/Word Support (P1–P6 + mixed arc):** Synthetic homotopy-coherent categorical reasoning over words/discourse + bridged numeric phases (ZEDOS_LINGUISTIC* + Linguistic* structs/mint in types.rs; VSA mixed ops + fibered CRS guards/class-mixing in ops.rs; mcp_linguistic_calculus dispatch + load in mcp.rs; processes/linguistic/*.toml + ritual_linguistic_wake.toml; full e2e mint→P3 compress→P4 calc→decompress→NREM/ego.leg3 with CRS>=0.85 homotopy/fidelity/roundtrip). New `docs/CATEGORICAL_LINGUISTIC_CALCULUS.md`; README polish (30s onboarding, short copy-paste exs, Mermaid lifecycle, punchier comparison bullets); RITUALS/MCP_TOOLS/plan Phase 6 updates + CHANGELOG. See prior handoffs (sub IDs, CRS gates) and GITHUB_MVP_PREP_PLAN.md.
- Full GitHub MVP prep for public representation (feat/mvp-github-prep-2026-06 branch):
  - Enhanced README with geometric Memory Model section, comparison table (vs mem0/Letta/chroma/qdrant/ragflow/milvus), badges, runnable examples section, links to new docs, 55+ MCP updates, build hygiene notes.
  - New docs/: GEOMETRIC_MEMORY.md (HolographicBlock, VSA, sheaf/H¹, spatial AABB, invariants), RITUALS.md (wake/working-memory/session-end + Code Edit Ritual v1 + sub-agent governance + lawfulness), MCP_TOOLS_REFERENCE.md (categorized 55+ tools).
  - examples/: mcp_client.py (improved runnable), ritual_verify.md, spatial_geosphere_demo.py (force/context/geosphere/momentum + ritual).
  - .github/: PULL_REQUEST_TEMPLATE.md (full ritual/spatial/manifold/verify/build/current-build checklist), ISSUE_TEMPLATE/bug_report.md + feature_request.md (engram-specific checks).
  - Enhanced .github/workflows/rust.yml (matrix ubuntu/macos, clippy/fmt, feat/docs branches, mcp-harness-and-ritual job).
  - Cargo.toml metadata: expanded description (geometric/non-flat/sheaf/rituals/MCP/spatial/continuation/lawfulness/256KB), keywords (ai-memory,geometric-memory,mcp,rituals,...), categories.
  - Enhanced SECURITY.md (manifold/ritual disclosure, verify/spatial/scar/subvisor/continuation, build hygiene).
  - CHANGELOG.md (this), start of AGENTS.md/CLAUDE.md.
- All changes under full engram-working-memory + Code Edit Ritual (pre context_for_file + record_reasoning_trace, post delta trace + relate, spatial force/ingest, engram dogfood remember/relate/promote/scar to goal:1780419540...).
- Traces, progress records, praxis solutions for prep arc (see manifold for trace:17804... series).

### Changed
- **Public agent path:** README, AGENTS.md, FIRST_RUN.md, SKILLS.md, wake-up skill, `integrations/workflows/wake_up.md`, MCP configs — all lead with 8-tool lean contract (not mandatory `watch_workspace` at wake).
- `docs/MCP_TOOLS_REFERENCE.md` — Essential / Power / Lean-avoid tiers (66 tools retained, not deleted).
- Public surface now explicitly represents current MVP uniques (geometric sheaf + rituals + subvisor + spatial + continuation + lawfulness + process sheaf) vs flat vector/RAG clones.
- CI triggers expanded; build hygiene enforced (target/debug/engram preferred).

### Fixed
- Gaps vs popular memory GitHub best practices (hero/comparison/examples/templates/CI/docs/AGENTS/CHANGELOG) identified via sub-agent recon + supervisor + narrow audit (see GITHUB_MVP_PREP_PLAN.md for details, scars for sub-agent governance lessons).

See [docs/GITHUB_MVP_PREP_PLAN.md](docs/GITHUB_MVP_PREP_PLAN.md) for full execution log, sub-agent IDs, gap matrix, success criteria.

## [0.4.0] - 2026-06 (MVP Sheaf / Rituals / Geometric Substrate)

- Process Architecture Sheaf: declarative processes/*.toml (7: ritual/wake-up, nrem-consolidation, spatial-recon, momentum-query, session-end, manifold-health, monitor/subvisor), dynamic loader in mcp.rs (registers process:engram.* at session_start), category gluing/H¹ (OP_ADD/OP_GEOMETRIC_PRODUCT/OP_INVERT/OP_IS_SYMBOLIC_OF).
- Subvisor (monitor): OP_INVERT + H¹ for sub-agent oversight, loop detect via tool graph, geometric enforce, scar repetitive (governance from sub-agent doom loop scars).
- Rituals first-class: engram-wake-up (Phases 0-5 + lawfulness metric:wake_up_verification + continuation bundle), engram-working-memory (momentum/relational/spatial entry, Code Edit Ritual v1 pre/post + trace A/D/R + goal, expensive tool hygiene, scar), engram-session-end (crystallize + COMPRESS 0x10 + handoff).
- Spatial (Item 1.5): watch_workspace, context_for_file, recall_in_file, force_spatial_ingest, AABB from tree-sitter on save; item1.5_spatial_ingestion_state_engram; bootstrap notes.
- MCP: 55+ tools (memory, spatial, graph/sheaf, verify/lawfulness, goals/tiles, session/continuation, process).
- Geometric core: HolographicBlock .leg3 (256KB, q 8192D, p momentum, CRS, Merkle, provlog), VSA, geosphere/symplectic, non-flat vs flat (momentum + relations + scar/verify/continuation vs append-log).
- Other: goal stack as geometric, thought tiles + hot promotion, verify_manifold_integrity + block lawfulness + genesis, NREM/ego.leg3, continuation bundles, harness-gate, lawfulness-metrics skill.
- GitHub prep initiated on feat branch; current build hygiene (target/debug).

See prior handoff docs, MANIFESTO, design/process_architecture_sheaf, 2026-06_Substrate_CS_Gap_Closure_Roadmap.md, .grok/skills/ for full.

## Earlier
See git log and in-manifold traces (trace:* , goal:*) for pre-sheaf history. Primary objective: engram_mvp_v1 (harness continuity, Against Flat Knowledge via geometric sheaf).

## [Unreleased / 0.4.x follow-up] - GPU Backend Patches + Polish (2026-06, GPU hand-off)

### Added
- Metal backend patches: high_priority_buffers pool (RwLock<Vec<MTLBuffer>> + get_or_create/return helpers), gpu_cosine_batch now reuses buffers (no per-query new_buffer), wait_until_completed_timeout + CPU fallback on timeout (5s probe), project_pipeline wired/activated (removed dead_code allow + note).
- wgpu backend patches: HotBlockCache (paged/hot residency replacing full Vec<PackedBlock> mirror), device.on_uncaptured_error lost handler (with reinitialize note), dispatch/readback uses wgpu::Maintain::Poll (vs Wait), arch comment + cache usage updated in store/forget/query.
- Working-memory discipline explicitly anchored + activated (ritual:engram.working-memory + relations from wake_up_anchor/self + hot + traces) as part of wake-up continuation.
- Traces for all decisions (1780459617 hand-off start, 1780459702 Metal, 1780459770 wgpu, 1780459817 plan append, etc.).

### Changed
- GPU stability/perf: reduced blocking/alloc in hot query paths for Metal/wgpu (addresses last major gaps before shippable).
- Loader polish: prior toml upgrade + live relations (including to working-memory anchor); no leftover string-parse comments.
- Public polish in progress: plan.md updated with full execution log + forward (pure geo mcp_engram_query_pure, two-stage momentum leveraging momentum-query.toml [optimization] + LRU, runtime H1, docs sync, examples hello using processes/*.toml, README/AGENT_INTEGRATION_GUIDE/CHANGELOG/FIRST_RUN/skills/examples updates).
- All under engram-working-memory (pre/post spatial/context/recall/force on every target + mcp update on state + record_reasoning_trace A/D/R with contexts/prev/goal/ritual + relate to 1780419540 + promote).

### Fixed
- Blocking waits/allocs, device loss, full RAM in wgpu, unused projection in Metal.
- Wake-up clarity bottlenecks (via declarative processes/ + geometry-first paths noted).

See GITHUB_MVP_PREP_PLAN.md for detailed execution, traces, spatial hygiene, dogfood under working-memory + Code Edit Ritual. Primary goal:1780419540. Related to engram_manifesto, ritual anchors, processes/ (sheaf sections).

## [0.4.0] - 2026-06 (MVP Sheaf / Rituals / Geometric Substrate) [prior]