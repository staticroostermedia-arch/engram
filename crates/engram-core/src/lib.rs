//! `engram-core` — VSA-based persistent geometric memory for AI agents.
//!
//! # The LEG Container Format
//!
//! Engram stores knowledge as **HolographicBlocks** — self-contained 256KB
//! binary containers (`.leg` files) defined by the LEG container format specification.
//!
//! - A 8192-dimensional complex phase vector (`q`) — the geometric "fingerprint"
//! - A momentum tensor (`p`) — encodes relational binding state  
//! - A Coherence-Reliability Score (`crs`) — geometric memory health [0.0, 1.0]
//! - A provenance payload — the human-readable source text (ProvLog)
//! - A BLAKE3 Merkle footer — cryptographic lineage chain
//!
//! # Operations
//!
//! - [`ops::op_add`] — Superposition: merge two memories (union)
//! - [`ops::op_bind`] — Binding: associate two concepts (role-filler encoding)
//! - [`ops::cosine_similarity`] — Geometric similarity between memories
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use engram_core::backend::{CpuBackend, VsaBackend};
//!
//! let backend = CpuBackend::new("~/.engram/manifold");
//! backend.remember("krebs_cycle", "The Krebs cycle converts acetyl-CoA to ATP").unwrap();
//!
//! let results = backend.recall("how does cellular respiration produce energy", 5);
//! for mem in results {
//!     println!("{}: {:.3}", mem.concept, mem.score);
//! }
//! ```

pub mod backend;
pub mod block_integrity;
pub mod encode;
pub mod genesis;
pub mod index;
pub mod mmap;
pub mod ops;
pub mod payload_crypto;
pub mod storage;
pub mod types;

pub use backend::{CpuBackend, SheafBackend, VsaBackend};
pub use block_integrity::{
    is_legacy_unsealed, seal_whole_block, verify_block_integrity, verify_relation_lineage,
    whole_block_digest, BlockIntegrityStatus,
};
pub use genesis::{
    AGENT_GENESIS_TEXT, KEPLER_GATE, SACRED_FREQUENCY_HZ, SACRED_PHI, SACRED_PI, SACRED_VESICA,
    SACRED_ZETA_CRITICAL,
};
pub use ops::{apply_frame, cosine_similarity, frame_combine, op_add, op_bind};
pub use types::{
    HolographicBlock,
    Leg3Pointer,
    LegFooter,
    Logenergetics,
    // WS3-A Substrate Phase 2: live Geosphere 5th coordinate runtime register
    // (SymplecticState + reserved persistence tag; no HolographicBlock layout impact)
    SymplecticState,
    BLOCK_SIZE,
    DIMENSION,
    LAW_CONSTANT,
    ZEDOS_BODY,
    // ZEDOS epistemic tags — exposed so downstream storage tools can work with the full format
    ZEDOS_DECLARATIVE,
    ZEDOS_EPISODIC,
    ZEDOS_GEOSPHERE,
    ZEDOS_HYPOTHESIS,
    // CodeLand Phase 4 NREM/ego.leg3 integration (Tier 5 subjective deltas + energy)
    ZEDOS_NREM_CENTROID,
    ZEDOS_OPERATIONAL,
    // Phase 2.2 VSA Calculus + ZEDOS_OPERATOR: explicit VSA ops as first-class tagged
    // instances (for sheaf/harmonics consumption). Tag only; layout invariant preserved.
    ZEDOS_OPERATOR,
    // External pointer support (smart refs for >256KB data, guardrail-compliant)
    ZEDOS_POINTER,
    ZEDOS_PRAXIS,
    ZEDOS_RELATION,
    ZEDOS_SYNTHESIS,
    // Phase 2 WS2-B: richer CLS TRAINING tag for NREM bias + MCP surface (child goal sub1)
    ZEDOS_TRAINING,
    ZEDOS_VERBATIM,
};

/// `Complex32` — a 32-bit complex number. The fundamental unit of the phase vector.
pub use num_complex::Complex32;
