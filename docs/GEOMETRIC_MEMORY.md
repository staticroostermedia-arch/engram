# Geometric Memory in Engram

Engram is a **persistent geometric (non-flat) memory engine** for AI agents.

## Core Concepts

- **HolographicBlock (.leg3)**: Fixed 256KB containers (64 x 4KB NVMe pages). Contains:
  - q tensor (8192D complex phase vector for semantics)
  - p tensor (momentum for trajectory/drift)
  - CRS (Coherence-Reliability Score via Lyapunov stability)
  - BLAKE3 Merkle chain (provenance, tamper-evident)
  - AABB spatial bounds (from tree-sitter AST)
  - Provlog (verbatim source)
  - ZEDOS tag (DECLARATIVE, EPISODIC, etc.)

- **Hardware-native**: O_DIRECT bypass, GPUDirect Storage (NVMe -> GPU VRAM DMA, no CPU bounce). LBVH for O(log N) search.

- **VSA Calculus**: OP_ADD (blending), OP_BIND (role-filler, invertible, compositional).

- **Sheaf Gluing**: Relations (OP_BIND) + H¹ handlers from declarative processes/*.toml (category table: OP_ADD, OP_GEOMETRIC_PRODUCT, OP_INVERT, OP_IS_SYMBOLIC_OF). See processes/ (ritual, harness, operator, monitor, process).

- **Spatial AABB / Item 1.5**: tree-sitter AST extraction on save (watcher). `context_for_file`, `recall_in_file`, `force_spatial_ingest`. Code Edit Ritual requires pre/post recon.

- **Geosphere / Symplectic**: Phase vectors, frames, 432Hz harmonic, unit hypersphere invariants.

- **Non-flat vs Flat**: Momentum (p-tensor), relations/sheaf, CRS/scars/verify/lawfulness gates, continuation bundles, ego.leg3/NREM consolidation vs flat vector DB append-logs or text.

## Key Tools (MCP)

- Spatial: watch_workspace, context_for_file, recall_in_file, force_spatial_ingest, spatial_status.
- Momentum/Physics: query_with_momentum, set_geosphere_frame, genesis, verify_* (manifold, block_lawfulness, behavior).
- Graph/Sheaf: relate, search_by_relation, visualize.
- Rituals: session_start/end, scar, remember_solution, record_reasoning_trace.

## Invariants (Substrate CS Gap Closure Roadmap)

.leg3 isomorphism, CRS gate >=0.74, lawfulness, unit hypersphere, allowed transforms.

### Runtime guards (M1)
as_block now has 2 guards (len debug_assert + non-destructive first-4-bytes via as_bytes assert for ZEDOS/Holographic magic plausibility); open enforces BLOCK_SIZE; layout asserts in types.rs:436+ ; still requires valid writer per .leg3 contract. CRS/p-momentum preserved as no mutation here.

See also: MANIFESTO.md, [architecture.md](architecture.md), [processes/](../processes/) (declarative tomls for rituals/harness/etc.), [SUBSTRATE_WINS_PLAN.md](SUBSTRATE_WINS_PLAN.md), [memory_lifecycle.md](memory_lifecycle.md).

For external agents: BYOP (Build Your Own Perspective) on the neutral substrate.