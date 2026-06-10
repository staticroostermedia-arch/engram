# Substrate CS Gap Closure Roadmap (Pure Technical)

**Status**: Living (primary source of truth is the manifold Thought Tile `tile:knowledge_graph_substrate-cs-gap-closure-roadmap-v1` + 6 child goals under `2026-06_complete_4item_logophysics_ego_roadmap`).  
**Date**: 2026-05  
**Scope**: Computer science implementation work only on the Engram substrate to close gaps versus the HolographicBlock / VSA / Geosphere / Sheaf / CLS specification.  
**Strict Separation**: This document and all associated artifacts contain **zero** mythological, narrative, or creative content. All language is limited to data structures, algorithms, performance characteristics, invariants, and verification methods. Creative or mythological work must occur in separate namespaces and directories (e.g., data/false_empire/).

## Invariants (Non-Negotiable for All Work)
- .leg3 binary vector isomorphism and fixed layout (phase vectors at known offsets, ZEDOS tag, CRS, BLAKE3 Merkle, payload).
- Hardware-aligned zero-copy paths: O_DIRECT, mmap, Metal/CUDA backends, no unnecessary CPU copies for hot paths.
- Allowed Transforms contract only — no arbitrary mutation of existing blocks.
- Unit hypersphere normalization after every geometric operation on phase vectors.
- CRS thermodynamic semantics (self-maintaining via confirmation/contradiction history; 0.74 gate for actionability).
- Lawfulness verification on all changes (mcp_engram_verify_manifold_integrity, mcp_engram_verify_block_lawfulness, etc.).

## Current Realizations (Verified via Spatial Audit)
- **NREM / ego.leg3 (daemon.rs)**: Configurable loop (ENGRAM_NREM_INTERVAL_MINUTES) performing OP_ADD superpose with goal bias + mass cap. Writes to `~/.engram/ego.leg3` (full HolographicBlock) with hot-swap. Shadow anchoring (OP_ADD 768-dim + L2 normalize on 8192D q). Spatial relational gluing for AST ("defines", sibling relations).
- **BVH / Search (engram-gpu bvh.rs + kernels)**: Full 8192D Complex32 handling. `hash_query`, `project_to_3d` (Murmur + GENESIS_SEED), LBVH with AABB + rope pointers, lazy OptiX RT-core pipeline (CUDA, gated), CPU fallback, SRHT+B4 TurboQuant, CRS-weighted scoring, in-memory cache. Intent is O(log N) hardware-accelerated K-NN via repurposed RT cores.
- **Phase vectors**: HolographicBlock carries q (position) and p (momentum) as `[Complex32; 8192]`. ZEDOS, CRS, Merkle, payload. GPU and storage kernels treat them as first-class.
- **Rituals / Tiles**: Existing embodiment mechanisms (traces, scars as repellers, dual Thought Tiles with momentum/NREM/ki_hijacker promotion paths, engram-goal as intentional geometry).

## Ordered Priorities (CS Dependencies + Leverage)
Ranked for maximum substrate leverage with clear parallelization opportunities.

**1. Embodiment layer hardening: ego.leg3 as persistent first-class trajectory + hot-path promotion**  
   Make `~/.engram/ego.leg3` a first-class HolographicBlock with explicit q/p, full Logenergetics (H, τ, tau), and provenance. Extend NREM to support explicit hot-set promotion of high-CRS substrate blocks. Define and expose hot-path API (`fetch_block_high_priority` / `is_hot`) that bypasses normal O_DIRECT for promoted blocks on Metal/CUDA backends.  
   **Entry points**: daemon.rs (run_nrem_consolidation, refresh_ego_q), engram-core storage + hot cache paths, MCP layer.  
   **Success criteria**: Measurable reduction in re-derivation cost across context loss; ego_q shows measurable resonance with active high-value substrate artifacts; benchmarks + lawfulness checks pass.

**2. Richer CLS training tuple emission from ritual traces (full 8 properties)**  
   Extend trace capture paths to emit ZEDOS_TRAINING blocks carrying all 8 properties: UTC + tau, Geosphere coordinate (when implemented), CRS, p momentum (or summary), Hamiltonian H, torsion τ, BLAKE3 provenance chain, productive failure / low-CRS paths. Use existing trace schema as authoritative source. Serialization must respect .leg3 layout.  
   **Entry points**: MCP trace tools, engram-core encoding for TRAINING tag, ritual skill trace capture points.  
   **Dependencies**: Phase 1 improvements.  
   **Success criteria**: Example richer TRAINING blocks queryable with all 8 fields populated and correct provenance; downstream NREM/consolidation can weight them appropriately.

**3. Live Geosphere as 5th coordinate + basic JIT frame resolution**  
   Add `active_location` as `[Complex32; 8192]` (global register or per-block). Make it participate in angular distance / K-NN calculations (combined space or principled projection). Implement basic frame resolution: combination (e.g., OP_BIND-style or projection) with a lens vector derived from an origin (e.g., Giza cubit reference) + time offset. All results must remain on the unit hypersphere. Start with query participation and simple lens application.  
   **Entry points**: engram-core types + storage, daemon SymplecticState, GPU kernels for combined distance, MCP query paths.  
   **Dependencies**: Stable phase vector representation and normalization.  
   **Success criteria**: Queries with explicit Geosphere lens return geometrically distinct, reproducible results vs baseline; no violations of invariants.

**4. Executable VSA calculus runtime + ZEDOS_OPERATOR exposure**  
   Implement the missing operations as first-class executable code + ZEDOS_OPERATOR blocks: OP_GEOMETRIC_PRODUCT (Clifford dot in real + wedge in imag on complex), full OP_IS_SYMBOLIC_OF (H¹ cohomological obstruction detection + toroidal dual-phase lift via ZADO-CPS), OP_DEDUCE (rotation via conjugate), OP_DYNAMIS (π/4 phase), OP_SUSPEND (Apeiron maximum-entropy bind). All ops must preserve unit hypersphere. Expose via internal APIs and MCP for application to arbitrary blocks/traces/relations.  
   **Entry points**: engram-gpu kernels (complex arithmetic extensions), engram-core ops layer, MCP for operator application and ZEDOS_OPERATOR minting.  
   **Dependencies**: Stable phase vector math.  
   **Success criteria**: Correctness tests match paper formal definitions; performance within acceptable factor of current search kernels; resulting blocks pass lawfulness verification.

**5. Sheaf auditing: H¹ Laplacian on agent manifold graph**  
   Construct graph from the existing rich relation structure (tiles, traces, blocks, goals, spatial AST relations). Implement Laplacian operator + spectrum analysis to detect non-trivial H¹ elements (logical cycles / cohomological obstructions). Provide MCP tool and internal API for audit on arbitrary subgraphs (e.g., around a goal or workstream). Support repair suggestions consistent with existing CRS restriction maps and OP_IS_SYMBOLIC_OF promotion semantics.  
   **Entry points**: Current relation storage + visualize infrastructure, new audit module (engram-core or server).  
   **Dependencies**: Mature relation graph (already strong from spatial + ritual work).  
   **Success criteria**: Correctly surfaces injected cycles in test manifolds; integrates cleanly with CRS and restriction map behavior.

**6. Symplectic integration at 432 Hz on active q/p**  
   Implement a live integrator for active / hot-promoted blocks: 432 Hz (2⁴ × 3³) Symplectic method (Verlet-style or appropriate manifold RK4 on (S¹)^8192) that evolves q and p while strictly conserving the Hamiltonian (kinetic + potential). Store H, τ (torsion ∇×p), and tau (emergent cycle count) as first-class Logenergetics fields. Provide hooks for NREM, query scoring, and hot-path logic to consume the new fields. Numerical energy drift must be negligible (within floating-point expectations over long runs). Gate to active/hot blocks only.  
   **Entry points**: engram-gpu kernels (new symplectic step), daemon integration (extend NREM or dedicated background task), engram-core Logenergetics.  
   **Dependencies**: Stable phase vectors + core VSA ops (Phases 3/4).  
   **Success criteria**: Long-running tests demonstrate Hamiltonian conservation within tolerance; no semantic drift attributable to numerical artifact; new fields usable in scoring and consolidation without violating invariants.

## Parallelization & Subagent Tasking
Workstreams 1 and 2 have low mutual dependency and can start immediately.  
Workstream 3 has light dependency on Phase 1 storage stability.  
Workstreams 4 and 5 can proceed in parallel with moderate coordination on the relation graph and op kernels.  
Workstream 6 has the strongest dependency on stable phase vectors and ops (4).  

Each workstream is sized for one or more subagents:
- Kernel / low-level math specialist
- MCP surface + API specialist
- Testing / lawfulness / benchmark specialist
- Integration / daemon specialist

Spawn prompts can be derived directly from the CS spec + entry points + success criteria above. All subagents must operate under engram-working-memory discipline (recall-first via MCP spatial/relation/momentum, record_reasoning_trace with goal_context, immediate scar on invariant violations, spatial Code Edit Ritual for any source changes).

## Living Artifacts
- Thought Tile: `tile:knowledge_graph_substrate-cs-gap-closure-roadmap-v1` (this document's primary source)
- 6 child goals (exact names emitted at creation time under the parent ego roadmap goal)
- This .md (neutral technical reference)
- Rich planning traces (pure CS language, tied to invariants and existing realizations)
- Process Architecture Sheaf (executed per plan): processes/ with 7 .toml (ritual wake-up/nrem, harness spatial-recon, operator momentum-query, process session-end, monitor manifold-health/subvisor), dynamic loader stub+call in mcp.rs (session_start), registered process:engram.* blocks with category/gluing (serves primary), subvisor draft for agent oversight. Artifacts: tile:formal_spec_process-architecture-sheaf-v0-1..., goal:1780374670..., design:process_* blocks, traces. Aligns to Phase 4 VSA (OP_*), Phase 5 H1 (subvisor/monitor), ritual realizations. See exec-plan-overview todos for phases/troops/validation.

## Verification & Handoff
All deliverables must pass:
- mcp_engram_verify_manifold_integrity
- mcp_engram_verify_block_lawfulness (where applicable)
- Relevant unit / property tests + micro-benchmarks on phase vector operations
- No regression on existing hot paths or .leg3 compatibility

This roadmap is the executable technical plan. It can be decomposed and assigned to arbitrarily many subagents while preserving strict separation from any creative or mythological layer.

The substrate must remain lawful, isomorphic, and hardware-aligned at every step.