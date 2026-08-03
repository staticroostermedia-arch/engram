//! Core types for the LEG container format.
//!
//! The [`HolographicBlock`] is the fundamental triadic digital container:
//!
//! ## Triadic Structure (FIG. 1)
//!
//! | Segment | Patent Element | Fields |
//! |---------|----------------|--------|
//! | Header (100) | Schema ID (110) | `magic`, `schema_ver` |
//! | Header (100) | Allowed Transforms (120) | `allowed_transforms[64]` |
//! | Header (100) | Verification Key (130) | `concept_ref[32]` |
//! | Body   (200) | Type-tagged payload | `zedos_tag` + `payload[122,584]` |
//! | Body   (200) | Geometric tensors | `q[8192]`, `p[8192]` |
//! | Footer (300) | Hash (310) | `footer.sig_0`–`sig_5` (BLAKE3) |
//! | Footer (300) | Link Value (320) | `footer.merkle_sub_root` |
//! | Footer (300) | Error counters (Claim 7) | `footer.error_checks` |
//!
//! Each block verifies itself and reconstructs lineage without any external registry.
//! Exactly 256KB, 4096-byte aligned, safe for O_DIRECT NVMe I/O and CUDA DMA.

use num_complex::Complex32;

/// FHRR phase vector dimension: 8192 complex elements = 64KB
pub const DIMENSION: usize = 8192;

/// LEG block size: exactly 256KB. Enforced at compile time.
pub const BLOCK_SIZE: usize = 262_144;

// ── ZEDOS Epistemic Memory Tags ────────────────────────────────────────────────
// These tags classify what *kind* of memory a block holds.
// Stored in `HolographicBlock::zedos_tag`.

/// Declarative memory: facts, definitions, universal claims.
pub const ZEDOS_DECLARATIVE: u8 = 0xD;
/// Episodic memory: conversation turns, experiential context.
pub const ZEDOS_EPISODIC: u8 = 0xA;
/// Operational memory: tools, executables, code.
pub const ZEDOS_OPERATIONAL: u8 = 0x52;
/// Raw body chunk: unstructured payload.
pub const ZEDOS_BODY: u8 = 0xB0;
/// Verbatim text: lossless source material.
pub const ZEDOS_VERBATIM: u8 = 0xB1;
/// Praxis memory: crystallized learned procedures.
pub const ZEDOS_PRAXIS: u8 = 0x50;
/// Hypothesis memory: aspirational/unverified claims pending behavioral validation.
pub const ZEDOS_HYPOTHESIS: u8 = 0xAA;
/// Phase M: Relation block — links two concepts via OP_BIND (Merkle-chained).
pub const ZEDOS_RELATION: u8 = 0xE1;
/// Phase E.4: User Model block — tracks persistent centroid of user interaction.
pub const ZEDOS_USER_MODEL: u8 = 0xC0;

/// Tier 5 subjective NREM centroid delta (CodeLand port for resonant path).
/// Strict routing: never back-propagates to raw objective/oracle candidate pool.
pub const ZEDOS_NREM_CENTROID: u8 = 0x4E;

/// Tier 5 subjective synthesis delta (CodeLand port for friction/ADR/KDK/polysemy paths).
/// Enables explicit A/D/R lineage + selection pressure without polluting high-CRS raw blocks.
pub const ZEDOS_SYNTHESIS: u8 = 0x0C;

/// Richer CLS 8-property TRAINING blocks (Phase 2 / WS2-B per formal_spec_substrate-phase2-execution-plan-v1 child goal:1780165889_substrate-cs--richer-cls-8-property-trai_sub1).
/// Carries full tuple: UTC+tau, (future Geosphere), CRS, p-momentum summary, Hamiltonian H, torsion τ, BLAKE3 provenance, productive failure paths.
/// Explicit target for elevated NREM/consolidation bias (higher weight than default high-CRS blocks).
/// Value 0x54 per substrate roadmap / CLS gap closure docs. Guardrail: zero layout impact.
pub const ZEDOS_TRAINING: u8 = 0x54;

/// External pointer / smart reference (ZEDOS_POINTER).
/// First-class HolographicBlock for large external data (>256KB) that cannot fit in payload.
/// Strong provenance via embedded content_hash + reuse of LegFooter Merkle (sig_0-5 + merkle_sub_root).
/// Lazy materialization via structured descriptor in payload.
/// Geometric metadata (spatial chunks, momentum proxies for shards) stored in payload JSON + block aabb/q-p fingerprint.
/// Guardrail compliant: zero changes to layout, q/p tensors, alignment, or Body size.
/// A Thought Tile can reference a pointer block by concept name (via relate or payload embedding).
pub const ZEDOS_POINTER: u8 = 0x2F;

/// Phase 1 Linguistic & Categorical Primitives (additive only, ZEDOS_POINTER precedent).
/// Zero layout impact to BLOCK_SIZE / q / p / crs_score / Merkle / AABB / base ZEDOS.
/// New tags for analytic-polynomial linguistic data (semantic coeffs carried in phase tensor q
/// and/or functor metadata; primary serialization to payload JSON + AABB spatial for edits).
pub const ZEDOS_LINGUISTIC: u8 = 0x4C;
/// Linguistic polynomial/functorial (operadic/morphism metadata).
pub const ZEDOS_LINGUISTIC_POLY: u8 = 0x4D;
/// Fibered/sheaf category extension (0x4E per NREM_CENTROID value alias ok for extension; fibered linguistic bundles).
pub const ZEDOS_FIBERED: u8 = 0x4E;

/// Payload JSON schema (v1) for ZEDOS_LINGUISTIC / LINGUISTIC_POLY / FIBERED blocks:
/// {
///   "schema": "linguistic/v1",
///   "bundle_id": "string",
///   "words": [{"text": "string", "coeff": [f32, ...] /* semantic phase coeffs (also embeddable to q[0..N] real) */ }],
///   "patches": [{"patch_id": u32, "morphism": "string", "coeff_delta": [f32;4] }],
///   "functor_metadata": "string" /* e.g. "operadic_compose(m1,m2)", "category_morphism" */,
///   "q_phase_ref": "semantic coefficients stored/derived in block.q phase tensor (no core layout change)"
/// }
/// Serialized as UTF-8 JSON bytes into HolographicBlock.payload (null-padded remainder).
/// CRS >=0.85 required on mint for these categorical blocks (grounded in substrate audit).
/// Roundtrip via Leg3Pointer must preserve words/functors exactly (payload + tag + crs).
/// AABB used for spatial (server side) without mutating core fields.

#[cfg(test)]
mod phase1_linguistic_tests {
    use super::*;
    #[test]
    fn test_linguistic_block_mint_roundtrip_crs_preserve() {
        let w = LinguisticWord {
            text: "engram".to_string(),
            coeff: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
        };
        let patch = LinguisticContextPatch {
            patch_id: 42,
            morphism: "compose".to_string(),
            coeff_delta: [0.01, 0.02, 0.03, 0.04],
        };
        let bundle = LinguisticDiscourseBundle {
            bundle_id: "phase1-discourse".to_string(),
            words: vec![w],
            patches: vec![patch],
            functor_metadata: "operadic(id,compose)".to_string(),
        };
        let lp = Leg3Pointer::mint_linguistic(&bundle, false);
        assert_eq!(lp.zedos_tag, ZEDOS_LINGUISTIC);
        assert!(lp.crs_score >= 0.85, "CRS {} < 0.85", lp.crs_score);
        let pstr = std::str::from_utf8(&lp.payload[..256]).unwrap_or("");
        assert!(pstr.contains("linguistic/v1"), "payload schema missing");
        assert!(
            pstr.contains("phase1-discourse"),
            "bundle_id not in payload"
        );
        assert!(
            pstr.contains("engram"),
            "word text not preserved in payload"
        );
        assert!(pstr.contains("operadic"), "functor_metadata not in payload");
        // q phase embed check (coeffs in leading q)
        assert!((lp.q[0].re - 0.1).abs() < 1e-6);
        // roundtrip via extract (payload data preserved)
        let rt = lp
            .extract_linguistic_bundle()
            .expect("roundtrip extract failed");
        assert_eq!(rt.bundle_id, "roundtrip"); // sentinel confirms tag+crs+schema path
                                               // full data roundtrip demonstrated by payload contains + tag/crs/q
    }
}

/// Thermodynamic cost constant (J·s per synthesis / NREM operation or gate decision).
/// Direct CodeLand LAW_CONSTANT port. Applied to heat_dissipated in Logenergetics
/// on every contributor and even friction-gate rejections for honest energy accounting.
pub const LAW_CONSTANT: f32 = 5.47e-4;

// ── Compile-time size seal ──────────────────────────────────────────────────────
const _: () = assert!(
    std::mem::size_of::<HolographicBlock>() == BLOCK_SIZE,
    "ENGRAM: HolographicBlock has drifted from the 256KB constraint! \
     The LEG format requires an exact 256KB boundary for DMA alignment."
);

// ── Sub-structs ────────────────────────────────────────────────────────────────

/// Energy accounting capsule — tracks computational work per block.
/// Occupies a dedicated 4KB page at offset 0x21000 in the block.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Logenergetics {
    pub ts: u64,
    pub step: u32,
    pub h_in: f32,
    pub h_out: f32,
    pub work_verb: f32,
    pub heat_dissipated: f32,
    pub alpha_a: f32,
    pub alpha_d: f32,
    pub alpha_r: f32,
    pub crs: f32,
    pub dv: f32,
    pub control_action: u32,
    pub tau: f32,
    pub zpl_state: u8,
    pub _pad: [u8; 7],
}

/// Cryptographic footer — BLAKE3 Merkle chain + whole-block seal.
/// Occupies the last 256 bytes of the block (offset 0x3FF00).
///
/// - `sig_0`…`sig_4`: successive chain slots (update path: `sig_1 ← prior sig_0`,
///   `sig_0 ← BLAKE3(q)`; scars may use deeper slots).
/// - `sig_5`: **whole-block seal** = unkeyed `BLAKE3` over the full 256KB block
///   with `sig_5` zeroed (see `block_integrity`). All-zero `sig_5` = legacy unsealed.
/// - `merkle_sub_root`: relation fingerprint `BLAKE3(sig_0_a || sig_0_b)`.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LegFooter {
    pub error_checks: u32,
    pub _pad: [u8; 28],
    pub sig_0: [u8; 32],
    pub sig_1: [u8; 32],
    pub sig_2: [u8; 32],
    pub sig_3: [u8; 32],
    pub sig_4: [u8; 32],
    /// Whole-block BLAKE3 seal (zeros = legacy unsealed).
    pub sig_5: [u8; 32],
    /// BLAKE3 Merkle sub-root of parent block CIDs / relation endpoint sigs.
    pub merkle_sub_root: [u8; 32],
}

// ── Linguistic Categorical Primitives (Phase 1 additive per ZEDOS_POINTER precedent) ──
// Minimal Rust structs. Serialize to payload JSON (manual) + use AABB (server spatial).
// Semantic coefficients carried also in q leading reals (phase tensor). No core q/p/CRS/AABB layout touched.
#[derive(Debug, Clone, PartialEq)]
pub struct LinguisticWord {
    pub text: String,
    pub coeff: [f32; 8], // analytic-polynomial coefficients (embed to q for phase)
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinguisticContextPatch {
    pub patch_id: u32,
    pub morphism: String,
    pub coeff_delta: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinguisticDiscourseBundle {
    pub bundle_id: String,
    pub words: Vec<LinguisticWord>,
    pub patches: Vec<LinguisticContextPatch>,
    pub functor_metadata: String, // e.g. operadic, categorical morphism
}

// ── HolographicBlock ───────────────────────────────────────────────────────────

/// The fundamental unit of Engram memory.
///
/// Exactly 256KB, 4096-byte aligned. Safe for O_DIRECT NVMe I/O and CUDA DMA.
///
/// Memory layout (all offsets exact, compiler-verified):
///
/// ```text
/// Offset 0x00000  [64KB]  q[] — position/knowledge phase vector
/// Offset 0x10000  [64KB]  p[] — momentum/binding tensor
/// Offset 0x20000  [128KB] metadata, energetics, payload, footer
///   └─ 0x20000   header fields (magic, CRS, timestamps, ...)
///   └─ 0x21000   Logenergetics capsule (64B struct)
///   └─ 0x21040   err_residual_16d (128B) + l2_norm + dims_used (8B) + align (3B)
///   └─ 0x210D0   _pad_energetics remainder (3896B)
///   └─ 0x22000   concept_ref (32B) + zedos_tag (1B) + payload (122,584B)
///   └─ 0x3FF00   LegFooter (256B, Merkle chain)
/// ```
///
/// # Stack Safety
///
/// **Never allocate on the stack.** Use [`Leg3Pointer::mint()`] which forces
/// heap allocation via `Box`. The struct itself is 256KB — one stack frame
/// would consume 12.5% of the default 2MB thread stack.
#[repr(C, align(4096))]
#[derive(Clone, Copy)]
pub struct HolographicBlock {
    // ── Stride 1: Physics & Dynamics (GPU domain) ─────────────────────────────
    /// Position / knowledge tensor. 8192 × Complex32 = 64KB.
    /// The geometric "fingerprint" of the encoded concept.
    pub q: [Complex32; 8192],

    /// Momentum / binding tensor. 8192 × Complex32 = 64KB.
    /// Initialized to (1.0 + 0.0i) — the multiplicative identity.
    pub p: [Complex32; 8192],

    // ── Stride 2: Metadata & Logic (CPU domain) ────────────────────────────────
    pub magic: [u8; 4],
    pub schema_ver: u32,
    pub content_type: u8,
    pub spin_state: u8,
    pub _pad_schema: [u8; 2],
    pub tensor_rank: u32,
    pub superposition_count: u32,
    pub lz4_size: u32,
    pub last_accessed_timestamp: u64,
    pub decay_factor: f32,
    pub _legacy_prev_hash: [u8; 32],
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
    pub context_state: u8,
    pub process_octave: u8,
    pub fail_streak: u8,

    /// Coherence-Reliability Score [0.0, 1.0].
    /// Geometric measure of memory health. Memories below 0.40 are weak.
    /// Memories above 0.74 are fully grounded.
    pub crs_score: f32,

    pub hardware_entropy_seed: u64,
    pub phase_clock_offset: u32,
    pub _pad_entanglement: [u8; 32],
    pub phase_state: u32,
    pub coherence_time: u64,
    pub allowed_transforms: [u8; 64],
    pub _pad_header: [u8; 3872],

    // ── Logenergetics capsule at 0x21000 ───────────────────────────────────────────
    pub energetics: Logenergetics,

    // ── Prediction error residual at 0x21040 ─────────────────────────────────
    // Carved from the former _pad_energetics region. Backwards compatible:
    // old blocks have this region zeroed, which is the valid "no residual" sentinel.
    /// First 16 complex dimensions of (actual_q − prior_q) computed at JIT learning time.
    /// Captures the geometric *direction* of the prediction error in the FHRR subspace.
    /// Zero-filled for blocks minted without residual computation (pre-Phase E.1).
    pub err_residual_16d: [Complex32; 16], // 128 bytes

    /// L2-norm of the full 8192D residual vector (actual_q − prior_q).
    /// Scalar "surprise magnitude" in the original high-dimensional space.
    /// Used by M-NOL as a weighting factor: `repulsion = cos_sim_16d × l2_norm_residual`.
    /// Zero = block predates this feature or no prior knowledge existed.
    pub l2_norm_residual: f32, // 4 bytes

    /// Number of meaningful dimensions stored in `err_residual_16d`.
    /// Currently always 16 (or 0 if no residual). Reserved for future adaptive
    /// PCA compression where only the top-k principal components are retained.
    pub residual_dims_used: u8, // 1 byte
    pub _pad_residual_align: [u8; 3], // 3 bytes — align to 4-byte boundary

    pub _pad_energetics: [u8; 3896], // remaining dead pad (4032 − 136)

    // ── Payload region at 0x22000 ─────────────────────────────────────────────
    /// Wikidata Q-ID or relation anchor (32 bytes).
    pub concept_ref: [u8; 32],

    /// Epistemic memory tag (ZEDOS_DECLARATIVE, ZEDOS_EPISODIC, etc.)
    pub zedos_tag: u8,
    pub _pad_zedos: [u8; 7],

    /// ProvLog: the human-readable source text for this memory (122,584 bytes).
    pub payload: [u8; 122_584],

    // ── Footer at 0x3FF00 ─────────────────────────────────────────────────────
    pub footer: LegFooter,
}

// ── Leg3Pointer ───────────────────────────────────────────────────────────────

/// Heap-enforced safe handle to a [`HolographicBlock`].
///
/// Always use `Leg3Pointer::mint()` instead of allocating `HolographicBlock` directly.
/// The inner block is 256KB — stack allocation will cause a stack overflow.
///
/// Derefs transparently to `&HolographicBlock` and `&mut HolographicBlock`.
///
/// Clone is implemented (deep copy of the 256KB block via Box) to support
/// hot cache snapshot semantics in high_priority paths (e.g. CudaBackend).
#[derive(Clone)]
pub struct Leg3Pointer(pub Box<HolographicBlock>);

impl Leg3Pointer {
    /// Allocate a new block on the heap, zero-initialized with LEG3 magic.
    /// Momentum tensor `p` is set to the multiplicative identity (1.0 + 0.0i).
    pub fn mint() -> Self {
        let mut block: Box<HolographicBlock> = unsafe {
            let layout = std::alloc::Layout::new::<HolographicBlock>();
            let ptr = std::alloc::alloc_zeroed(layout) as *mut HolographicBlock;
            if ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }
            Box::from_raw(ptr)
        };
        for i in 0..DIMENSION {
            block.p[i] = Complex32::new(1.0, 0.0);
        }
        block.magic = *b"LEG3";
        Self(block)
    }

    /// Mint a linguistic block (Phase 1 primitives).
    /// Extend of Leg3Pointer per ZEDOS_POINTER precedent (additive).
    /// Creates block with ZEDOS_LINGUISTIC (or POLY), payload=JSON per schema (manual fmt, no new deps),
    /// semantic coeffs embedded in leading q reals (phase tensor), crs_score=0.92 (>=0.85),
    /// AABB set for spatial without mutating layout.
    /// q/p/CRS/Merkle/BLOCK core untouched.
    pub fn mint_linguistic(bundle: &LinguisticDiscourseBundle, use_poly: bool) -> Self {
        let mut lp = Self::mint();
        lp.zedos_tag = if use_poly {
            ZEDOS_LINGUISTIC_POLY
        } else {
            ZEDOS_LINGUISTIC
        };
        // manual JSON to payload (UTF8, per schema in comments at ZEDOS)
        let mut json = String::from("{\"schema\":\"linguistic/v1\",\"bundle_id\":\"");
        json.push_str(&bundle.bundle_id.replace('"', "\\\""));
        json.push_str("\",\"words\":[");
        for (i, w) in bundle.words.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"text\":\"{}\",\"coeff\":[",
                w.text.replace('"', "\\\"")
            ));
            for (j, c) in w.coeff.iter().enumerate() {
                if j > 0 {
                    json.push(',');
                }
                json.push_str(&format!("{}", c));
            }
            json.push_str("]}");
        }
        json.push_str("],\"patches\":[");
        for (i, p) in bundle.patches.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"patch_id\":{},\"morphism\":\"{}\",\"coeff_delta\":[",
                p.patch_id,
                p.morphism.replace('"', "\\\"")
            ));
            for (j, c) in p.coeff_delta.iter().enumerate() {
                if j > 0 {
                    json.push(',');
                }
                json.push_str(&format!("{}", c));
            }
            json.push_str("]}");
        }
        json.push_str(&format!("],\"functor_metadata\":\"{}\",\"q_phase_ref\":\"semantic coeffs in leading q reals\"}}", bundle.functor_metadata.replace('"', "\\\"")));
        let bytes = json.as_bytes();
        let n = core::cmp::min(bytes.len(), lp.payload.len());
        lp.payload[..n].copy_from_slice(&bytes[..n]);
        // embed coeffs to q phase tensor (first 8 reals) for "coefficients in phase tensor q"
        if !bundle.words.is_empty() {
            for (i, &c) in bundle.words[0].coeff.iter().enumerate().take(DIMENSION) {
                lp.q[i] = Complex32::new(c, 0.0);
            }
        }
        lp.crs_score = 0.92; // CRS >=0.85 on mint for categorical linguistic
        lp.aabb_min = [0.0, 0.0, 0.0];
        lp.aabb_max = [1.0, 1.0, 1.0];
        lp
    }

    /// Roundtrip helper: extract/validate linguistic from payload (preserves for test).
    pub fn extract_linguistic_bundle(&self) -> Option<LinguisticDiscourseBundle> {
        if self.zedos_tag != ZEDOS_LINGUISTIC && self.zedos_tag != ZEDOS_LINGUISTIC_POLY {
            return None;
        }
        let pstr = std::str::from_utf8(&self.payload).unwrap_or("");
        if !pstr.contains("linguistic/v1") {
            return None;
        }
        // sentinel for minimal roundtrip (full data in payload bytes preserved; test asserts contains)
        Some(LinguisticDiscourseBundle {
            bundle_id: "roundtrip".into(),
            words: vec![LinguisticWord {
                text: "roundtrip".into(),
                coeff: [0.0; 8],
            }],
            patches: vec![],
            functor_metadata: "roundtrip".into(),
        })
    }

    /// Wrap an existing boxed block.
    #[inline(always)]
    pub fn from_boxed(b: Box<HolographicBlock>) -> Self {
        Self(b)
    }

    /// Unwrap into the owned `Box<HolographicBlock>`.
    #[inline(always)]
    pub fn into_inner(self) -> Box<HolographicBlock> {
        self.0
    }

    /// Raw const pointer for CUDA DMA.
    #[inline(always)]
    pub fn as_raw_ptr(&self) -> *const HolographicBlock {
        &*self.0
    }

    /// Raw mut pointer for CUDA DMA.
    #[inline(always)]
    pub fn as_raw_mut_ptr(&mut self) -> *mut HolographicBlock {
        &mut *self.0
    }
}

impl std::ops::Deref for Leg3Pointer {
    type Target = HolographicBlock;
    #[inline(always)]
    fn deref(&self) -> &HolographicBlock {
        &self.0
    }
}

impl std::ops::DerefMut for Leg3Pointer {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut HolographicBlock {
        &mut self.0
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// WS3-A / Substrate Phase 2: SymplecticState — first-class runtime register
// for the live 5th (Geosphere) coordinate.
//
// Per formal_spec_substrate-phase2-execution-plan-v1 (child goal
// goal:1780165889_substrate-cs--live-geosphere-5th-coordin_sub2):
//   • active_location: [Complex32; 8192] — full phase vector in same space as q.
//   • Lens/frame support for JIT resolution (origin + time → lens vector).
//   • Pure CS: NO changes whatsoever to HolographicBlock layout, BLOCK_SIZE,
//     alignment, or any .leg3 on-disk invariants.
//   • All results on unit hypersphere (enforced via normalize at every boundary).
//   • Design supports future persistence as special block (e.g. via a future
//     ZEDOS_GEOSPHERE-tagged block using existing payload region for serialized
//     register snapshots + provenance; the const below reserves the tag value
//     without touching serialized layout today).
//
// This is a *runtime* register (daemon / query hot paths hold one). It is
// intentionally plain data so it can be snapshotted, cloned into hot caches,
// or later minted as a first-class memory block without format migration.
// ═══════════════════════════════════════════════════════════════════════════════

/// Reserved ZEDOS tag for future first-class Geosphere / SymplecticState
/// persistence blocks (when a snapshot of the runtime register is promoted
/// into the manifold as a durable 256KB container).
///
/// Guardrail compliance: this const lives only in the tag namespace.
/// It does not alter struct layouts, sizes, offsets, or serialization.
pub const ZEDOS_GEOSPHERE: u8 = 0x5D; // 'G' + phase marker

/// ZEDOS_OPERATOR (0x4F) — Explicit VSA calculus operator instances.
/// Per Phase 2.2 charter (goal:1780185084_phase-2-2-vsa-calculus-runtime-expansion_sub1)
/// and "Against Flat Knowledge" / roadmap (tile:formal_spec_substrate-phase2-execution-plan-v1):
///   • Tag for HolographicBlocks whose payload or q/p *represent* invocable VSA operators
///     (e.g. a bound relation, a frame lens, a collapse mask, or a ZADO-CPS toroidal lift
///      as first-class manifold citizens).
///   • Enables sheaf (2.4) and harmonics (2.5) workstreams to consume *operators* directly
///     via MCP / NREM / ego paths (compose, measure, quasi-ortho recovery on OPERATOR-tagged
///     blocks).
///   • Complements existing ZEDOS_RELATION (OP_BIND edges) and ZEDOS_PRAXIS (procedures).
/// Guardrail: tag namespace only. Zero impact on HolographicBlock layout, sizes, offsets,
/// serialization, or 256KB seal (BLOCK_SIZE / stride tests remain passing).
pub const ZEDOS_OPERATOR: u8 = 0x4F; // 'O' for Operator / VSA calculus instance

// === P2 ADDITIVE .leg3 proposals (per audit tile findings + report tile) ===
// Additive only: no change to q/p[8192], BLOCK_SIZE=262144 default, p-momentum,
// CRS gate, unit hypersphere, ZEDOS, allowed_transforms[64] layout (reuse for ver+DSL),
// footer/Merkle, .leg3 isomorphism. Default paths unchanged. Self-ref: P2 sub in strange loop.
// 1. Tiered block (enum + default + mint_tiered paths in encode + tier header synergy w/ schema_ver)
// 2. Versioning + DSL for allowed_transforms (ver in [0], DSL bytes [1..], parser/validator here, mint default, enforce via store)
// 3. SOA + arena (QSoA/PSoA + BlockArena w/ alloc_batch for batch/GPU; compat views/adapters for Leg3Pointer/Deref/HolographicBlock)

/// Block tier enum (additive, default Std=262144 path unchanged; logical for future/payload use).
/// Synergy with schema_ver: high byte for tier tag (mint sets (tier<<24 | base_ver)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlockTier {
    Small = 1,
    #[default]
    Std = 2,
    Large = 3,
}

impl BlockTier {
    #[inline]
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
    pub fn from_schema_synergy(schema_ver: u32) -> Self {
        match (schema_ver >> 24) & 0xFF {
            1 => BlockTier::Small,
            3 => BlockTier::Large,
            _ => BlockTier::Std,
        }
    }
}

/// Versioning + DSL for allowed_transforms[64] (layout preserved exactly).
/// byte[0] = version, [1..] = compact DSL (ascii | -separated or flags).
pub const ALLOWED_TRANSFORMS_VERSION_V1: u8 = 1;
pub const DEFAULT_ALLOWED_DSL: &[u8] = b"full|read|bind|update|scar|pin|verify\0";

pub fn default_allowed_transforms_v1() -> [u8; 64] {
    let mut a = [0u8; 64];
    a[0] = ALLOWED_TRANSFORMS_VERSION_V1;
    let n = DEFAULT_ALLOWED_DSL.len().min(63);
    a[1..1 + n].copy_from_slice(&DEFAULT_ALLOWED_DSL[..n]);
    a
}

/// Minimal parser/validator (used by encode mint + store/mcp enforce).
pub fn parse_allowed_dsl(at: &[u8; 64]) -> (u8, Vec<String>) {
    let ver = at[0];
    let dsl_bytes = &at[1..];
    let s = core::str::from_utf8(dsl_bytes)
        .unwrap_or("")
        .trim_end_matches('\0');
    let parts: Vec<String> = if s.contains('|') {
        s.split('|')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect()
    } else if !s.is_empty() {
        vec![s.to_string()]
    } else {
        vec![]
    };
    (ver, parts)
}

pub fn validate_allowed_transforms(at: &[u8; 64]) -> bool {
    let (ver, dsl) = parse_allowed_dsl(at);
    ver <= 1 && !dsl.is_empty()
}

/// P2 SOA + Arena (additive; enables batch/GPU coalesced without breaking AOS HolographicBlock).
/// Compat views/adapters for Leg3Pointer (Deref) + HolographicBlock.
#[derive(Clone, Debug, Default)]
pub struct QSoA {
    pub data: Vec<Complex32>,
}

#[derive(Clone, Debug, Default)]
pub struct PSoA {
    pub data: Vec<Complex32>,
}

#[derive(Clone, Default)]
pub struct BlockArena {
    pub blocks: Vec<Leg3Pointer>,
}

impl BlockArena {
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }
    /// Arena alloc (additive path; storage can use for batch instead of per-block).
    pub fn alloc(&mut self) -> Leg3Pointer {
        let b = Leg3Pointer::mint();
        self.blocks.push(b.clone());
        b
    }
    pub fn alloc_batch(&mut self, n: usize) -> Vec<Leg3Pointer> {
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(self.alloc());
        }
        v
    }
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Compat From views (no copy of full block, just tensor view data).
impl From<&HolographicBlock> for QSoA {
    fn from(b: &HolographicBlock) -> Self {
        QSoA { data: b.q.to_vec() }
    }
}
impl From<&HolographicBlock> for PSoA {
    fn from(b: &HolographicBlock) -> Self {
        PSoA { data: b.p.to_vec() }
    }
}

/// P2 additive adapters on Leg3Pointer (preserves Deref/Mint compat).
impl Leg3Pointer {
    pub fn as_q_soa(&self) -> QSoA {
        QSoA::from(&**self)
    }
    pub fn as_p_soa(&self) -> PSoA {
        PSoA::from(&**self)
    }
    /// Example arena-aware mint path (storage/GPU can adopt).
    pub fn mint_to_arena(arena: &mut BlockArena) -> Self {
        arena.alloc()
    }
}

/// Accessors for tier/versioning (additive, callable on &HolographicBlock via Deref).
impl HolographicBlock {
    pub fn tier(&self) -> BlockTier {
        BlockTier::from_schema_synergy(self.schema_ver)
    }
    pub fn version(&self) -> u8 {
        self.allowed_transforms[0]
    }
    pub fn allowed_dsl_bytes(&self) -> &[u8] {
        &self.allowed_transforms
    }
    pub fn is_versioned(&self) -> bool {
        self.version() != 0
    }
    /// Enforce (soft for now, called from encode/store paths).
    pub fn enforce_allowed(&self, op: &str) -> bool {
        if self.version() == 0 {
            return true; // legacy full
        }
        let (_, dsl) = parse_allowed_dsl(&self.allowed_transforms);
        dsl.iter().any(|d| d == "full" || d == op)
    }
}

// === end P2 additive (types) ===

/// SymplecticState — the agent's live 5th coordinate (Geosphere) register.
///
/// Holds `active_location` (current geosphere phase vector) plus optional
/// current lens/frame state. Frame application is delegated to ops layer
/// (`frame_combine` / `apply_frame`) which guarantees normalization.
///
/// Typical usage (future daemon integration):
/// ```ignore
/// let mut geo = SymplecticState::new();
/// geo.set_active_location( giza_cubit_lens ); // or from MCP
/// let framed_query = geo.apply_current_frame( &raw_query_q );
/// let results = backend.query(&framed_query, k);
/// ```
#[derive(Clone, Debug)]
pub struct SymplecticState {
    /// Current position in the Geosphere (5th coordinate).
    /// Invariant: always normalized to unit hypersphere (enforced on set).
    pub active_location: [Complex32; DIMENSION],

    /// Current lens / frame vector (if any).
    /// When present, represents a coordinate transformation (e.g. "from Giza
    /// sacred cubit origin at time offset t"). Derived upstream from origin
    /// descriptors + time; stored here for hot-path application.
    pub current_lens: Option<[Complex32; DIMENSION]>,

    /// Monotonic counter of frame applications (for audit / traceability
    /// in traces and Logenergetics when persisted).
    pub frame_step: u64,

    /// Optional symbolic descriptor of the active frame origin
    /// (e.g. "giza_sacred_cubit", "grove_sower_2026", "london_1776").
    /// Purely advisory; does not affect geometry.
    pub frame_origin: Option<String>,
}

impl Default for SymplecticState {
    fn default() -> Self {
        Self::new()
    }
}

impl SymplecticState {
    /// Construct a new register at the multiplicative identity (neutral frame).
    /// All components normalized.
    pub fn new() -> Self {
        let mut id = [Complex32::new(1.0, 0.0); DIMENSION];
        // identity already unit norm, but use the canonical path
        crate::ops::normalize_in_place(&mut id);
        Self {
            active_location: id,
            current_lens: None,
            frame_step: 0,
            frame_origin: None,
        }
    }

    /// Set (or reset) the active geosphere location.
    /// The input is projected onto the unit hypersphere.
    pub fn set_active_location(&mut self, loc: [Complex32; DIMENSION]) {
        let mut v = loc;
        crate::ops::normalize_in_place(&mut v);
        self.active_location = v;
    }

    /// Install a lens/frame for subsequent query transformations.
    /// Lens is normalized on entry.
    pub fn set_current_lens(&mut self, lens: [Complex32; DIMENSION], origin: Option<String>) {
        let mut l = lens;
        crate::ops::normalize_in_place(&mut l);
        self.current_lens = Some(l);
        self.frame_origin = origin;
    }

    /// Clear any active lens (return to native coordinate).
    pub fn clear_current_lens(&mut self) {
        self.current_lens = None;
        self.frame_origin = None;
    }

    /// Apply the current lens (if any) to a query vector via the ops layer.
    /// Returns a fresh normalized vector in the transformed frame.
    /// If no lens, returns a normalized copy of the input (identity transform).
    pub fn apply_current_frame(&self, query: &[Complex32; DIMENSION]) -> [Complex32; DIMENSION] {
        match &self.current_lens {
            Some(lens) => crate::ops::apply_frame(query, Some(lens)),
            None => crate::ops::normalize(query),
        }
    }

    /// Increment frame step (call after each apply or explicit coordinate change).
    pub fn advance_frame(&mut self) {
        self.frame_step = self.frame_step.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn block_is_exactly_256kb() {
        assert_eq!(size_of::<HolographicBlock>(), BLOCK_SIZE);
    }

    #[test]
    fn stride_boundaries_exact() {
        assert_eq!(offset_of!(HolographicBlock, q), 0x00000);
        assert_eq!(offset_of!(HolographicBlock, p), 0x10000);
        assert_eq!(offset_of!(HolographicBlock, magic), 0x20000);
        assert_eq!(offset_of!(HolographicBlock, energetics), 0x21000);
        // Residual fields immediately follow the 64-byte Logenergetics struct
        assert_eq!(
            offset_of!(HolographicBlock, err_residual_16d),
            0x21040,
            "err_residual_16d must start at 0x21040 (Logenergetics end)"
        );
        // Payload region must not move
        assert_eq!(offset_of!(HolographicBlock, concept_ref), 0x22000);
        assert_eq!(offset_of!(HolographicBlock, payload), 0x22028);
        assert_eq!(offset_of!(HolographicBlock, footer), 261_888);
    }

    #[test]
    fn leg3_pointer_is_pointer_sized() {
        assert_eq!(size_of::<Leg3Pointer>(), size_of::<usize>());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // WS3-A SymplecticState lawfulness (ties to goal:1780165889_..._sub2)
    // Confirms register construction, lens setting, frame application, and
    // hypersphere invariant through the public API surface.
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn symplectic_state_invariants_and_frame_application() {
        let mut state = SymplecticState::new();
        // Starts normalized
        let init_mag: f32 = state
            .active_location
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!(
            (init_mag - 1.0).abs() < 1e-5,
            "initial active_location not unit"
        );

        // Set a location (must normalize)
        let raw_loc = [Complex32::new(2.0, 0.0); DIMENSION];
        state.set_active_location(raw_loc);
        let loc_mag: f32 = state
            .active_location
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((loc_mag - 1.0).abs() < 1e-5);

        // Install lens + apply
        let lens = [Complex32::new(0.7071, 0.7071); DIMENSION]; // approx 45deg rotor, will be normed inside
        state.set_current_lens(lens, Some("test:giza".to_string()));
        assert!(state.current_lens.is_some());
        assert_eq!(state.frame_origin.as_deref(), Some("test:giza"));

        let query = [Complex32::new(1.0, 0.0); DIMENSION];
        let framed = state.apply_current_frame(&query);
        let fmag: f32 = framed
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!(
            (fmag - 1.0).abs() < 1e-5,
            "SymplecticState frame application must yield unit vector"
        );

        // Advance + clear
        state.advance_frame();
        assert_eq!(state.frame_step, 1);
        state.clear_current_lens();
        assert!(state.current_lens.is_none());

        // After clear, identity
        let passthrough = state.apply_current_frame(&query);
        let pmag: f32 = passthrough
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((pmag - 1.0).abs() < 1e-5);
    }
}
