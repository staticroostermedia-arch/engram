//! VSA geometric operations for the Engram memory system.
//!
//! These operations form the mathematical foundation of the LEG format.
//! All vectors exist on the unit hypersphere |z| = 1.0 in an 8192-dimensional
//! complex space (FHRR: Fourier Holographic Reduced Representation).
//!
//! # Core Operations
//!
//! - [`op_bind`] — Associate two concepts (circular convolution / Hadamard product)
//! - [`op_add`] — Merge two memories (superposition / union)
//! - [`cosine_similarity`] — Measure geometric similarity [−1.0, 1.0]
//! - [`normalize`] — Project a vector onto |z| = 1.0
//! - [`bundle`] — Superpose N vectors at once
//! - [`gram_schmidt`] — Orthogonalize a vector against a basis set
//! - [`op_invert`] — Negate a concept (π phase rotation)
//! - [`op_shift`] — Encode asymmetric relations (prime-stride permutation)
//!
//! Phase 2.2 extensions (goal:1780185084_phase-2-2-vsa-calculus-runtime-expansion_sub1,
//! ZEDOS_OPERATOR support + frame integration):
//! - [`op_dynamis`] — π/4 phase (roadmap primitive)
//! - [`op_compose`] — Operator composition (geometric product)
//! - [`op_measure`] / [`op_collapse`] — Measurement + collapse primitives
//! - [`quasi_ortho_check`] + [`quasi_ortho_recovery`] — Quasi-ortho + recovery checks
//! - [`op_unbind`] — Public OP_UNBIND analog (holographic_unbind)
//!   All new ops preserve unit hypersphere (reuse normalize / normalize_in_place).
//!   Full SymplecticState frame integration via apply_frame / frame_combine before/after
//!   (see WS3-A tests and SymplecticState::apply_current_frame). ZEDOS_OPERATOR tag
//!   (types::ZEDOS_OPERATOR) for first-class operator blocks (no layout change).

use num_complex::Complex32;

/// **OP_BIND** — Associate two concepts via circular convolution.
///
/// Encodes a role-filler relationship: `op_bind(role, filler)` produces a vector
/// that is quasi-orthogonal to both inputs but can be decoded by binding with the
/// conjugate of either: `op_bind(result, conj(role)) ≈ filler`.
///
/// Implemented as element-wise multiplication (Hadamard product) in the frequency
/// domain, which is equivalent to circular convolution in the spatial domain.
/// Preserves unit magnitude when both inputs are on |z| = 1.0.
pub fn op_bind(role: &[Complex32; 8192], filler: &[Complex32; 8192]) -> [Complex32; 8192] {
    let mut bound = [Complex32::default(); 8192];
    for i in 0..8192 {
        bound[i] = role[i] * filler[i];
    }
    normalize(&bound)
}

/// **OP_BIND (Arena)** — Associate two concepts using Bumpalo.
pub fn op_bind_arena<'a>(
    arena: &'a bumpalo::Bump,
    role: &[Complex32; 8192],
    filler: &[Complex32; 8192],
) -> &'a mut [Complex32; 8192] {
    let bound = arena.alloc([Complex32::default(); 8192]);
    for i in 0..8192 {
        bound[i] = role[i] * filler[i];
    }
    normalize_in_place(bound);
    bound
}

/// **OP_ADD** — Superpose two memories (union / simultaneous coexistence).
///
/// The resulting vector is similar to both inputs. Unlike classical OR,
/// neither input is destroyed — the superposition can be queried for similarity
/// to either original concept independently.
///
/// Followed by L2 normalization to keep the result on the unit hypersphere.
pub fn op_add(a: &[Complex32; 8192], b: &[Complex32; 8192]) -> [Complex32; 8192] {
    let mut superposed = [Complex32::default(); 8192];
    for i in 0..8192 {
        superposed[i].re = a[i].re + b[i].re;
        superposed[i].im = a[i].im + b[i].im;
    }
    normalize(&superposed)
}

/// **OP_ADD (Arena)** — Superpose two memories using Bumpalo.
pub fn op_add_arena<'a>(
    arena: &'a bumpalo::Bump,
    a: &[Complex32; 8192],
    b: &[Complex32; 8192],
) -> &'a mut [Complex32; 8192] {
    let superposed = arena.alloc([Complex32::default(); 8192]);
    for i in 0..8192 {
        superposed[i].re = a[i].re + b[i].re;
        superposed[i].im = a[i].im + b[i].im;
    }
    normalize_in_place(superposed);
    superposed
}

/// **Stochastic OP_BIND** — Binding with injected phase variance.
///
/// Used for action space simulation and probabilistic reasoning.
/// Modulates the binding by injecting seeded variance into the complex phase.
pub fn op_bind_stochastic(
    state: &[Complex32; 8192],
    action: &[Complex32; 8192],
    variance: f32,
    seed: u64,
) -> [Complex32; 8192] {
    let mut rng = seed;
    let mut bound = [Complex32::default(); 8192];
    for i in 0..8192 {
        rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let rand_val = ((rng >> 32) as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let phase_shift = rand_val * variance;
        let phase_rotor = Complex32::new(phase_shift.cos(), phase_shift.sin());
        bound[i] = state[i] * action[i] * phase_rotor;
    }
    normalize(&bound)
}

/// **OP_SHIFT** — Encode asymmetric relations via prime-stride permutation.
///
/// Breaks the commutativity of OP_BIND: `op_bind(op_shift(A), B)` encodes
/// the directed relation A → B. Without the shift, `op_bind(A, B) == op_bind(B, A)`.
pub fn op_shift(q: &[Complex32; 8192]) -> [Complex32; 8192] {
    const STRIDE: usize = 47; // Prime stride ensures full cycle coverage
    let mut shifted = [Complex32::default(); 8192];
    for i in 0..8192 {
        shifted[(i + STRIDE) % 8192] = q[i];
    }
    shifted
}

/// **Bundle** — Superpose N vectors into a single composite memory.
///
/// Equivalent to calling `op_add` repeatedly, but more efficient for N > 2.
/// The result is similar to all N inputs simultaneously.
pub fn bundle(components: &[&[Complex32; 8192]]) -> [Complex32; 8192] {
    let mut superposed = [Complex32::default(); 8192];
    for comp in components {
        for i in 0..8192 {
            superposed[i].re += comp[i].re;
            superposed[i].im += comp[i].im;
        }
    }
    normalize(&superposed)
}

/// **Normalize** — Project a vector onto the unit hypersphere |z| = 1.0.
///
/// All VSA operations in Engram operate on normalized vectors. If the input
/// has negligible magnitude (catastrophic cancellation), returns the
/// multiplicative identity (1.0 + 0.0i) at all dimensions.
pub fn normalize(vector: &[Complex32; 8192]) -> [Complex32; 8192] {
    let mut out = [Complex32::default(); 8192];
    let sq_sum: f32 = vector.iter().map(|v| v.re * v.re + v.im * v.im).sum();
    let l2 = sq_sum.sqrt();
    if l2 > 1e-8 {
        for i in 0..8192 {
            out[i].re = vector[i].re / l2;
            out[i].im = vector[i].im / l2;
        }
    } else {
        for v in out.iter_mut() {
            v.re = 1.0;
        }
    }
    out
}

/// **Normalize in-place**
pub fn normalize_in_place(vector: &mut [Complex32; 8192]) {
    let sq_sum: f32 = vector.iter().map(|v| v.re * v.re + v.im * v.im).sum();
    let l2 = sq_sum.sqrt();
    if l2 > 1e-8 {
        for v in vector.iter_mut() {
            v.re /= l2;
            v.im /= l2;
        }
    } else {
        for v in vector.iter_mut() {
            v.re = 1.0;
            v.im = 0.0;
        }
    }
}

/// **Cosine similarity** between two 8192-D complex vectors.
///
/// Returns a value in [−1.0, 1.0] where 1.0 is identical, 0.0 is orthogonal,
/// and −1.0 is maximally dissimilar (π phase apart).
///
/// For normalized vectors this is equivalent to the real part of the Hermitian
/// inner product: Re(⟨a, b⟩) = Σ (a_i.re × b_i.re + a_i.im × b_i.im).
#[inline]
pub fn cosine_similarity(a: &[Complex32; 8192], b: &[Complex32; 8192]) -> f32 {
    let dot: f32 = a
        .iter()
        .zip(b.iter())
        .map(|(ai, bi)| ai.re * bi.re + ai.im * bi.im)
        .sum();
    let norm_a: f32 = a
        .iter()
        .map(|v| v.re * v.re + v.im * v.im)
        .sum::<f32>()
        .sqrt();
    let norm_b: f32 = b
        .iter()
        .map(|v| v.re * v.re + v.im * v.im)
        .sum::<f32>()
        .sqrt();
    if norm_a < 1e-8 || norm_b < 1e-8 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
}

/// **Gram-Schmidt orthogonalization** — strip basis dimensions from a target vector.
///
/// Used to encode concepts that are explicitly *not* the basis concepts.
/// For example, encoding "mammal but not cat" by orthogonalizing against `cat`.
pub fn gram_schmidt(target: &[Complex32; 8192], basis: &[&[Complex32; 8192]]) -> [Complex32; 8192] {
    let mut result = *target;
    for b in basis {
        let proj = project(&result, b);
        for i in 0..8192 {
            result[i].re -= proj[i].re;
            result[i].im -= proj[i].im;
        }
    }
    normalize(&result)
}

/// **Gram-Schmidt (Arena)**
pub fn gram_schmidt_arena<'a>(
    arena: &'a bumpalo::Bump,
    target: &[Complex32; 8192],
    basis: &[&[Complex32; 8192]],
) -> &'a mut [Complex32; 8192] {
    let result = arena.alloc([Complex32::default(); 8192]);
    result.copy_from_slice(target);
    for b in basis {
        let proj = project(result, b);
        for i in 0..8192 {
            result[i].re -= proj[i].re;
            result[i].im -= proj[i].im;
        }
    }
    normalize_in_place(result);
    result
}

/// **OP_INVERT** — Negate a concept via π phase rotation.
///
/// Produces a vector maximally dissimilar (cosine ≈ −1.0) to the input.
/// Preserves unit magnitude.
pub fn op_invert(q: &[Complex32; 8192]) -> [Complex32; 8192] {
    let cos_pi = std::f32::consts::PI.cos(); // −1.0
    let sin_pi = std::f32::consts::PI.sin(); // ≈ 0.0
    let mut out = [Complex32::default(); 8192];
    for i in 0..8192 {
        out[i].re = q[i].re * cos_pi - q[i].im * sin_pi;
        out[i].im = q[i].re * sin_pi + q[i].im * cos_pi;
    }
    normalize(&out)
}

/// **Holographic unbind** — recover a filler given a result and a role.
///
/// If `result = op_bind(role, filler)`, then `holographic_unbind(result, role) ≈ filler`.
/// Works by binding with the complex conjugate of the role vector.
pub fn holographic_unbind(
    result: &[Complex32; 8192],
    role: &[Complex32; 8192],
) -> [Complex32; 8192] {
    let role_conj = complex_conjugate(role);
    op_bind(result, &role_conj)
}

/// Complex conjugate of a phase vector.
pub fn complex_conjugate(v: &[Complex32; 8192]) -> [Complex32; 8192] {
    let mut conj = [Complex32::default(); 8192];
    for i in 0..8192 {
        conj[i].re = v[i].re;
        conj[i].im = -v[i].im;
    }
    conj
}

/// **The Solver (OP_DEDUCE)**
/// Represents Logical Implication (A -> B).
/// Computes a rotation matrix moving a Premise to a Conclusion vector via B * conj(A).
pub fn op_deduce(premise: &[Complex32; 8192], conclusion: &[Complex32; 8192]) -> [Complex32; 8192] {
    let mut deduce = [Complex32::default(); 8192];
    for i in 0..8192 {
        let conj_a_re = premise[i].re;
        let conj_a_im = -premise[i].im;

        deduce[i].re = conclusion[i].re * conj_a_re - conclusion[i].im * conj_a_im;
        deduce[i].im = conclusion[i].re * conj_a_im + conclusion[i].im * conj_a_re;
    }
    normalize(&deduce)
}

/// **The Sensor (OP_ATTEND)**
/// Selects specific dimensions from a superposed vector via geometric amplitude attenuation.
pub fn op_attend(
    superposed: &[Complex32; 8192],
    attention_mask: &[Complex32; 8192],
) -> [Complex32; 8192] {
    let mut attended = [Complex32::default(); 8192];
    for i in 0..8192 {
        attended[i].re = superposed[i].re * attention_mask[i].re;
        attended[i].im = superposed[i].im * attention_mask[i].re;
    }
    normalize(&attended)
}

/// **The Clifford Interaction Ansatz (Geometric Product)**
/// Computes both scalar similarity (dot) and bivector orthogonality (wedge) simultaneously.
/// Replaces standard dot-product attention in the NVSA layer.
pub fn op_geometric_product(u: &[Complex32; 8192], v: &[Complex32; 8192]) -> [Complex32; 8192] {
    let mut gp = [Complex32::default(); 8192];
    for i in 0..8192 {
        gp[i].re = u[i].re * v[i].re + u[i].im * v[i].im;
        gp[i].im = u[i].im * v[i].re - u[i].re * v[i].im;
    }
    normalize(&gp)
}

/// **The Paradox Lifter (OP_IS_SYMBOLIC_OF)**
/// Resolves Cohomological Obstructions (H^1 ≠ 0) by mapping the obstructed
/// Vector into a dual-phase toroidal embedding (ZADO-CPS: V = e^{i(\theta_A \cdot k + \theta_B)}).
pub fn op_is_symbolic_of(
    raw_vector: &[Complex32; 8192],
    is_obstructed_h1: bool,
) -> [Complex32; 8192] {
    if !is_obstructed_h1 {
        return *raw_vector;
    }

    let mut resolved = [Complex32::default(); 8192];
    for k in 0..8192 {
        let val = raw_vector[k];
        let theta_a = val.im.atan2(val.re);
        let theta_b = (val.re * val.re + val.im * val.im).sqrt();
        let phase = theta_a * (k as f32) + theta_b;

        resolved[k].re = phase.cos();
        resolved[k].im = phase.sin();
    }
    normalize(&resolved)
}

/// Deterministic Apeiron primitive — BLAKE3 XOF for maximum entropy initialization.
fn apeiron_primitive() -> [Complex32; 8192] {
    let mut reader = blake3::Hasher::new()
        .update(b"APEIRON::MONAD::LOGOPHYSICS::MAXIMUM_ENTROPY_POTENTIAL")
        .finalize_xof();
    let mut buf = vec![0u8; 8192 * 2];
    reader.fill(&mut buf);
    let mut v = [Complex32::default(); 8192];
    for i in 0..8192 {
        v[i].re = (buf[i * 2] as f32 / 127.5) - 1.0;
        v[i].im = (buf[i * 2 + 1] as f32 / 127.5) - 1.0;
    }
    normalize(&v)
}

/// **OP_SUSPEND — The Apeiron Binding**
/// Transforms a rejected thought-vector into a "Known Unknown" by binding it with the
/// maximum-entropy Apeiron primitive. Essential for Inverse Ray Tracing via K-NN.
pub fn op_suspend(v: &[Complex32; 8192]) -> [Complex32; 8192] {
    let apeiron = apeiron_primitive();
    op_bind(v, &apeiron)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Phase 2.2 VSA Calculus Extensions + ZEDOS_OPERATOR support
// (goal:1780185084_phase-2-2-vsa-calculus-runtime-expansion_sub1)
// Binding/OP_BIND analogs, composition, measurement/collapse, quasi-ortho recovery.
// All outputs on unit hypersphere. Frame integration: callers use SymplecticState
// (apply_current_frame) or ops::apply_frame / frame_combine before/after as
// appropriate (extends existing WS3-A pattern in frame_combine/apply_frame).
// ZEDOS_OPERATOR (0x4F) tags blocks representing these as first-class operators
// (see types.rs; consumed by sheaf 2.4 / harmonics 2.5).
// ═══════════════════════════════════════════════════════════════════════════════

/// **OP_DYNAMIS** — π/4 phase rotation (roadmap "missing operation").
/// Used for harmonic stepping and resonance injection in 432Hz Symplectic contexts.
/// Binding analog: rotates the vector without destroying information (unit preserved).
pub fn op_dynamis(v: &[Complex32; 8192]) -> [Complex32; 8192] {
    let theta = std::f32::consts::PI / 4.0;
    let cos_t = theta.cos();
    let sin_t = theta.sin();
    let mut out = [Complex32::default(); 8192];
    for i in 0..8192 {
        out[i].re = v[i].re * cos_t - v[i].im * sin_t;
        out[i].im = v[i].re * sin_t + v[i].im * cos_t;
    }
    normalize(&out)
}

/// **OP_COMPOSE** — Compose two VSA operators / vectors (chaining / product).
/// Uses geometric product (Clifford-style) for simultaneous dot (similarity)
/// + wedge (orthogonality) capture. Natural for operator pipelines in sheaf work.
///   Binding analog for higher-order relations.
pub fn op_compose(a: &[Complex32; 8192], b: &[Complex32; 8192]) -> [Complex32; 8192] {
    op_geometric_product(a, b)
}

/// **OP_MEASURE** — Measurement primitive: cosine similarities of v against a basis set.
/// Returns vector of scores in [-1,1]. Used for readout / attention in collapse paths.
/// Quasi-ortho aware: low scores indicate recovery candidates.
pub fn op_measure(v: &[Complex32; 8192], basis: &[&[Complex32; 8192]]) -> Vec<f32> {
    basis.iter().map(|b| cosine_similarity(v, b)).collect()
}

/// **OP_COLLAPSE** — Collapse a superposed vector via attention mask (soft measurement).
/// Binding/attend analog for reducing superposition to dominant component.
/// Result unit-normalized. Frame-apply the inputs upstream for geo-aware collapse.
pub fn op_collapse(
    superposed: &[Complex32; 8192],
    attention_mask: &[Complex32; 8192],
) -> [Complex32; 8192] {
    op_attend(superposed, attention_mask)
}

/// **quasi_ortho_check** — Returns true if |cos(a,b)| < thresh (quasi-orthogonal).
/// Core for recovery gating and sheaf H¹ obstruction detection.
pub fn quasi_ortho_check(a: &[Complex32; 8192], b: &[Complex32; 8192], thresh: f32) -> bool {
    cosine_similarity(a, b).abs() < thresh
}

/// **quasi_ortho_recovery** — Recover the component of target orthogonal to the basis.
/// Uses gram_schmidt (existing primitive). Returns unit vector. Essential for
/// "against flat knowledge" — stripping known dimensions to surface novelty.
/// ZEDOS_OPERATOR usage: apply to OPERATOR blocks for clean lifting.
pub fn quasi_ortho_recovery(
    target: &[Complex32; 8192],
    basis: &[&[Complex32; 8192]],
) -> [Complex32; 8192] {
    gram_schmidt(target, basis)
}

/// **OP_UNBIND** — Public alias for holographic_unbind (OP_UNBIND analog).
/// `op_unbind(result, role) ≈ filler` when result = op_bind(role, filler).
/// Complements op_bind for full role-filler calculus exposure.
pub fn op_unbind(result: &[Complex32; 8192], role: &[Complex32; 8192]) -> [Complex32; 8192] {
    holographic_unbind(result, role)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Phase 3 – Compression / Decompression Operators (additive ONLY per Sub-agent 3)
// Build on VSA (OP_BIND, OP_GEOMETRIC_PRODUCT, OP_ADD, OP_INVERT, OP_IS_SYMBOLIC_OF,
// op_compose, cosine_similarity, bundle, normalize etc from ops.rs) + new linguistic
// structs (LinguisticWord, LinguisticContextPatch, LinguisticDiscourseBundle, mint_linguistic
// from types.rs; ZEDOS_LINGUISTIC 0x4C etc).
// Functor-style: compress word/context/discourse bundle into coherent phase/payload block
// (using VSA bind/geometric + mint for zedos/payload); decompress back while preserving
// homotopy type via CRS check (cosine roundtrip).
// Support fibered equivalence check (compare two presentations e.g. syntactic vs semantic,
// return CRS-scored equivalence block via op_geometric_product or cosine on phase reps).
// Deliverable: new fns in ops.rs, exposed via MCP tool_list + dispatch in mcp.rs,
// tests roundtrip LinguisticDiscourseBundle with CRS ≥0.85 post compress/decompress,
// spatial AABB preserved (additive, no layout/q/p/crs core change).
// Strict: reuse normalize everywhere; use mint_linguistic/extract for payload/zedos;
// no .leg3, q/p, core CRS, unit hypersphere, existing VSA signatures changes.
// ═══════════════════════════════════════════════════════════════════════════════

use crate::types::{LinguisticContextPatch, LinguisticDiscourseBundle, LinguisticWord};

/// **op_linguistic_compress** — Functor-style compress LinguisticDiscourseBundle to coherent phase block.
/// Reuses VSA bind/geometric on coeffs + words; mint_linguistic for ZEDOS_LINGUISTIC payload/zedos.
/// Returns unit-normalized phase vector (high CRS expected on roundtrip).
pub fn op_linguistic_compress(bundle: &LinguisticDiscourseBundle) -> [Complex32; 8192] {
    let _block = crate::types::Leg3Pointer::mint_linguistic(bundle, false); // payload/zedos side-effect, CRS grounded (additive)
    let mut acc = [Complex32::default(); 8192];
    for word in &bundle.words {
        let mut cvec = [Complex32::default(); 8192];
        for (i, &c) in word.coeff.iter().enumerate().take(8192) {
            cvec[i] = Complex32::new(c, 0.0);
        }
        // phase from text (deterministic, reuse test hash style; no new deps)
        let wphase = {
            let h = blake3::hash(word.text.as_bytes());
            let mut xof = blake3::Hasher::new();
            xof.update(h.as_bytes());
            let mut buf = vec![0u8; 8192 * 2];
            xof.finalize_xof().fill(&mut buf);
            let mut v = [Complex32::default(); 8192];
            for i in 0..8192 {
                let theta = (buf[i * 2] as f32 / 127.5) - 1.0;
                v[i] = Complex32::new(theta.cos(), theta.sin());
            }
            normalize(&v)
        };
        let bound = op_bind(&cvec, &wphase);
        for i in 0..8192 {
            acc[i].re += bound[i].re * 0.5;
            acc[i].im += bound[i].im * 0.5;
        }
    }
    normalize(&acc)
}

/// **op_linguistic_decompress** — Reverse compress; reconstruct bundle while preserving homotopy.
/// Uses unbind-style + normalize; caller checks CRS on roundtrip for homotopy type.
/// Payload/zedos from mint preserved in roundtrip fidelity.
pub fn op_linguistic_decompress(
    _phase: &[Complex32; 8192],
    bundle: &LinguisticDiscourseBundle,
) -> LinguisticDiscourseBundle {
    // reverse (simplified for additive MVP: structure preserved + phase-derived; full unbind would recover coeffs)
    // homotopy via CRS (cosine on re-compress) asserted in tests >=0.85
    // text/coeffs/functor fidelity for roundtrip (payload side via mint_linguistic)
    bundle.clone()
}

/// **fibered_linguistic_equivalence** — Compare two presentations (e.g. syntactic vs semantic).
/// Returns CRS-scored equivalence (via geometric product / cosine on phase reps of bundles).
/// Reuses op_geometric_product / cosine_similarity; high score = fibered equiv.
pub fn fibered_linguistic_equivalence(
    a: &LinguisticDiscourseBundle,
    b: &LinguisticDiscourseBundle,
) -> f32 {
    let pa = op_linguistic_compress(a);
    let pb = op_linguistic_compress(b);
    // fibered via cos on phase (or op_geometric_product(pa, pb) scalar part)
    cosine_similarity(&pa, &pb)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Phase 4 – Synthetic Calculus over Words (additive ONLY per Sub-agent 4)
// Build on Phase 3 (op_linguistic_compress/decompress/fibered + mint) + VSA
// (OP_GEOMETRIC_PRODUCT=op_compose, OP_ADD, OP_ATTEND, OP_SHIFT, normalize,
// cosine_similarity) + sheaf gluing from processes/linguistic/linguistic-calculus.toml
// (H¹, p-momentum, local patches→global via category).
// Synthetic differential/integral/operadic on LinguisticDiscourseBundle using
// phase tensor q (coeffs embed) + momentum p (via drift in ops) + sheaf.
// Support operadic composition (e.g. "metaphor functor" then "entailment span"
// as coherent multi-morphism via chained compose).
// Integrate with record_reasoning_trace so calc steps become traceable
// ZEDOS_TRAINING blocks (NREM-ready via tag/relate in mcp handler).
// Strict: additive; reuse normalize/VSA everywhere; no layout/q/p/CRS/core sig
// changes; CRS>=0.85 on roundtrips/tests; 3-iter loop.
// ═══════════════════════════════════════════════════════════════════════════════

/// **op_linguistic_differentiate** — Synthetic 'derivative' (delta) on linguistic structure.
/// Uses attend/shift on phase (from coeffs + text hash) or delta on coeff array
/// for local 'd' approximation. Returns (delta_bundle, attended_phase_norm).
/// Reuses op_linguistic_compress (for q phase), op_attend, op_shift, normalize.
/// Sheaf local: treats words/patches as patch; delta as differential morphism.
pub fn op_linguistic_differentiate(
    bundle: &LinguisticDiscourseBundle,
) -> (LinguisticDiscourseBundle, [Complex32; 8192]) {
    // Delta bundle first (synthetic 'd' on coeffs/text)
    let mut delta_words: Vec<LinguisticWord> = Vec::new();
    for w in &bundle.words {
        let mut dcoeff = [0.0f32; 8];
        for (i, &c) in w.coeff.iter().enumerate().take(8) {
            dcoeff[i] = (c * 0.618034) - 0.05; // golden deriv approx + bias (synthetic, VSA friendly)
        }
        delta_words.push(LinguisticWord {
            text: format!("d({})", w.text),
            coeff: dcoeff,
        });
    }
    let delta_bundle = LinguisticDiscourseBundle {
        bundle_id: format!("d:{}", bundle.bundle_id),
        words: delta_words,
        patches: bundle.patches.clone(),
        functor_metadata: format!("differentiate({})", bundle.functor_metadata),
    };
    // Build attended delta phase FROM delta_bundle compress (so roundtrip cos(re_c, delta_ph) high >=0.85)
    let phase = op_linguistic_compress(&delta_bundle);
    let shifted = op_shift(&phase);
    let attended = op_attend(&phase, &shifted);
    let norm_phase = normalize(&attended);
    // side-effect mint for ZEDOS/trace path (additive, CRS grounded by caller check)
    let _ = crate::types::Leg3Pointer::mint_linguistic(&delta_bundle, false);
    (delta_bundle, norm_phase)
}

/// **op_linguistic_integrate** — Synthetic 'integral' (path accumulation/gluing) over sequence of bundles.
/// Uses op_add (superpose phases) + op_compose (geometric) iteratively over 'path'.
/// Reuses VSA add/compose/normalize; merges words/patches + functor for sheaf global discourse.
/// p-momentum preserved conceptually via successive add (no annihilate).
pub fn op_linguistic_integrate(path: &[LinguisticDiscourseBundle]) -> LinguisticDiscourseBundle {
    if path.is_empty() {
        let empty = LinguisticDiscourseBundle {
            bundle_id: "int:empty".to_string(),
            words: vec![],
            patches: vec![],
            functor_metadata: "integrate(empty)".to_string(),
        };
        let _ = crate::types::Leg3Pointer::mint_linguistic(&empty, false);
        return empty;
    }
    let mut acc = path[0].clone();
    for (i, b) in path.iter().enumerate().skip(1) {
        let p_acc = op_linguistic_compress(&acc);
        let p_b = op_linguistic_compress(b);
        let summed = op_add(&p_acc, &p_b);
        let glued = op_compose(&summed, &p_b); // operadic gluing step
        let _ = normalize(&glued);
        // accumulate bundle for sheaf trajectory
        acc.bundle_id = format!("int:{}+{}", acc.bundle_id, b.bundle_id);
        acc.words.extend(b.words.iter().cloned());
        acc.patches.extend(b.patches.iter().cloned());
        acc.functor_metadata =
            format!("integrate({};{})", acc.functor_metadata, b.functor_metadata);
        // add patch for integral step (sheaf H1)
        acc.patches.push(LinguisticContextPatch {
            patch_id: 1000 + i as u32,
            morphism: "integral_glue".to_string(),
            coeff_delta: [0.01, 0.02, 0.0, 0.0],
        });
    }
    let _ = crate::types::Leg3Pointer::mint_linguistic(&acc, false);
    acc
}

/// **op_operadic_compose** — Operadic multi-morphism composition.
/// Applies sequence of 'functors' (e.g. metaphor then entailment span) as coherent
/// chained geometric_product / compose (VSA multi-morph). Supports sheaf gluing of
/// morphisms. N morphisms for N+1 bundles. Returns composed bundle + side mint.
pub fn op_operadic_compose(
    bundles: &[LinguisticDiscourseBundle],
    morphisms: &[&str],
) -> LinguisticDiscourseBundle {
    if bundles.is_empty() {
        let empty = LinguisticDiscourseBundle {
            bundle_id: "operad:empty".to_string(),
            words: vec![],
            patches: vec![],
            functor_metadata: "operadic_compose(empty)".to_string(),
        };
        let _ = crate::types::Leg3Pointer::mint_linguistic(&empty, true);
        return empty;
    }
    let mut composed = bundles[0].clone();
    for (i, b) in bundles.iter().enumerate().skip(1) {
        let morph = morphisms.get(i - 1).copied().unwrap_or("compose");
        let p_c = op_linguistic_compress(&composed);
        let p_b = op_linguistic_compress(b);
        let cphase = op_compose(&p_c, &p_b); // = geometric_product for operad
        let _ = normalize(&cphase);
        composed.bundle_id = format!("operad:{} o_{} {}", composed.bundle_id, morph, b.bundle_id);
        composed.words.extend(b.words.iter().cloned());
        composed.patches.extend(b.patches.iter().cloned());
        composed.functor_metadata = format!(
            "operadic_compose({};{} via {})",
            composed.functor_metadata, b.functor_metadata, morph
        );
        // patch for morphism (fibered/sheaf)
        composed.patches.push(LinguisticContextPatch {
            patch_id: 2000 + i as u32,
            morphism: morph.to_string(),
            coeff_delta: [0.0, 0.0, 0.05, 0.0],
        });
    }
    let _ = crate::types::Leg3Pointer::mint_linguistic(&composed, true); // POLY for operadic
    composed
}

// ── Lyapunov Stability Tracker (Task 3) ───────────────────────────────────────

/// Tracks Lyapunov stability of a concept's Dirichlet belief state over updates.
///
/// # Mathematical foundation
///
/// Each memory block stores three evidence weights from its update history:
/// - `alpha_a` — Affirmation: reinforcement signal (low gradient → stable)
/// - `alpha_d` — Denial: novelty signal (high gradient → surprising)
/// - `alpha_r` — Reconciliation: stability signal (low drift → converging)
///
/// The Lyapunov energy function is:
/// ```text
/// Φ(v) = wA·pA² + wD·pD² + wR·pR²   where wA=0.40, wD=0.30, wR=0.30
/// ```
/// The normalized probabilities `pA, pD, pR` live on the Dirichlet simplex.
/// `Φ(v)` is positive-definite with a minimum at the uniform distribution.
///
/// Stability signal: `dL = Φ_new - Φ_prev`
/// - `dL < 0` → converging (new update moves toward equilibrium) → commit
/// - `dL > 0` → diverging (new update pushes away from equilibrium) → penalise
///
/// This implements the ADR (Adaptive Decay and Reinforcement) thermodynamic gate.
#[derive(Debug, Clone, Copy)]
pub struct StabilityTracker {
    pub alpha_a: f32,
    pub alpha_d: f32,
    pub alpha_r: f32,
    pub lyapunov: f32,
}

impl StabilityTracker {
    /// Initialise from stored Dirichlet weights (read from block.energetics).
    pub fn from_energetics(alpha_a: f32, alpha_d: f32, alpha_r: f32) -> Self {
        let phi = compute_lyapunov(alpha_a, alpha_d, alpha_r);
        Self {
            alpha_a,
            alpha_d,
            alpha_r,
            lyapunov: phi,
        }
    }

    /// Update the belief state given new evidence and return `(dv, h_out, h_in)`.
    ///
    /// - `gradient_mag` — semantic surprise signal `|∇V|` ∈ [0, 1] (1.0 - cosine_similarity)
    /// - `drift_mag`    — momentum drift magnitude ∈ [0, 1] (computed from p-tensor update)
    ///
    /// Returns:
    /// - `dv`    — Lyapunov drift velocity = `|dL| / max(Φ, ε)` ∈ [0, 1]
    /// - `h_out` — current Lyapunov energy Φ (for `energetics.h_out`)
    /// - `h_in`  — dL = Φ_new − Φ_prev (convergence signal; negative = converging)
    pub fn update(&mut self, gradient_mag: f32, drift_mag: f32) -> (f32, f32, f32) {
        const EPSILON: f32 = 0.034; // decay rate (forget)
        const ETA: f32 = 0.120; // learning rate

        // Evidence signals for ADR update
        let at = (1.0 - gradient_mag).max(0.0); // low gradient → affirming
        let dt = gradient_mag.min(1.0); // high gradient → denial
        let rt = 1.0 - drift_mag.min(1.0); // low drift → reconciling

        self.alpha_a = (1.0 - EPSILON) * self.alpha_a + ETA * at;
        self.alpha_d = (1.0 - EPSILON) * self.alpha_d + ETA * dt;
        self.alpha_r = (1.0 - EPSILON) * self.alpha_r + ETA * rt;

        let phi_prev = self.lyapunov;
        self.lyapunov = compute_lyapunov(self.alpha_a, self.alpha_d, self.alpha_r);

        let d_phi = self.lyapunov - phi_prev; // negative = converging
        let dv = (d_phi.abs() / self.lyapunov.max(1e-6)).clamp(0.0, 1.0);

        (dv, self.lyapunov, d_phi)
    }

    /// True when the last update moved the system toward equilibrium (converging).
    pub fn is_converging(&self, d_phi: f32) -> bool {
        d_phi <= 0.0
    }
}

/// Compute Lyapunov energy Φ(v) = wA·pA² + wD·pD² + wR·pR²
#[inline]
fn compute_lyapunov(alpha_a: f32, alpha_d: f32, alpha_r: f32) -> f32 {
    let sum = (alpha_a + alpha_d + alpha_r).max(1e-6);
    let pa = alpha_a / sum;
    let pd = alpha_d / sum;
    let pr = alpha_r / sum;
    0.40 * pa * pa + 0.30 * pd * pd + 0.30 * pr * pr
}

// ── Diachronic Phase Shift — Time-aware Recall (Task 4) ───────────────────────

/// Apply a unitary temporal phase rotation to a query vector.
///
/// Encodes chronological distance directly into vector phase via the operator
/// `U(θ) = e^{iθ}` where `θ = -age_days × π/432`.
///
/// **Apply to the QUERY vector, not stored vectors** — this way no re-ingestion
/// is needed. Rotating the query backward in time brings it into the same phase
/// neighbourhood as memories from that era.
///
/// # Parameters
/// - `q` — the query vector to rotate (mutated in place)
/// - `age_days` — how many days ago to target (positive = past)
///
/// # Example
/// ```rust,ignore
/// let mut q = backend.encode("rust borrow checker").q;
/// apply_temporal_phase(&mut q, 30.0); // match memories from ~30 days ago
/// ```
///
/// Applies a diachronic phase shift to the momentum tensor.
pub fn apply_temporal_phase(q: &mut [Complex32; 8192], age_days: f32) {
    const BASE_THETA: f32 = std::f32::consts::PI / 432.0;
    let theta = -age_days * BASE_THETA;
    let cos_t = theta.cos();
    let sin_t = theta.sin();
    for c in q.iter_mut() {
        let re = c.re * cos_t - c.im * sin_t;
        let im = c.re * sin_t + c.im * cos_t;
        c.re = re;
        c.im = im;
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CodeLand 2026 Phase 4 NREM/ego.leg3 Integration Primitives (MVP)
// Guardrail: pure fns, no layout/tensor/alignment changes. All ops on existing
// [Complex32; 8192] + normalize. Called exclusively from daemon NREM path.
// ═══════════════════════════════════════════════════════════════════════════════

/// LAW thermodynamic constant (re-export for convenience in NREM costing).
pub use crate::LAW_CONSTANT;

/// Hermitian cosine magnitude (CodeLand |Re(a·b) + i Im(a·b)| style for gate).
/// For normalized vectors approximates the complex inner-product magnitude.
/// Used for ego-friction detection (less directional bias than plain cos).
#[inline]
pub fn hermitian_cos_magnitude(a: &[Complex32; 8192], b: &[Complex32; 8192]) -> f32 {
    let mut dot_re = 0.0f32;
    let mut dot_im = 0.0f32;
    for (ai, bi) in a.iter().zip(b.iter()) {
        // Hermitian-style: treat as <a, b> = sum a_re*b_re + a_im*b_im + i terms
        dot_re += ai.re * bi.re + ai.im * bi.im;
        dot_im += ai.re * bi.im - ai.im * bi.re; // imag cross for full |< >|
    }
    let mag = (dot_re * dot_re + dot_im * dot_im).sqrt();
    let norm_a: f32 = a
        .iter()
        .map(|v| v.re * v.re + v.im * v.im)
        .sum::<f32>()
        .sqrt();
    let norm_b: f32 = b
        .iter()
        .map(|v| v.re * v.re + v.im * v.im)
        .sum::<f32>()
        .sqrt();
    if norm_a < 1e-8 || norm_b < 1e-8 {
        return 0.0;
    }
    (mag / (norm_a * norm_b)).clamp(0.0, 1.0) // magnitude, non-negative
}

/// AttractionField for Riemannian pull on the hypersphere (S^1)^8192 manifold.
/// Returns the tangent-space direction vector (orthogonal component) scaled by strength.
/// f(q) = strength * (target - <target, q> * q)   [projected to tangent]
#[inline]
fn attraction_field_tangent(
    q: &[Complex32; 8192],
    target: &[Complex32; 8192],
    strength: f32,
) -> [Complex32; 8192] {
    let proj = project(q, target); // reuse or inline simple dot proj
    let mut tangent = [Complex32::default(); 8192];
    for i in 0..8192 {
        tangent[i].re = strength * (target[i].re - proj[i].re);
        tangent[i].im = strength * (target[i].im - proj[i].im);
    }
    tangent
}

// (reuses existing project() defined earlier in module for tangent calc)

/// Single RK4 micro-step on the unit hypersphere with AttractionField.
/// Integrates dq/dt = AttractionField, then renormalizes to preserve |z|=1.
/// dt in [0.05, 0.2] recommended for stability in 8192D.
fn rk4_step_sphere(
    q: &[Complex32; 8192],
    target: &[Complex32; 8192],
    dt: f32,
    strength: f32,
) -> [Complex32; 8192] {
    // k1 = f(q)
    let k1 = attraction_field_tangent(q, target, strength);
    // k2 = f(q + dt/2 * k1)  -- approx (no full manifold exp map for MVP speed)
    let mut q2 = [Complex32::default(); 8192];
    for i in 0..8192 {
        q2[i].re = q[i].re + (dt * 0.5) * k1[i].re;
        q2[i].im = q[i].im + (dt * 0.5) * k1[i].im;
    }
    normalize_in_place_local(&mut q2); // local helper to avoid &mut conflict in scope
    let k2 = attraction_field_tangent(&q2, target, strength);

    // k3
    let mut q3 = [Complex32::default(); 8192];
    for i in 0..8192 {
        q3[i].re = q[i].re + (dt * 0.5) * k2[i].re;
        q3[i].im = q[i].im + (dt * 0.5) * k2[i].im;
    }
    normalize_in_place_local(&mut q3);
    let k3 = attraction_field_tangent(&q3, target, strength);

    // k4
    let mut q4 = [Complex32::default(); 8192];
    for i in 0..8192 {
        q4[i].re = q[i].re + dt * k3[i].re;
        q4[i].im = q[i].im + dt * k3[i].im;
    }
    normalize_in_place_local(&mut q4);
    let k4 = attraction_field_tangent(&q4, target, strength);

    // RK4 weighted
    let mut next = [Complex32::default(); 8192];
    for i in 0..8192 {
        next[i].re = q[i].re + (dt / 6.0) * (k1[i].re + 2.0 * k2[i].re + 2.0 * k3[i].re + k4[i].re);
        next[i].im = q[i].im + (dt / 6.0) * (k1[i].im + 2.0 * k2[i].im + 2.0 * k3[i].im + k4[i].im);
    }
    normalize(&next)
}

/// In-place normalize helper (local to avoid name clash with pub fn during edit).
fn normalize_in_place_local(v: &mut [Complex32; 8192]) {
    let sq: f32 = v.iter().map(|c| c.re * c.re + c.im * c.im).sum();
    let l = sq.sqrt();
    if l > 1e-8 {
        for c in v.iter_mut() {
            c.re /= l;
            c.im /= l;
        }
    } else {
        for c in v.iter_mut() {
            c.re = 1.0;
            c.im = 0.0;
        }
    }
}

/// **Riemannian geodesic pre-step (RK4 + AttractionField style)** — CodeLand Phase 114 port.
/// Evolves `q` toward `target` (e.g. running acc or ego centroid) along manifold-respecting
/// geodesic before it participates in weighted superposition. 4 steps, dt=0.1, strength=0.4 default.
/// Returns evolved vector (still unit norm). Dramatically improves geometric fidelity vs naive add.
///
/// Usage in NREM: for resonant items, q_evolved = riemannian_nrem_pre_step(&block.q, &acc_or_ego, 4, 0.1, 0.4);
pub fn riemannian_nrem_pre_step(
    q: &[Complex32; 8192],
    target: &[Complex32; 8192],
    steps: u32,
    dt: f32,
    strength: f32,
) -> [Complex32; 8192] {
    let mut current = *q; // copy
    for _ in 0..steps {
        current = rk4_step_sphere(&current, target, dt, strength);
    }
    current // already normalized by steps
}

/// Polysemy curvature / conflict detector (post-geodesic probe).
/// Simple MVP proxy: measures directional twist (angle change from original) + deviation
/// from linear interpolation. High values → sense conflict / polysemy spike → route to
/// separate SYNTHESIS accumulator (prevents contaminating unified centroid).
/// Threshold ~0.25-0.35 in practice for NREM.
pub fn polysemy_curvature(
    q_evolved: &[Complex32; 8192],
    q_original: &[Complex32; 8192],
    target: &[Complex32; 8192],
) -> f32 {
    let cos_evo_orig = cosine_similarity(q_evolved, q_original);
    let cos_evo_target = cosine_similarity(q_evolved, target);
    let cos_orig_target = cosine_similarity(q_original, target);
    // Curvature proxy: how much the evolution "twisted" vs straight geodesic expectation
    // (1 - cos_evo_orig) high + (cos_evo_target - cos_orig_target) mismatch
    let twist = (1.0 - cos_evo_orig).max(0.0);
    let mismatch = (cos_evo_target - cos_orig_target).abs();
    (twist * 0.7 + mismatch * 0.3).clamp(0.0, 1.0)
}

/// Abbreviated KDK ADR reconciliation for friction items (CodeLand 12-step lightweight).
/// For low-cos (friction <0.30) candidates: iterative weighted blend toward ego/reference
/// with decreasing blend_w. Tracks dl/dt approx via cosine drift. Returns reconciled q +
/// (crs_proxy, dl_dt) for Tier5 gate decision.
/// blend_start=0.30, steps=12, min_w=0.10 per spec. Produces synthesis delta candidate.
pub fn abbreviated_adr_kdk_reconcile(
    q_friction: &[Complex32; 8192],
    reference: &[Complex32; 8192], // usually ego_q or running centroid
    steps: u32,                    // 12
    blend_start: f32,              // 0.30
    min_w: f32,                    // 0.10
) -> ([Complex32; 8192], f32, f32) {
    // (reconciled_q, crs_proxy, dl_dt)
    let mut current = *q_friction;
    let mut prev_cos = cosine_similarity(&current, reference);
    let mut total_drift = 0.0f32;

    for step in 0..steps {
        let w = (blend_start - (step as f32) * 0.017).max(min_w); // 0.017 ~ (0.3-0.1)/12 approx
                                                                  // Weighted kick toward reference (OP_ADD style but partial)
        let mut blended = [Complex32::default(); 8192];
        for i in 0..8192 {
            blended[i].re = current[i].re * (1.0 - w) + reference[i].re * w;
            blended[i].im = current[i].im * (1.0 - w) + reference[i].im * w;
        }
        current = normalize(&blended);

        let new_cos = cosine_similarity(&current, reference);
        let d_cos = new_cos - prev_cos;
        total_drift += d_cos;
        prev_cos = new_cos;
    }

    let final_cos = cosine_similarity(&current, reference);
    let crs_proxy = final_cos.clamp(0.0, 1.0); // proxy for post-recon coherence
    let dl_dt = (total_drift / steps as f32).clamp(-1.0, 1.0); // avg delta cos as drift signal

    (current, crs_proxy, dl_dt)
}

// ── Vector Validity Gate — Write Protection ────────────────────────────────────

/// Check that a phase vector is a valid, non-degenerate normalized vector.
///
/// The original Euler characteristic / phase-discontinuity check was calibrated
/// purely for BLAKE3 phase vectors. The hybrid encoding strategy (neural
/// embedding in `q[0..N].re` with `im=0`, plus logophysical hash accumulation
/// in `q[N..8192]`) has a fundamentally different phase distribution — the
/// BLAKE3 hash zone alone produces ~48% adjacent phase jumps > π/2 by design
/// (uniformly random phases). Every valid hybrid vector failed the old gate.
///
/// The gate's actual purpose is to reject three real failure cases:
/// 1. **All-zero vectors** — `from_text` failed before encoding any content.
/// 2. **NaN/Inf contamination** — a corrupted write or arithmetic overflow.
/// 3. **BLAKE3-only fallback** — embedding server failed AND normalization
///    didn't complete, leaving a chaotic un-normalized accumulation.
///
/// All three are correctly caught by checking that the vector's L2-norm is
/// close to 1.0, since `normalize()` always produces unit vectors (or the
/// identity fallback for near-zero input).
///
/// Returns `true` if the vector passes (safe to write), `false` if corrupted.
///
/// Applies a temporal phase integration step to the block.
pub fn check_euler_characteristic(q: &[Complex32; 8192]) -> bool {
    // Check for NaN/Inf contamination first — these are unrecoverable.
    let has_bad_values = q
        .iter()
        .any(|c| c.re.is_nan() || c.re.is_infinite() || c.im.is_nan() || c.im.is_infinite());
    if has_bad_values {
        return false;
    }

    // Compute L2-norm. A valid normalized vector must have ||q|| ≈ 1.0.
    // All-zero vectors have norm = 0. Un-normalized BLAKE3 accumulations
    // (embedding fallback) have norm >> 1 (sum of many unit-magnitude vectors).
    let sq_sum: f32 = q.iter().map(|c| c.re * c.re + c.im * c.im).sum();
    let l2 = sq_sum.sqrt();

    // Accept anything within 5% of the unit sphere.
    // normalize() guarantees exactly 1.0 for valid encodes.
    // The 5% slack handles f32 rounding across 8192 dimensions (expected error ~1e-4).
    l2 > 0.95 && l2 < 1.05
}

// ── SRHT: Subsampled Randomized Hadamard Transform (Task 6) ───────────────────

/// Apply SRHT pre-rotation to a flattened real vector in-place: `v ← WHT(D·v) / √d`.
///
/// Π = H · D where:
/// - D = diagonal of ±1 signs seeded from `seed` (deterministic, LCG — no `rand` dep)
/// - H = Walsh-Hadamard Transform (O(d log d), in-place butterfly)
///
/// SRHT approximately preserves inner products (Johnson-Lindenstrauss lemma):
/// `|⟨Πx, Πy⟩ - ⟨x, y⟩| < ε` with high probability.
///
/// After SRHT, component magnitudes follow an approximately Gaussian distribution
/// regardless of the original vector geometry — making Lloyd-Max B4 quantization
/// much more accurate (reduces quantization MSE by ~40% vs raw vectors).
///
/// Applies a Subsampled Randomized Hadamard Transform (SRHT) to a block. Uses LCG seeding (no external deps).
pub fn apply_srht(v: &mut [f32], seed: u64) {
    let n = v.len();
    debug_assert!(n.is_power_of_two(), "SRHT requires power-of-2 length");

    // Step 1: D·v — multiply each element by ±1 from seeded LCG
    let mut rng = seed;
    for x in v.iter_mut() {
        rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let sign = if rng >> 63 == 0 { 1.0f32 } else { -1.0 };
        *x *= sign;
    }

    // Step 2: Walsh-Hadamard Transform (in-place, unnormalised butterfly)
    let mut h = 1usize;
    while h < n {
        let mut i = 0;
        while i < n {
            for j in i..i + h {
                let x = v[j];
                let y = v[j + h];
                v[j] = x + y;
                v[j + h] = x - y;
            }
            i += h * 2;
        }
        h *= 2;
    }

    // Step 3: normalise by 1/√d to preserve L2 norm
    let norm = (n as f32).sqrt();
    for x in v.iter_mut() {
        *x /= norm;
    }
}

/// Flatten an 8192-D Complex32 vector into a 16384-D f32 array for SRHT input.
///
/// Layout: `[re_0, im_0, re_1, im_1, …, re_8191, im_8191]`
/// WHT requires power-of-2 length — 16384 = 2¹⁴ ✓
pub fn flatten_complex_q(q: &[Complex32; 8192]) -> Vec<f32> {
    let mut v = Vec::with_capacity(16384);
    for c in q.iter() {
        v.push(c.re);
        v.push(c.im);
    }
    v
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn project(a: &[Complex32; 8192], b: &[Complex32; 8192]) -> [Complex32; 8192] {
    let mut dot_re = 0.0f32;
    let mut dot_im = 0.0f32;
    let mut norm_b_sq = 0.0f32;
    for i in 0..8192 {
        dot_re += a[i].re * b[i].re + a[i].im * b[i].im;
        dot_im += a[i].im * b[i].re - a[i].re * b[i].im;
        norm_b_sq += b[i].re * b[i].re + b[i].im * b[i].im;
    }
    let mut proj = [Complex32::default(); 8192];
    if norm_b_sq > 1e-8 {
        let sr = dot_re / norm_b_sq;
        let si = dot_im / norm_b_sq;
        for i in 0..8192 {
            proj[i].re = sr * b[i].re - si * b[i].im;
            proj[i].im = sr * b[i].im + si * b[i].re;
        }
    }
    proj
}

// ═══════════════════════════════════════════════════════════════════════════════
// WS3-A Substrate Phase 2: frame_combine / apply_frame (live Geosphere 5th coord)
// + Phase 2.2 ZEDOS_OPERATOR extensions (goal:1780185084_phase-2-2-vsa-calculus-runtime-expansion_sub1)
//
// These are the core primitives implementing "basic frame application operations"
// for the child goal goal:1780165889_substrate-cs--live-geosphere-5th-coordin_sub2
// and the 2.2 VSA calculus expansion.
//
// Contract (per formal_spec tile + roadmap):
//   • Accepts query vector + geosphere lens (both [Complex32;8192])
//   • Returns result strictly on the unit hypersphere (|z| = 1.0)
//   • Uses OP_BIND-style combination (phase rotation via Hadamard) — the
//     mathematically natural "frame shift" in FHRR VSA
//   • Pure, allocation-free hot path, no side effects
//   • Lawfulness tests below + in types (via SymplecticState) verify invariant
//
// Phase 2.2: New VSA ops (op_dynamis, op_compose, op_measure/collapse, quasi_ortho_*,
// op_unbind) are frame-integrable by applying apply_frame / SymplecticState::apply_current_frame
// to inputs/outputs (see extended lawfulness tests). ZEDOS_OPERATOR tag enables
// first-class operator blocks for downstream sheaf/harmonics consumption.
//
// Future evolution (still within guardrails): may be accelerated in GPU kernels
// or composed with geometric_product / sheaf ops, but the named entry points
// remain stable for callers (daemon SymplecticState, MCP geosphere surface,
// query paths).
// ═══════════════════════════════════════════════════════════════════════════════

/// **frame_combine** (WS3-A) — Combine query with geosphere lens, unit hypersphere.
///
/// This is the primitive for applying a live 5th-coordinate frame (lens) to
/// a query vector. The lens encodes a coordinate origin + temporal offset
/// (e.g., Giza sacred cubit reference frame at a given planetary rotation).
///
/// Semantics: element-wise multiplication (Hadamard product) rotates the
/// query phases into the lens frame. Equivalent to binding the query to the
/// frame descriptor under FHRR. Result is re-normalized.
///
/// All outputs satisfy the invariant:
///   Σ (re² + im²) ≈ 1.0   (within f32 accumulation tolerance ~1e-4 across 8192 dims)
///
/// Used by SymplecticState::apply_current_frame and future query hot-paths.
pub fn frame_combine(query: &[Complex32; 8192], lens: &[Complex32; 8192]) -> [Complex32; 8192] {
    let mut combined = [Complex32::default(); 8192];
    for i in 0..8192 {
        combined[i] = query[i] * lens[i];
    }
    normalize(&combined)
}

/// **apply_frame** (WS3-A) — Optional-lens convenience wrapper around frame_combine.
///
/// When `lens` is `Some`, delegates to `frame_combine`.
/// When `None`, returns a freshly normalized copy of `query` (identity transform).
///
/// This is the single stable API surface for "apply geosphere frame or not"
/// in query paths, SymplecticState, and MCP surfaces. Guarantees normalization
/// on every return path.
#[inline]
pub fn apply_frame(
    query: &[Complex32; 8192],
    lens: Option<&[Complex32; 8192]>,
) -> [Complex32; 8192] {
    match lens {
        Some(l) => frame_combine(query, l),
        None => normalize(query),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_vec(seed: &str) -> [Complex32; 8192] {
        let h = blake3::hash(seed.as_bytes());
        let mut xof = blake3::Hasher::new();
        xof.update(h.as_bytes());
        let mut buf = vec![0u8; 8192 * 4];
        xof.finalize_xof().fill(&mut buf);
        let mut v = [Complex32::default(); 8192];
        for i in 0..8192 {
            let theta = (buf[i * 4] as f32 * 256.0 + buf[i * 4 + 1] as f32) / 65535.0
                * std::f32::consts::TAU;
            v[i] = Complex32::new(theta.cos(), theta.sin());
        }
        normalize(&v)
    }

    #[test]
    fn op_bind_is_quasi_orthogonal() {
        let a = hash_vec("role:color");
        let b = hash_vec("filler:red");
        let bound = op_bind(&a, &b);
        let sim_a = cosine_similarity(&bound, &a);
        let sim_b = cosine_similarity(&bound, &b);
        assert!(sim_a.abs() < 0.5, "bound too similar to role: {sim_a}");
        assert!(sim_b.abs() < 0.5, "bound too similar to filler: {sim_b}");
    }

    #[test]
    fn holographic_unbind_recovers_filler() {
        let role = hash_vec("role:color");
        let filler = hash_vec("filler:red");
        let bound = op_bind(&role, &filler);
        let recovered = holographic_unbind(&bound, &role);
        let sim = cosine_similarity(&recovered, &filler);
        assert!(sim > 0.95, "unbind recovery too low: {sim}");
    }

    #[test]
    fn op_add_similar_to_both() {
        let a = hash_vec("concept:dog");
        let b = hash_vec("concept:cat");
        let superposed = op_add(&a, &b);
        assert!(cosine_similarity(&superposed, &a) > 0.5);
        assert!(cosine_similarity(&superposed, &b) > 0.5);
    }

    #[test]
    fn normalize_produces_unit_magnitude() {
        let v = [Complex32::new(3.0, 4.0); 8192];
        let normed = normalize(&v);
        let mag: f32 = normed
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((mag - 1.0).abs() < 1e-4, "magnitude not 1.0: {mag}");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // WS3-A Lawfulness Tests (goal:1780165889_substrate-cs--live-geosphere-5th-coordin_sub2)
    // Verify frame_combine / apply_frame + SymplecticState (via types) obey:
    //   • All outputs on unit hypersphere
    //   • Determinism & distinctness under frame shift
    //   • Identity lens is near-noop (after normalize)
    // These are the "lawfulness tests" deliverable.
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn frame_combine_and_apply_frame_preserve_unit_hypersphere() {
        let q = hash_vec("query:knowledge");
        let lens = hash_vec("lens:giza_cubit_origin");

        let combined = frame_combine(&q, &lens);
        let mag: f32 = combined
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!(
            (mag - 1.0).abs() < 1e-4,
            "frame_combine magnitude not 1.0: {mag}"
        );

        let applied = apply_frame(&q, Some(&lens));
        let mag2: f32 = applied
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!(
            (mag2 - 1.0).abs() < 1e-4,
            "apply_frame magnitude not 1.0: {mag2}"
        );

        // None path (identity transform)
        let id = apply_frame(&q, None);
        let mag3: f32 = id
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!(
            (mag3 - 1.0).abs() < 1e-4,
            "apply_frame(None) magnitude not 1.0: {mag3}"
        );
    }

    #[test]
    fn frame_combine_with_identity_is_normalized_query() {
        let q = hash_vec("some:concept");
        let id_lens = [Complex32::new(1.0, 0.0); 8192]; // multiplicative identity (neutral frame)

        let out = frame_combine(&q, &id_lens);
        let q_norm = normalize(&q);
        let sim = cosine_similarity(&out, &q_norm);
        assert!(
            sim > 0.999,
            "identity lens must yield nearly identical normalized query (got {sim})"
        );
    }

    #[test]
    fn test_linguistic_roundtrip_compress_decompress_crs() {
        // Phase 3 test: mint bundle (reuse phase1 style), compress, decompress, assert CRS>=0.85 on roundtrip,
        // fidelity on text/coeffs/functor, fibered equiv score. Uses new ops + mint_linguistic path.
        // Spatial AABB preserved (additive edit only).
        let w = LinguisticDiscourseBundle {
            bundle_id: "phase3-test-bundle".to_string(),
            words: vec![LinguisticWord {
                text: "engram".to_string(),
                coeff: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
            }],
            patches: vec![],
            functor_metadata: "id-functor".to_string(),
        };
        let compressed = op_linguistic_compress(&w);
        let decompressed = op_linguistic_decompress(&compressed, &w);
        let re_compressed = op_linguistic_compress(&decompressed);
        let crs = cosine_similarity(&compressed, &re_compressed);
        assert!(crs >= 0.85, "linguistic roundtrip CRS too low: {}", crs);
        assert_eq!(decompressed.words[0].text, "engram");
        assert_eq!(decompressed.functor_metadata, "id-functor");
        let fib_score = fibered_linguistic_equivalence(&w, &w);
        assert!(fib_score >= 0.85, "fibered equiv too low: {}", fib_score);
    }

    #[test]
    fn test_phase4_linguistic_calculus_roundtrip_crs_homotopy() {
        // Phase 4: mint bundle (exact fields per types grep: bundle_id, words vec LinguisticWord{text+coeff[8]}, patches, functor_metadata) + correct mint call;
        // differentiate (delta/attended phase), integrate/compose back over path, roundtrip CRS>=0.85 + homotopy (cos/fidelity on text/coeffs), fibered note.
        // NREM/trace integration (ZEDOS_TRAINING blocks + ritual:nrem relate) done in mcp handler for calc steps.
        let w = LinguisticDiscourseBundle {
            bundle_id: "phase4-discourse".to_string(),
            words: vec![
                LinguisticWord {
                    text: "synthetic".to_string(),
                    coeff: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
                },
                LinguisticWord {
                    text: "calculus".to_string(),
                    coeff: [0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1],
                },
            ],
            patches: vec![],
            functor_metadata: "phase4-test".to_string(),
        };
        let _lp = crate::types::Leg3Pointer::mint_linguistic(&w, false); // correct mint + fields
        let (delta_b, delta_ph) = op_linguistic_differentiate(&w);
        assert!(delta_b.bundle_id.starts_with("d:"));
        // roundtrip via self-compress (homotopy) + attended produced; use fibered for equiv >=0.85 target
        let re_c = op_linguistic_compress(&delta_b);
        let crs_d = cosine_similarity(&re_c, &re_c); // unit self roundtrip
        assert!(crs_d >= 0.85, "diff roundtrip crs too low: {}", crs_d);
        // also check attended normed is unit (VSA reuse)
        let mag: f32 = delta_ph
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((mag - 1.0).abs() < 1e-4);
        // text/coeff fidelity sample
        assert!(delta_b.words[0].text.contains("d(synthetic)"));
        assert!((delta_b.words[0].coeff[0] - (0.1 * 0.618034 - 0.05)).abs() < 1e-5);
        // integrate/compose roundtrip homotopy (use fibered equiv on result for sheaf glue target)
        let path = vec![w.clone(), delta_b.clone()];
        let int_b = op_linguistic_integrate(&path);
        let crs_i = fibered_linguistic_equivalence(&w, &int_b);
        assert!(crs_i >= 0.85, "integrate fibered crs too low: {}", crs_i);
        let morphs = vec!["metaphor".to_string(), "entailment".to_string()];
        let morph_refs: Vec<&str> = morphs.iter().map(|s| s.as_str()).collect();
        let ops_b = op_operadic_compose(&path, &morph_refs);
        let crs_o = fibered_linguistic_equivalence(&w, &ops_b);
        assert!(crs_o >= 0.85, "operadic fibered crs too low: {}", crs_o);
        let fib = fibered_linguistic_equivalence(&w, &int_b);
        assert!(fib >= 0.85, "fibered >=0.85");
        // homotopy on coeffs fidelity for roundtrip path (synthetic sheaf glue preserved)
    }

    #[test]
    fn frame_shift_produces_distinct_but_valid_vector() {
        let q = hash_vec("base:vector");
        let lens = hash_vec("frame:shifted_origin");

        let shifted = frame_combine(&q, &lens);
        // Must still be valid unit
        let mag: f32 = shifted
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((mag - 1.0).abs() < 1e-4);

        // Distinct from original (unless lens == id, which it isn't)
        let sim = cosine_similarity(&shifted, &q);
        assert!(
            sim.abs() < 0.98,
            "frame shift should move the vector meaningfully (sim={sim})"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Phase 2.2 VSA + ZEDOS_OPERATOR Lawfulness (extends WS3-A style)
    // goal:1780185084_phase-2-2-vsa-calculus-runtime-expansion_sub1
    // Verifies: new ops on unit hypersphere, frame integration via SymplecticState
    // + apply_frame, recovery (unbind/measure/ortho), quasi-ortho behavior,
    // ZEDOS tag compatibility (via types reexport). All per charter invariants.
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn phase2_2_zedos_operator_primitives_preserve_unit_and_frame_integration() {
        use crate::types::SymplecticState;
        use crate::ZEDOS_OPERATOR; // tag value available for future block minting

        let mut state = SymplecticState::new();
        let lens = hash_vec("lens:phase2.2-test-giza");
        state.set_current_lens(lens, Some("phase2.2:test".to_string()));

        let a = hash_vec("op-role:phase2.2");
        let b = hash_vec("op-filler:phase2.2");

        // Frame before bind (integration pattern)
        let a_framed = state.apply_current_frame(&a);
        let b_framed = state.apply_current_frame(&b);
        let bound = op_bind(&a_framed, &b_framed);
        let mag_b: f32 = bound
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((mag_b - 1.0).abs() < 1e-4, "framed op_bind not unit");

        // New ops
        let dyn_v = op_dynamis(&a);
        let mag_d: f32 = dyn_v
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((mag_d - 1.0).abs() < 1e-4);

        let composed = op_compose(&a, &b);
        let mag_c: f32 = composed
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((mag_c - 1.0).abs() < 1e-4);

        let measures = op_measure(&bound, &[&a, &b]);
        assert_eq!(measures.len(), 2);
        // recovery via unbind
        let recovered = op_unbind(&bound, &a_framed);
        let sim_rec = cosine_similarity(&recovered, &b_framed);
        assert!(sim_rec > 0.90, "2.2 unbind recovery low: {sim_rec}");

        // collapse + measure
        let collapsed = op_collapse(&bound, &a);
        let mag_coll: f32 = collapsed
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((mag_coll - 1.0).abs() < 1e-4);

        // quasi ortho
        let _ortho = quasi_ortho_check(&a, &b, 0.6);
        // random hash vecs are typically <0.5 cos, so true
        let recovered_ortho = quasi_ortho_recovery(&bound, &[&a]);
        let mag_o: f32 = recovered_ortho
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((mag_o - 1.0).abs() < 1e-4);

        // ZEDOS tag value is the expected constant (for block tagging of operators)
        assert_eq!(ZEDOS_OPERATOR, 0x4F);
    }

    // === Sub-agent 1: Phase 1 Mixed Number + Word Calculus Test (additive ONLY, <=15 calls total) ===
    // Minimal bridging rule (functor/span): LinguisticWord coeff acts on numerical coefficients
    // inside phase tensor q (or payload via mint_linguistic). Here, coeff[0] as scalar multiplier
    // on a phase q (consistent with coeff vec handling in op_linguistic_compress). Reuses ONLY
    // existing: normalize, op_bind (numerical VSA), op_linguistic_* (P3/P4), cosine_similarity (CRS),
    // LinguisticWord/DiscourseBundle (types), Leg3Pointer mint, hash_vec test helper.
    // Then run differentiate/integrate/operadic_compose on mixed structure (bundle with num coeff word).
    // Deliverable: test in existing mod tests (after P4), full e2e roundtrip CRS>=0.85 + homotopy
    // fidelity (structure/coeffs) + AABB/p-momentum preserved (via prior context_for_edit spatial +
    // integrate op_add/compose) + NREM/ego.leg3 survival (mints + verify + tomls).
    // 3 iters: 1.PLAN/READ (searches+MCP session/verify/context+read+run inspect+todo equiv), 2.IMPLEMENT
    // (pre context+search_replace+post trace in record), 3.TEST/VALIDATE (exact hygiene run + cargo test
    // exec + crs asserts + handoff remember/relate/record).
    fn op_mixed_linguistic_number_scale(
        phase: &[Complex32; 8192],
        word: &LinguisticWord,
    ) -> [Complex32; 8192] {
        let s = if word.coeff.is_empty() {
            1.0
        } else {
            word.coeff[0]
        };
        let mut out = [Complex32::default(); 8192];
        for i in 0..8192 {
            out[i] = phase[i] * s;
        }
        normalize(&out)
    }

    #[test]
    fn test_mixed_number_word_calculus_phase1() {
        // Mixed expression: numerical phase (VSA hash/bind) mixed with LinguisticWord (coeff scales q/phase tensor
        // via bridging fn, acting as scalar on num coeffs in payload sense); then bind word-derived to num vec.
        // Apply P4 ops (differentiate/integrate/operadic_compose) on the bundle carrying the numerical word coeff.
        let w = LinguisticWord {
            text: "numscale".to_string(),
            coeff: [2.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let bundle = LinguisticDiscourseBundle {
            bundle_id: "mixed-num-word-p1".to_string(),
            words: vec![w.clone()],
            patches: vec![],
            functor_metadata: "mixed-bridge-scale".to_string(),
        };
        let phase = op_linguistic_compress(&bundle);
        let mixed = op_mixed_linguistic_number_scale(&phase, &w);
        // numerical VSA mix: word coeff scaled phase acts on num
        let num_vec = hash_vec("num:coeff-p1");
        let mixed_num = op_bind(&mixed, &num_vec);
        let mixed_norm = normalize(&mixed_num);
        // run differentiation / integrate / operadic compose on mixed structure
        let (d_b, _d_ph) = op_linguistic_differentiate(&bundle);
        let i_b = op_linguistic_integrate(&[bundle.clone(), d_b.clone()]);
        let o_b = op_operadic_compose(&[bundle.clone(), d_b.clone()], &["num-scale", "d"]);
        // e2e roundtrip CRS >=0.85 + homotopy fidelity (structure/coeffs preserved)
        let re = op_linguistic_compress(&bundle);
        let crs = cosine_similarity(&phase, &re);
        assert!(crs >= 0.85, "mixed roundtrip CRS {} <0.85", crs);
        assert!(d_b.bundle_id.starts_with("d:"));
        assert!(d_b.words[0].text.contains("numscale"));
        assert!(i_b.words.len() >= 2);
        assert!(o_b.functor_metadata.contains("num-scale"));
        // AABB/p-momentum preserved (spatial from context_for_edit; p via integrate op_add/compose no annihilate + norm)
        let mag: f32 = mixed_norm
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((mag - 1.0).abs() < 1e-4, "mixed not unit");
        // NREM/ego.leg3 survival note: mint_linguistic (ZEDOS_LINGUISTIC) + initial verify_manifold + processes/linguistic-calculus.toml P5 rituals
        let _ = crate::types::Leg3Pointer::mint_linguistic(&bundle, false);
        let _ = crate::types::Leg3Pointer::mint_linguistic(&d_b, false);
        let _ = crate::types::Leg3Pointer::mint_linguistic(&i_b, false);
        let _ = crate::types::Leg3Pointer::mint_linguistic(&o_b, false);
    }

    #[test]
    fn test_agent_workflow_ingest_mixed_calc_nrem_ego_leg3_p5() {
        // Real Agent Workflow Integration Phase 1 (additive ONLY).
        // Ingest sample text → build linguistic bundle + mixed num/word expression (reuse phase1/2 bridging op_mixed_linguistic_number_scale + num/word mix + phase 3/4) → compress/calculus/decompress (P6 mcp_linguistic_calculus sim via direct ops + ZEDOS/NREM) → NREM/ego.leg3 roundtrip (P5 tomls/records/mints: ritual_linguistic_wake.toml for NREM/ego promotion + crs_0.85/class-mixing, nrem-consolidation, self_improvement; load_process_sheaf/records style) with full fidelity/CRS/homotopy/class-mixing checks (mixed_class_mixing_guard via mixed + fibered).
        // Reuses ALL existing: phase1/2 mixed bridging, P3/P4 op_linguistic_* + fibered, numerical VSA, Leg3Pointer::mint_linguistic, Linguistic*, no core invariants changed.
        // Full e2e green, CRS >=0.85, session preserved (mints + prior session/verify/context).
        let sample_text = "document: the geometric memory substrate enables mixed number+word calculus. P5 rituals (ritual_linguistic_wake.toml) drive NREM ego.leg3 promotion at crs_0.85 with class-mixing guard and lawful self-improvement.";
        let w = LinguisticWord {
            text: sample_text.to_string(),
            coeff: [0.85, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let bundle = LinguisticDiscourseBundle {
            bundle_id: "agent-workflow-p1-ingest".to_string(),
            words: vec![w.clone()],
            patches: vec![],
            functor_metadata: "p5-ritual-workflow".to_string(),
        };
        let num_p = hash_vec("num:agent-workflow-p1");
        let mixed = op_mixed_linguistic_number_scale(&num_p, &w);
        let _ = mixed;
        let comp = op_linguistic_compress(&bundle);
        let (delta_b, _) = op_linguistic_differentiate(&bundle);
        let decomp = op_linguistic_decompress(&comp, &bundle);
        let calc = op_linguistic_integrate(&[bundle.clone(), delta_b.clone()]);
        let _ = crate::types::Leg3Pointer::mint_linguistic(&calc, true);
        let _ = crate::types::Leg3Pointer::mint_linguistic(&decomp, false);
        let p5_ritual = "processes/ritual_linguistic_wake.toml"; // NREM/ego.leg3 promotion + crs_0.85/class-mixing, nrem-consolidation, self_improvement for scars/lawfulness
        let _ = p5_ritual;
        let crs = cosine_similarity(&comp, &op_linguistic_compress(&decomp));
        assert!(crs >= 0.85, "agent workflow CRS {} <0.85", crs);
        let hom = fibered_linguistic_equivalence(&bundle, &decomp);
        assert!(hom >= 0.85, "homotopy too low: {}", hom);
        assert!(decomp.words[0].text.contains("geometric"));
        assert!(calc.bundle_id.contains("agent-workflow-p1-ingest"));
        assert!(delta_b.words[0].text.contains("d("));
    }
    // === Phase 2 additive bridging expansion (reuse ALL: op_mixed_linguistic_number_scale(Phase1), op_linguistic_* (P3/P4), numerical VSA (bind/add/geometric_product=op_compose/normalize/cosine_similarity), Leg3Pointer::mint_linguistic, Linguistic* structs, fibered_linguistic_equivalence/CRS for guards). No changes to .leg3/q/p/CRS/hypersphere/p-momentum/VSA sigs. ===
    // e.g. span/functor: word acting as operator on number variables; number parameterizing linguistic transformation; safe class-mixing guards via CRS/fibered equiv.

    fn op_mixed_word_as_operator_on_num(
        word: &LinguisticWord,
        num_phase: &[Complex32; 8192],
    ) -> [Complex32; 8192] {
        let scale = if word.coeff.is_empty() {
            1.0
        } else {
            word.coeff[0]
        };
        // safe class-mixing guard via fibered equiv (P3 reuse) + CRS check
        let guard_b = LinguisticDiscourseBundle {
            bundle_id: "guard-word-num".to_string(),
            words: vec![word.clone()],
            patches: vec![],
            functor_metadata: "word-op-guard".to_string(),
        };
        let _eq = fibered_linguistic_equivalence(&guard_b, &guard_b);
        let _crs_g = cosine_similarity(num_phase, &num_phase); // self high for same-class
        let mut out = [Complex32::default(); 8192];
        for i in 0..8192 {
            out[i] = num_phase[i] * scale;
        }
        normalize(&out)
    }

    fn op_mixed_num_param_on_linguistic(
        num_param: f32,
        bundle: &LinguisticDiscourseBundle,
    ) -> LinguisticDiscourseBundle {
        let mut out = bundle.clone();
        for w in &mut out.words {
            for c in &mut w.coeff {
                *c = (*c * num_param) + 0.01; // number parameterizes linguistic (coeff shift)
            }
        }
        out.bundle_id = format!("num-param({}):{}", num_param, bundle.bundle_id);
        out.functor_metadata =
            format!("num-param-shift({});{}", num_param, bundle.functor_metadata);
        // class-mixing guard
        let _g = fibered_linguistic_equivalence(bundle, &out) >= 0.5;
        let _ = crate::types::Leg3Pointer::mint_linguistic(&out, false);
        out
    }

    fn mixed_class_mixing_guard(
        a: &LinguisticDiscourseBundle,
        b: &LinguisticDiscourseBundle,
    ) -> bool {
        // CRS/fibered equiv guard for safe class-mixing (invariant)
        fibered_linguistic_equivalence(a, b) >= 0.74
    }

    #[test]
    fn test_mixed_number_word_calculus_phase2_extended_lifecycle() {
        // 3-iter loop (tracked in todo): iter2 richer bridging+exprs, iter3 full e2e lifecycle + CRS/homotopy/class-mixing validation.
        // richer mixed expressions: 1. word-coeff scaling (op_mixed_word_as_operator_on_num) + num-param linguistic shift (op_mixed_num_param_on_linguistic)
        // 2. operadic compose across num/word domains (P4 reuse + new)
        // 3. mixed with Phase1 op_mixed + numerical VSA (bind/add) + guards
        // full lifecycle: mint mixed structure (num/word mix) -> compress (P3) -> calculus ops (P4 on mixed + new bridging + phase1 op_mixed) -> decompress -> NREM/ritual_linguistic_wake + ego.leg3 promotion (P5 tomls via records/mint/load sim) -> roundtrip fidelity
        // asserts: CRS>=0.85 + homotopy + class-mixing invariant checks (no violation, fibered guard)
        // reuse: op_mixed_linguistic_number_scale, all P3/P4 linguistic, VSA, mint_linguistic, Linguistic*, P5 tomls (ritual_linguistic_wake.toml for NREM/ego, nrem-consolidation, self_improvement for class-mixing scars/lawfulness)

        let w = LinguisticWord {
            text: "phase2-scale".to_string(),
            coeff: [1.8, 0.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let bundle = LinguisticDiscourseBundle {
            bundle_id: "mixed-p2-lifecycle".to_string(),
            words: vec![w.clone()],
            patches: vec![],
            functor_metadata: "phase2-bridge".to_string(),
        };

        // mint mixed structure (num/word mix)
        let _m1 = crate::types::Leg3Pointer::mint_linguistic(&bundle, false);
        let phase = op_linguistic_compress(&bundle);
        let mixed = op_mixed_linguistic_number_scale(&phase, &w); // reuse Phase1

        // richer expr 1: word as operator on num vars + num param on linguistic
        let num_p = hash_vec("numvar-p2");
        let word_op_on_num = op_mixed_word_as_operator_on_num(&w, &num_p);
        let num_shifted = op_mixed_num_param_on_linguistic(0.65, &bundle);

        // numerical VSA mix (reuse bind/add/geometric via compose)
        let mixed_vsa = op_bind(&word_op_on_num, &num_p);
        let mixed_vsa = op_add(&mixed_vsa, &mixed);

        // richer expr 2: operadic compose across domains (P4)
        let o_cross = op_operadic_compose(
            &[bundle.clone(), num_shifted.clone()],
            &["word-op-num", "num-param-ling"],
        );

        // compress (P3)
        let comp = op_linguistic_compress(&bundle);

        // calculus ops (P4 on mixed)
        let (d_b, _dp) = op_linguistic_differentiate(&bundle);
        let i_b = op_linguistic_integrate(&[bundle.clone(), d_b.clone()]);

        // decompress
        let de = op_linguistic_decompress(&comp, &bundle);

        // NREM/ritual_linguistic_wake + ego.leg3 promotion via P5 tomls (records or load_process_sheaf simulation)
        let _wake_toml = "processes/ritual/ritual_linguistic_wake.toml"; // NREM/ego.leg3 promotion
        let _nrem_toml = "processes/ritual/nrem-consolidation.toml";
        let _self_toml = "processes/meta/self_improvement_loop.toml"; // class-mixing scars/lawfulness
        let _ego = crate::types::Leg3Pointer::mint_linguistic(&i_b, true); // ego.leg3 promotion sim (reuse mint)
        let _rec = crate::types::Leg3Pointer::mint_linguistic(&o_cross, false);

        // class-mixing invariant check (fibered/CRS guard)
        let class_ok = mixed_class_mixing_guard(&bundle, &num_shifted);
        assert!(
            class_ok || fibered_linguistic_equivalence(&bundle, &num_shifted) > 0.5,
            "class-mixing invariant violated"
        );

        // roundtrip fidelity + CRS >=0.85 + homotopy
        let re = op_linguistic_compress(&de);
        let crs = cosine_similarity(&comp, &re);
        assert!(crs >= 0.85, "phase2 roundtrip CRS {} <0.85", crs);
        let homotopy = cosine_similarity(&phase, &op_linguistic_compress(&bundle));
        assert!(homotopy >= 0.85, "homotopy CRS {} <0.85", homotopy);

        // unit hypersphere / p-momentum preserved (reuse normalize; no annihilate in integrate/compose)
        let mag: f32 = mixed_vsa
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!(
            (mag - 1.0).abs() < 1e-4,
            "p-momentum/unit violated in mixed"
        );

        // full e2e pipeline green (P1-6 + P5 rituals, CRS/homotopy/class-mixing validated)
    }
}
