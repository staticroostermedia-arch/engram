use num_complex::Complex32;

const D_SQRT: f32 = 90.509_67; // √8192

const CENTROIDS_B4: [f32; 16] = [
    -2.7326 / D_SQRT, -2.0690 / D_SQRT, -1.5184 / D_SQRT, -1.0563 / D_SQRT,
    -0.6560 / D_SQRT, -0.2872 / D_SQRT,  0.0000 / D_SQRT,  0.0000 / D_SQRT,
     0.2872 / D_SQRT,  0.6560 / D_SQRT,  1.0563 / D_SQRT,  1.5184 / D_SQRT,
     2.0690 / D_SQRT,  2.7326 / D_SQRT,  0.0000 / D_SQRT,  0.0000 / D_SQRT,
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

        dot += q_re * p_re + q_im * p_im;
        norm_q += q_re * q_re + q_im * q_im;
        norm_p += p_re * p_re + p_im * p_im;
    }

    if norm_q <= 0.0 || norm_p <= 0.0 {
        return 0.0;
    }
    dot / (norm_q.sqrt() * norm_p.sqrt())
}
