use num_complex::Complex32;

const D_SQRT: f32 = 90.509_67; // √8192

const CENTROIDS_B4: [f32; 16] = [
    -2.7326 / D_SQRT,
    -2.0690 / D_SQRT,
    -1.5184 / D_SQRT,
    -1.0563 / D_SQRT,
    -0.6560 / D_SQRT,
    -0.2872 / D_SQRT,
    0.0000 / D_SQRT,
    0.0000 / D_SQRT,
    0.2872 / D_SQRT,
    0.6560 / D_SQRT,
    1.0563 / D_SQRT,
    1.5184 / D_SQRT,
    2.0690 / D_SQRT,
    2.7326 / D_SQRT,
    0.0000 / D_SQRT,
    0.0000 / D_SQRT,
];

#[inline(always)]
fn nearest_centroid_b4(val: f32) -> u8 {
    let mut best_idx = 0u8;
    let mut best_d = f32::INFINITY;
    for (i, &centroid) in CENTROIDS_B4.iter().enumerate() {
        let d = (val - centroid).abs();
        if d < best_d {
            best_d = d;
            best_idx = i as u8;
        }
    }
    best_idx
}

/// Compress an 8192-D Complex32 vector into 8192 bytes (2 x 4-bit values per byte, for real and imaginary parts).
pub fn quantize_b4(q: &[Complex32; 8192]) -> Vec<u8> {
    let mut packed = vec![0u8; 8192];
    for i in 0..8192 {
        let re_idx = nearest_centroid_b4(q[i].re);
        let im_idx = nearest_centroid_b4(q[i].im);
        packed[i] = (re_idx << 4) | (im_idx & 0x0F);
    }
    packed
}

/// Compute approximate cosine similarity directly between an unquantized query and a quantized vector.
pub fn cosine_similarity_quantized(q: &[Complex32; 8192], packed: &[u8]) -> f32 {
    let mut dot = 0f32;
    let mut norm_q = 0f32;
    let mut norm_p = 0f32;

    for i in 0..8192 {
        let b = packed[i];
        let p_re = CENTROIDS_B4[(b >> 4) as usize];
        let p_im = CENTROIDS_B4[(b & 0x0F) as usize];

        let q_re = q[i].re;
        let q_im = q[i].im;

        dot += q_re * p_re + q_im * p_im; // dot(q, p) in complex inner product
        norm_q += q_re * q_re + q_im * q_im;
        norm_p += p_re * p_re + p_im * p_im;
    }

    if norm_q <= 0.0 || norm_p <= 0.0 {
        return 0.0;
    }
    dot / (norm_q.sqrt() * norm_p.sqrt())
}

// ── INT8 Poincaré Quantization (Randall / Command Center, April 2026) ─────────
//
// Scheme: symmetric affine quantization, zero-point = 0, scale = 127.0.
// Source: 384-dim f32 from the first 384 `.re` components of the 8192-D block
//         (slots where MiniLM all-MiniLM-L6-v2 ONNX output is stored).
// The WGSL shader dequantizes in-place: f32 = i8 / 127.0.

/// Pack 384 signed i8 values into 96 u32 values (little-endian, 4 bytes per u32).
///
/// This is the exact GPU upload format expected by `int8_raytracer.wgsl` binding 0/1.
///
/// From Randall's `embeddings.rs` (ported verbatim):
/// ```text
/// b0 = byte 0 of u32 (bits 0-7)
/// b1 = byte 1 of u32 (bits 8-15)
/// b2 = byte 2 of u32 (bits 16-23)
/// b3 = byte 3 of u32 (bits 24-31)
/// ```
pub fn pack_int8_to_u32(quantized: &[i8; 384]) -> [u32; 96] {
    let mut packed = [0u32; 96];
    for i in 0..96 {
        let b0 = (quantized[i * 4] as u8) as u32;
        let b1 = ((quantized[i * 4 + 1] as u8) as u32) << 8;
        let b2 = ((quantized[i * 4 + 2] as u8) as u32) << 16;
        let b3 = ((quantized[i * 4 + 3] as u8) as u32) << 24;
        packed[i] = b0 | b1 | b2 | b3;
    }
    packed
}

/// Quantize the first 384 real components of an 8192-D Complex32 block to INT8.
///
/// Returns the raw `[i8; 384]`, the GPU-packed `[u32; 96]`, and the L2 norm
/// of the quantized vector (for optional diagnostic use).
///
/// Scheme (from Randall's `embeddings.rs:77-88`):
/// - `q_int8[i] = (centroid[i].re * 127.0).round().clamp(-128, 127) as i8`
/// - The `.re` slots 0..384 contain the L2-normalised MiniLM embedding.
/// - The remaining 7808 dimensions are ignored by the INT8 path.
pub fn quantize_centroid_int8(centroid: &[Complex32; 8192]) -> ([i8; 384], [u32; 96], f32) {
    let mut q_int8 = [0i8; 384];
    let mut norm_sq = 0.0f32;

    for i in 0..384 {
        let val = (centroid[i].re * 127.0).round().clamp(-128.0, 127.0) as i8;
        q_int8[i] = val;
        norm_sq += (val as f32) * (val as f32);
    }

    let q_packed = pack_int8_to_u32(&q_int8);
    (q_int8, q_packed, norm_sq.sqrt())
}

// ── TurboQuant: SRHT + Lloyd-Max B4 (Task 6) ─────────────────────────────────

/// GENESIS_SEED matches engram-core/src/index.rs and arkade_8k.cu.
const SRHT_SEED: u64 = 0x454E_4752_0000_0000;

/// Compress with SRHT pre-rotation + B4 Lloyd-Max quantization.
///
/// SRHT: `v ← WHT(D·v) / √d` — Gaussianizes component distribution so the
/// Lloyd-Max B4 centroids (optimal for N(0,1/d)) are maximally accurate.
/// Reduces per-coordinate quantization MSE by ~40% vs raw B4.
/// Output: 8192-byte packed vector (same layout as `quantize_b4`).
pub fn quantize_srht_b4(q: &[Complex32; 8192]) -> Vec<u8> {
    let mut flat = engram_core::ops::flatten_complex_q(q);
    engram_core::ops::apply_srht(&mut flat, SRHT_SEED);
    let mut packed = vec![0u8; 8192];
    for i in 0..8192 {
        let re_idx = nearest_centroid_b4(flat[i * 2]);
        let im_idx = nearest_centroid_b4(flat[i * 2 + 1]);
        packed[i] = (re_idx << 4) | (im_idx & 0x0F);
    }
    packed
}

/// Cosine similarity between an unquantized query and an SRHT+B4 packed vector.
/// Applies the same SRHT rotation to the query — preserving geometric consistency.
pub fn cosine_similarity_srht_b4(q: &[Complex32; 8192], packed: &[u8]) -> f32 {
    let mut flat = engram_core::ops::flatten_complex_q(q);
    engram_core::ops::apply_srht(&mut flat, SRHT_SEED);
    let mut dot = 0f32;
    let mut norm_q = 0f32;
    let mut norm_p = 0f32;
    for i in 0..8192 {
        let b = packed[i];
        let p_re = CENTROIDS_B4[(b >> 4) as usize];
        let p_im = CENTROIDS_B4[(b & 0x0F) as usize];
        let q_re = flat[i * 2];
        let q_im = flat[i * 2 + 1];
        dot += q_re * p_re + q_im * p_im;
        norm_q += q_re * q_re + q_im * q_im;
        norm_p += p_re * p_re + p_im * p_im;
    }
    if norm_q <= 0.0 || norm_p <= 0.0 {
        return 0.0;
    }
    dot / (norm_q.sqrt() * norm_p.sqrt())
}
