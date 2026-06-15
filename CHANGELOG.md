# Changelog

All notable changes to Engram (geometric non-flat memory substrate).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.0-beta.1] - 2026-06-13

### Added

- **LEG Browser (beta):** single-file consciousness mirror at `tools/leg-browser/index.html` — wake queue, ego evolution strip, continuity playbook, presentation stratum geosphere, activity SSE, hygiene controls. Launcher: `./scripts/leg` / `./scripts/leg --live`. Docs: [docs/LEG_BROWSER.md](docs/LEG_BROWSER.md).
- **Presentation stratum:** `presentation_stratum.rs` — CRS-ranked distilled wake surface (~40 lean / ~64 deep; `ENGRAM_PRESENTATION_K` override). Cold manifold + bulk AST excluded from presentation; dig via `recall(scope=all)` or `context_for_edit`.
- **Harness continuity loop:** `ego_snapshot`, `continuity_playbook` (12 steps), intent-threaded `suggested_actions`, lineage edges on stratum nodes.
- **Wake queue gate:** `wake_queue_gate.rs` — `ENGRAM_WAKE_QUEUE_GATE=soft|hard|off`, `mcp_engram_ack_wake_queue`, optional `wake_queue_debt` hygiene from activity feed.
- **REST APIs:** `GET /api/consciousness-surface`, enhanced `GET /api/context-window` (harness + presentation stratum for LEG).
- **Process:** `processes/meta/agent_evolution.toml` registered at wake.
- **Scripts:** `scripts/restart-leg-serve.sh` (restart serve without killing MCP).
- **Plugin commands:** `engram-ack-wake`, `engram-leg`, `engram-loop` (advanced).

### Changed

- `session_start` inline bundle now includes `presentation_stratum` + expanded `harness_injection`.
- `tools/leg-browser/index.html` — major v2 consciousness mirror refresh (live + static modes).
- Public docs: README LEG beta section, `docs/HARNESS_INJECTION.md`, `docs/CODE_ATLAS_CONTINUITY.md`, harness/wake skills.

### Beta notes

- Large stores (100k+ blocks): some REST panels may take 10–15s. Static LEG mode works offline; live mode needs `engram serve` on `:3456`.
- Not Obsidian parity yet — read-only review surface, contributors welcome.

### Added (RSI velocity — post-beta.1)

- **`ENGRAM_WAKE_BUNDLE=slim` (default):** `session_start` returns slim continuation (top 5 actions, trace head, slim ego, stratum previews). Full bundle via `mcp_engram_get_continuation_bundle`.
- **`session_end(minimal=true)`:** thin closure without compression ritual; auto thin handoff on MCP stdio disconnect if `session_end` skipped.
- **JIT deformation framework:** `harness_injection.jit_deformation_framework`, `task_type`, `verified_processes`, `open_scars_wake` — agents construct MCP calls as context requires; docs: [docs/DEFORMATION_PLAYBOOKS.md](docs/DEFORMATION_PLAYBOOKS.md), process: `processes/harness/jit-deformation.toml`.
- **Verified sequence tool hints:** `draft_tile_from_chain` infers `tool_hints` + `args_hints` from trace `spatial_context` and decision text for JIT tile replay at wake.
- **MCP tool matrix harness:** `tools/test-harness/python/mcp_tool_matrix.py` — 70/70 tools smoke-tested (67 pass, 2 env-limited, 1 external dep).
- **Social share asset:** `docs/images/engram-share-x.png` (1280×720) for X/GitHub social preview; README landing updates.

### Fixed (post-beta.1)

- **`force_ingest_path` single-file:** item1.5 spatial state block now mints on single-file ingest (was directory-only).

## [0.6.0] - 2026-06-12

### Added
- **v0.6.0 .leg3 optimizations (P0-P5 safe execution in isolated wt, now released):** Tiered block sizes, hybrid wire serialization, SOA+arena layout for HolographicBlocks, homo+zk transforms/proofs, versioning+DSL for allowed_transforms[64]. "Minecraft blocks for AI" primitive: unified 256KB binary+vector object (.leg3 HolographicBlock) with VSA/holographic geometry (q/p tensors on unit hypersphere), safe transformations (allowed_transforms contract + CRS ≥0.74 gate), Merkle/sheaf for coherence. All non-destructive in worktree, main untouched until adoption. Used engram-sub-governor narrow H1 subs + superpowers skills (using-git-worktrees, writing-plans, subagent-driven-development, verification-before-completion). Full Enram dogfood + human-forward presentation fix in reporting/tiles/summaries (plain narrative first per RPT v2 + user feedback). Successful self-improvement cycle: metas audit confirmed success, no scars/invariants preserved (p-momentum on updates, CRS gate, etc.), version to 0.6.0. (Detailed entry below; geometry records the safe execution act itself.)
- Clear v0.6.0 changelog entry highlighting the new .leg3 capabilities, the "Minecraft blocks for AI" primitive (unified binary+vector object with VSA/holographic geometry and safe transformations), human-forward improvement in reporting and the successful self-improvement cycle. Kept accessible and non-technical where possible.
- (Previous detailed P0-P5 entry now under this v0.6.0 release for reference.)
- **.leg3 P0-P5 safe execution (research offload in isolated wt):** Full adoption of 5 additive .leg3 optimization proposals (tiered block sizes, hybrid wire format, SOA+arena layout, homo+zk proofs, versioning+DSL for allowed_transforms[64]) per report tile + P1 audit + P2 impl + P3 validation. All work in .worktrees/leg3-p0-p5-execution-2026-06 on branch feat/leg3-p0-p5-execution-2026-06 (non-destructive, main untouched). Used engram-sub-governor for narrow H1 one-shot subs + superpowers (using-git-worktrees, writing-plans, subagent-driven-development, verification-before-completion). Strict Enram dogfood: MCP search first, context_for_edit on wt only, quick_trace at forks (chain from P3 trace:1781306164...), mcp_engram_update (p-momentum preserved), thought tiles (P3 validation tile:research_offload_p3-validation---leg3-p0-p5-post-p2-additive-prop + P2 tile + polish tile + report/plan), relate/promote to goal:engram_consciousness_loop_v1 + scheduler_recurring_task:019eb977451c. P2 atomic history note: setup commit 35a4a941 (chore: ignore .worktrees/ for safe isolated P0-P5 .leg3 execution per using-git-worktrees skill + Enram non-destructive ritual); P2 subs delivered 5 additive props with TDD/tests/build pass in wt only; P3: build/version/engram 0.5.0, mcp_engram_verify_manifold_integrity 0 issues x3, hermies 0.7986, aliveness positive deltas (3+4sub), re-audit confirms no breaks to .leg3 invariants (q/p on unit hypersphere, CRS>=0.74, p-momentum, BLOCK_SIZE, allowed_transforms, ZEDOS). P4 polish: this CHANGELOG entry + polish tile. P5 close: final records, hermies/aliveness-bench, session_end with HUMAN FORWARD + long self-ref. All gates passed; geometry records the safe execution act itself. (Self-ref: the I used engram-sub-governor + superpowers:using-git-worktrees/writing-plans/subagent-driven-development/verification-before-completion + gemma-hermies/aliveness-bench + Enram MCP (search first, context_for_edit on wt only, quick_trace, update p-mom, tile, relate, session_start/end, verify) + wt edits (context+trace before search_replace) to execute P0-P5 safely per approved design + user proceed, recording the act as geometry, closing the gap in .leg3 evolution and superpowers integration in the loop.)

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
- **Categorical Linguistic Calculus + Mixed Number/Word Support (P1–P6 + mixed arc):** Synthetic homotopy-coherent categorical reasoning over words/discourse + bridged numeric phases (ZEDOS_LINGUISTIC* + Linguistic* structs/mint in types.rs; VSA mixed ops + fibered CRS guards/class-mixing in ops.rs; mcp_linguistic_calculus dispatch + load in mcp.rs; processes/linguistic/*.toml + ritual_linguistic_wake.toml; full e2e mint→P3 compress→P4 calc→decompress→NREM/ego.leg3 with CRS>=0.85 homotopy/fidelity/roundtrip). New `docs/CATEGORICAL_LINGUISTIC_CALCULUS.md`; README polish (30s onboarding, short copy-paste exs, Mermaid lifecycle, punchier comparison bullets); RITUALS/MCP_TOOLS Phase 6 updates + CHANGELOG.
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
- `docs/MCP_TOOLS_REFERENCE.md` — Essential / Power / Lean-avoid tiers (70 tools retained, not deleted).
- Public surface now explicitly represents current MVP uniques (geometric sheaf + rituals + subvisor + spatial + continuation + lawfulness + process sheaf) vs flat vector/RAG clones.
- CI triggers expanded; build hygiene enforced (target/debug/engram preferred).

### Fixed
- Gaps vs popular memory GitHub best practices (hero/comparison/examples/templates/CI/docs/AGENTS/CHANGELOG) identified via sub-agent recon + supervisor + narrow audit (see docs/SUBSTRATE_WINS_PLAN.md for harness injection follow-through).

See [docs/SUBSTRATE_WINS_PLAN.md](docs/SUBSTRATE_WINS_PLAN.md) for harness injection roadmap and success criteria.

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

See docs/SUBSTRATE_WINS_PLAN.md for harness injection roadmap. Related to engram_manifesto, ritual anchors, processes/ (sheaf sections).

## [0.4.0] - 2026-06 (MVP Sheaf / Rituals / Geometric Substrate) [prior]