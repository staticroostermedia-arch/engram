# Changelog

All notable changes to Engram (geometric non-flat memory substrate).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0-beta.4] - 2026-06-25

### Added

- **Relational lean v2 (PR #42):** Graph-walk default recall path; write breadcrumbs (`auto_relate_after_write`); `cufile` hot-path gate; turn-extract sidecar wiring.
- **Lean gaps closure (PR #43):** `HttpLlmExtractor` for `turn_record` when `ENGRAM_TURN_LLM_EXTRACT=1` + reachable `ENGRAM_LLM_URL` (heuristic fallback preserved; `**extraction:** llm` marker in episodic blocks).
- **`resolve_active_or_recent_goal`:** Shared resolver — `remember` / `turn_record` auto-relate via `recent_fallback` when `primary_goal` is unset after goal completion.
- **cuFile DMA path:** `cufile_direct_read_to_device` (O_DIRECT + `cuFileRead` to GPU); `cufile_transfer_path` in `get_backend_readiness`; honest `unavailable` fallback when `nvidia_fs` kernel module absent.

### Changed

- **Agent profile:** `ENGRAM_CUFILE_HOT=1` on NVIDIA rigs (DMA when driver present; H2D memcpy fallback otherwise).

### Fixed

- **cuFile tests:** O_DIRECT fd lifecycle; parallel-test env serialization for `ENGRAM_CUFILE_HOT`.
- **Lean gaps MCP verification:** 32MB-stack test harness; post-clear auto-relate test ordering.

## [0.7.0-beta.3] - 2026-06-20

### Added

- **Code atlas continuity v2:** Situated edit memory — agents inherit codebase evolution at the locus without full-repo context. See [docs/CODE_ATLAS_CONTINUITY.md](docs/CODE_ATLAS_CONTINUITY.md).
- **Atlas v2.1:** `traces_at_locus_tiers` (exact line / file / stem tiers) in `context_for_edit`; `spatial_context` normalization on `quick_trace`.
- **`mcp_engram_evolution_at_locus`:** Bounded evolution bundle (loci, arc segments, trace chain, scars) at a file window; auto-ingest when loci empty.
- **`mcp_engram_ack_edit_arc`:** Clear edit-arc debt when `ENGRAM_EDIT_ARC_GATE=hard`; prefer `update` on `__arc` after edits.
- **`mcp_engram_ingest_reference_frame`:** Mint `formal_spec:linguistic_reference_frame_v1` + genesis pillar blocks (CodeLand / etymology grounding); relates to patent container when present.
- **Harness enforcement (WS1):** `post_edit_palette` with concrete `mcp_engram_update` args in per-file harness; `edit_arc_debt` in atlas JSON.
- **Update coherence (WS4):** `ENGRAM_UPDATE_COHERENCE=warn` (agent profile default) — provlog splice coherence check on `update`.
- **LEG evolution panel:** `GET /api/code-atlas?evolution=1` + timeline in `tools/leg-browser/index.html`.
- **Plugin commands:** `engram-ack-wake` (existing), `engram-evolution`, `engram-ack-edit-arc`.

### Changed

- **`ENGRAM_WAKE_QUEUE_GATE=hard`** is the default when `ENGRAM_PROFILE=agent` — `mcp_engram_ack_wake_queue` required before `context_for_edit` (empty queue auto-acks at wake).
- **Large-store responsiveness:** Bounded NREM candidate scan (`nrem_candidate_concepts`); batched relation flush during AST ingest; shared `spatial_items_at_file` with stem-prefix scan — `read_concept`, `evolution_at_locus`, and wake return in seconds on ~192k-block stores (was minutes).
- **MCP tool count:** 79 registered (75 `mcp_engram_*` + 4 linguistic); lean default remains **8 essential**.

### Fixed

- **`evolution_at_locus` empty loci** on large stores — auto-ingest + bounded spatial resolution.
- **MCP hangs on `update` / `read_concept`** — eliminated full-store `list()` in NREM hot path and per-relate relation-index flush during bulk ingest.
- **CI:** Clippy `-D warnings`, `cargo fmt`, parallel-safe coherence env tests.

## [0.7.0-beta.2] - 2026-06-15

### Added

- **`ENGRAM_WAKE_BUNDLE=slim` (default):** `session_start` returns slim continuation (top 5 actions, trace head, slim ego, stratum previews). Full bundle via `mcp_engram_get_continuation_bundle`.
- **`session_end(minimal=true)`:** thin closure without compression ritual; auto thin handoff on MCP stdio disconnect if `session_end` skipped.
- **JIT deformation framework:** `harness_injection.jit_deformation_framework`, `task_type`, `verified_processes`, `open_scars_wake` — docs: [docs/DEFORMATION_PLAYBOOKS.md](docs/DEFORMATION_PLAYBOOKS.md), process: `processes/harness/jit-deformation.toml`.
- **Verified sequence tool hints:** `draft_tile_from_chain` infers `tool_hints` + `args_hints` from trace `spatial_context` and decision text.
- **MCP tool matrix harness:** `tools/test-harness/python/mcp_tool_matrix.py` — 70/70 tools smoke-tested (67 pass, 2 env-limited, 1 external dep).
- **Personal knowledge wiki:** [docs/PERSONAL_KNOWLEDGE_WIKI.md](docs/PERSONAL_KNOWLEDGE_WIKI.md), starter skill, cookbook.
- **Public docs polish:** `docs/internal/` maintainer journals, cold-start human→agent fork (README, FIRST_RUN, AGENT_MEMORY_CONTRACT), external-reader tone pass.
- **Social share asset:** `docs/images/engram-share-x.png` (1280×720) for X and GitHub social preview.

### Fixed

- **`force_ingest_path` single-file:** item1.5 spatial state block now mints on single-file ingest (was directory-only).
- **MANIFESTO tool count** aligned to 70 registered / 8 essential.

## [0.7.0-beta.1] - 2026-06-13

### Added

- **LEG Browser (beta):** single-file memory review UI at `tools/leg-browser/index.html` — wake queue, ego evolution strip, continuity playbook, presentation stratum geosphere, activity SSE, hygiene controls. Launcher: `./scripts/leg` / `./scripts/leg --live`. Docs: [docs/LEG_BROWSER.md](docs/LEG_BROWSER.md).
- **Presentation stratum:** `presentation_stratum.rs` — CRS-ranked distilled wake surface (~40 lean / ~64 deep; `ENGRAM_PRESENTATION_K` override). Cold manifold + bulk AST excluded from presentation; dig via `recall(scope=all)` or `context_for_edit`.
- **Harness continuity loop:** `ego_snapshot`, `continuity_playbook` (12 steps), intent-threaded `suggested_actions`, lineage edges on stratum nodes.
- **Wake queue gate:** `wake_queue_gate.rs` — `ENGRAM_WAKE_QUEUE_GATE=soft|hard|off`, `mcp_engram_ack_wake_queue`, optional `wake_queue_debt` hygiene from activity feed.
- **REST APIs:** `GET /api/consciousness-surface`, enhanced `GET /api/context-window` (harness + presentation stratum for LEG).
- **Process:** `processes/meta/agent_evolution.toml` registered at wake.
- **Scripts:** `scripts/restart-leg-serve.sh` (restart serve without killing MCP).
- **Plugin commands:** `engram-ack-wake`, `engram-leg`, `engram-loop` (advanced).

### Changed

- `session_start` inline bundle now includes `presentation_stratum` + expanded `harness_injection`.
- `tools/leg-browser/index.html` — major v2 memory review UI refresh (live + static modes).
- Public docs: README LEG beta section, `docs/HARNESS_INJECTION.md`, `docs/CODE_ATLAS_CONTINUITY.md`, harness/wake skills.

### Beta notes

- Large stores (100k+ blocks): some REST panels may take 10–15s. Static LEG mode works offline; live mode needs `engram serve` on `:3456`.
- Not Obsidian parity yet — read-only review surface, contributors welcome.

## [0.6.0] - 2026-06-12

### Added

- **.leg3 HolographicBlock optimizations (P0–P5):** tiered block sizes, hybrid wire serialization, SOA+arena layout, homotopy + zk transforms/proofs, versioning + DSL for `allowed_transforms[64]`.
- **Unified holographic primitive:** 256KB `.leg3` HolographicBlock combining binary payload and vector geometry (q/p tensors on unit hypersphere), safe transformations (`allowed_transforms` contract + CRS ≥ 0.74 gate), and Merkle/sheaf coherence checks.

### Changed

- **Human-forward reporting:** tiles, summaries, and changelog entries lead with plain narrative before technical detail.
- Version bump to 0.6.0 after full validation of additive `.leg3` changes (manifold integrity, invariants preserved).

## [0.5.0] - 2026-06-10

### Added

- **Categorical linguistic calculus (P1–P6):** native reasoning over words and discourse bundles with synthetic differentiate/integrate/operadic ops, homotopy coherence, and fibered CRS guards — inside the same geometric sheaf as numeric phases and code ASTs.
- **Mixed number/word support:** bridge linguistic coefficients and numeric phase tensors under `mixed_class_mixing_guard` (CRS ≥ 0.74; scar on violation).
- **Real agent workflow integration:** `processes/meta/*.toml` workflow fixtures, MCP tests for self-improvement loop simulation, process sheaf load + dispatch integration tests.
- **`docs/CATEGORICAL_LINGUISTIC_CALCULUS.md`:** full P1–P6 surface, beginner walkthrough, and lifecycle diagram.
- **Agent Memory MVP:** 8-tool lean contract — `docs/AGENT_MEMORY_CONTRACT.md`, `docs/GROK_BUILD_MEMORY.md`.
- **`mcp_engram_context_for_edit`**, **`mcp_engram_set_memory_mode`**, inline `session_start` bundle, structured `session_end` handoff packet.
- **`integrations/grok-build/mcp.json`** — safe MCP defaults for large stores; `scripts/engram-grok` launcher.
- **Lean perf flags:** `ENGRAM_MEMORY_MODE`, `ENGRAM_DEFER_BVH`, `ENGRAM_DEFER_WATCH_INGEST`, `mcp_lock.rs` for duplicate MCP safety.
- **Public documentation:** `docs/GEOMETRIC_MEMORY.md`, `docs/RITUALS.md`, `docs/MCP_TOOLS_REFERENCE.md` (70 tools, Essential / Power / Lean-avoid tiers).
- **Examples:** `examples/mcp_client.py`, `examples/ritual_verify.md`, `examples/spatial_geosphere_demo.py`.
- **GitHub templates & CI:** PR/issue templates, expanded `rust.yml` matrix (ubuntu/macos, clippy/fmt, MCP harness job).
- **Contributor guides:** `AGENTS.md`, `CLAUDE.md`, enhanced `SECURITY.md`, `CHANGELOG.md`.

### Changed

- **Public polish:** README hero, geometric memory model, comparison table (vs mem0/Letta/Chroma/Qdrant/RAGFlow/Milvus), copy-paste calculus examples, Mermaid memory lifecycle; `CONTRIBUTING` quick checklist.
- **Public agent path:** README, `AGENTS.md`, `FIRST_RUN.md`, `SKILLS.md`, wake-up skill, and MCP configs lead with the 8-tool lean contract (no mandatory `watch_workspace` at wake).
- **Cargo.toml metadata:** expanded description, keywords (`ai-memory`, `geometric-memory`, `mcp`, `rituals`, …), categories.
- Version bump from 0.4.0 → 0.5.0 for public sharing readiness.

### Fixed

- CI: committed missing `processes/meta/` test fixtures; clippy/fmt gates for Rust 1.96; wgpu backend selection on no-CUDA runners.
- Gaps vs common open-source memory-repo conventions (hero, comparison, examples, templates, CI, agent docs).

Harness injection program shipped — see [docs/HARNESS_INJECTION.md](docs/HARNESS_INJECTION.md) and historical [docs/SUBSTRATE_WINS_PLAN.md](docs/SUBSTRATE_WINS_PLAN.md).

## [0.4.0] - 2026-06

### Added

- **Process Architecture Sheaf:** declarative `processes/*.toml` (wake-up, NREM consolidation, spatial recon, momentum query, session-end, manifold health, subvisor), dynamic loader in `mcp.rs`, category gluing/H¹ (`OP_ADD`, `OP_GEOMETRIC_PRODUCT`, `OP_INVERT`, `OP_IS_SYMBOLIC_OF`).
- **Subvisor (monitor):** OP_INVERT + H¹ for sub-agent oversight, loop detection via tool graph, geometric enforcement, scar on repetitive patterns.
- **Rituals first-class:** `engram-wake-up` (Phases 0–5 + lawfulness metric + continuation bundle), `engram-working-memory` (Code Edit Ritual v1 pre/post + trace A/D/R), `engram-session-end` (crystallize + COMPRESS + handoff).
- **Spatial (Item 1.5):** `watch_workspace`, `context_for_file`, `recall_in_file`, `force_spatial_ingest`, AABB from tree-sitter on save.
- **MCP:** 55+ tools (memory, spatial, graph/sheaf, verify/lawfulness, goals/tiles, session/continuation, process).
- **Geometric core:** HolographicBlock `.leg3` (256KB, q 8192D, p momentum, CRS, Merkle, provlog), VSA, geosphere/symplectic — non-flat memory (momentum + relations + scar/verify/continuation vs append-log).
- **GPU backend stability:** Metal buffer pooling + timeout CPU fallback; wgpu `HotBlockCache` (paged residency), device-loss handler, `Maintain::Poll` readback.
- **Other:** goal stack as geometric, thought tiles + hot promotion, `verify_manifold_integrity` + block lawfulness + genesis, NREM/ego.leg3, continuation bundles, harness-gate, lawfulness-metrics skill.

### Changed

- GPU hot-query paths: reduced blocking and per-query allocation on Metal/wgpu backends.
- Loader polish: TOML upgrade + live relations (including working-memory anchor).
- Working-memory discipline anchored as part of wake-up continuation.

### Fixed

- Blocking waits/allocs, device loss, full RAM mirror in wgpu, unused projection pipeline in Metal.
- Wake-up clarity bottlenecks via declarative `processes/` and geometry-first paths.

See `MANIFESTO.md`, `design/process_architecture_sheaf`, and `docs/skills/` for full context.

## Earlier

See git history for pre-0.4.0 development. Primary objective: geometric sheaf memory substrate with harness continuity (against flat append-only knowledge).