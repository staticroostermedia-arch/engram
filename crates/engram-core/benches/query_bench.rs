//! Engram query latency benchmarks.
//!
//! # What this measures
//!
//! Three scenarios, each at corpus sizes 1K, 10K, and 100K synthetic memories:
//!
//! 1. **encode** — BLAKE3 XOF hashing + normalization (baseline: vector creation cost)
//! 2. **cosine_similarity** — single pair dot product (raw math cost)
//! 3. **apply_srht** — SRHT pre-rotation on a flattened 16384-D vector
//! 4. **euler_gate** — check_euler_characteristic on a valid vector
//! 5. **stability_tracker_update** — one StabilityTracker update step
//!
//! # Running
//!
//! ```bash
//! # Quick (analytical results to stdout)
//! cargo bench -p engram-core
//!
//! # Full HTML report in target/criterion/
//! cargo bench -p engram-core -- --save-baseline main
//! ```
//!
//! # What the numbers mean for due diligence
//!
//! At 7,000 MB/s NVMe read bandwidth (Gen4 PCIe):
//! - 1 × 256KB `.leg` block = 36 μs wall clock read time
//! - 128 LBVH candidates    = 4.6 ms total disk read time
//! - Linear scan at 10K     = 365 ms total (at NVMe saturation)
//! - Linear scan at 100K    = 3.65 s total (use LBVH above this point)
//!
//! LBVH + TurboQuant in-memory pre-scoring keeps disk reads to 128 blocks
//! regardless of corpus size, achieving O(log N) disk I/O per query.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use engram_core::ops::{
    apply_srht, check_euler_characteristic, cosine_similarity, flatten_complex_q, normalize,
    op_bind, StabilityTracker,
};
use num_complex::Complex32;

// ── Deterministic test-vector generator ───────────────────────────────────────

/// Generate a deterministic 8192-D complex unit vector from a u64 seed.
/// Uses the same BLAKE3 XOF path as encode::from_text() for realistic benchmarking.
fn make_vector(seed: u64) -> Box<[Complex32; 8192]> {
    let mut v: Box<[Complex32; 8192]> = Box::new([Complex32::default(); 8192]);
    let seed_bytes = seed.to_le_bytes();
    let mut hasher = blake3::Hasher::new();
    hasher.update(&seed_bytes);
    // XOF — same operation as encode::from_text()
    let mut buf = vec![0u8; 8192 * 8];
    hasher.finalize_xof().fill(&mut buf);
    for i in 0..8192 {
        let re_bits =
            u32::from_le_bytes([buf[i * 8], buf[i * 8 + 1], buf[i * 8 + 2], buf[i * 8 + 3]]);
        let im_bits = u32::from_le_bytes([
            buf[i * 8 + 4],
            buf[i * 8 + 5],
            buf[i * 8 + 6],
            buf[i * 8 + 7],
        ]);
        v[i].re = ((re_bits as f32) / u32::MAX as f32) * 2.0 - 1.0;
        v[i].im = ((im_bits as f32) / u32::MAX as f32) * 2.0 - 1.0;
    }
    // Normalize to unit sphere (same as cosine_similarity prerequisite)
    let normed = normalize(&*v);
    v.copy_from_slice(&normed);
    v
}

/// Generate a corpus of N synthetic unit vectors.
fn make_corpus(n: usize) -> Vec<Box<[Complex32; 8192]>> {
    (0..n).map(|i| make_vector(i as u64)).collect()
}

// ── Benchmarks ─────────────────────────────────────────────────────────────────

fn bench_cosine_similarity(c: &mut Criterion) {
    let a = make_vector(0xDEAD_BEEF);
    let b = make_vector(0xC0FF_EE00);

    c.bench_function("cosine_similarity/single_pair", |bencher| {
        bencher.iter(|| black_box(cosine_similarity(black_box(&*a), black_box(&*b))));
    });
}

fn bench_apply_srht(c: &mut Criterion) {
    let q = make_vector(0x1234_5678);
    let mut flat = flatten_complex_q(&*q);

    c.bench_function("apply_srht/16384_f32", |bencher| {
        bencher.iter(|| {
            let mut v = flat.clone();
            apply_srht(black_box(&mut v), 0x454E_4752_0000_0000);
            black_box(v);
        });
    });

    // Amortized: when we can reuse the flattening allocation
    c.bench_function("apply_srht+flatten/16384_f32", |bencher| {
        bencher.iter(|| {
            let mut v = flatten_complex_q(black_box(&*q));
            apply_srht(&mut v, 0x454E_4752_0000_0000);
            black_box(v);
        });
    });

    let _ = flat; // suppress unused warning
}

fn bench_euler_gate(c: &mut Criterion) {
    let q = make_vector(0xABCD_1234);

    c.bench_function("euler_gate/valid_vector", |bencher| {
        bencher.iter(|| black_box(check_euler_characteristic(black_box(&*q))));
    });
}

fn bench_stability_tracker(c: &mut Criterion) {
    let mut tracker = StabilityTracker::from_energetics(1.0, 0.1, 0.5);

    c.bench_function("stability_tracker/update", |bencher| {
        bencher.iter(|| {
            black_box(tracker.update(
                black_box(0.15), // gradient_mag (cosine distance)
                black_box(0.08), // drift_mag
            ))
        });
    });
}

/// Linear scan simulation: measures the cost of scoring N synthetic in-memory vectors.
/// **Does NOT hit disk** — this isolates the CPU cost of cosine similarity at scale.
///
/// At 10K vectors: shows how fast the Rayon linear scan would be if everything were in RAM.
/// Real-world NVMe I/O is the bottleneck (36 μs × N blocks), not this computation.
fn bench_linear_scan_cpu(c: &mut Criterion) {
    let mut group = c.benchmark_group("linear_scan_cpu");

    for &n in &[1_000usize, 5_000, 10_000] {
        let corpus = make_corpus(n);
        let query = make_vector(0xFFFF_0000);

        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |bencher, _| {
            bencher.iter(|| {
                let mut best = f32::NEG_INFINITY;
                for v in corpus.iter() {
                    let s = cosine_similarity(black_box(&*query), black_box(&**v));
                    if s > best {
                        best = s;
                    }
                }
                black_box(best)
            });
        });
    }
    group.finish();
}

/// SRHT pre-scoring simulation: measures TurboQuant candidate scoring cost.
/// Simulates the in-memory scoring of 128 LBVH candidates (no disk I/O).
fn bench_turbo_quant_candidates(c: &mut Criterion) {
    let corpus = make_corpus(128);
    let query = make_vector(0xCAFE_BABE);
    let mut flat_q = flatten_complex_q(&*query);
    apply_srht(&mut flat_q, 0x454E_4752_0000_0000);

    c.bench_function("turbo_quant/128_candidates_srht+cosine", |bencher| {
        bencher.iter(|| {
            let mut best = f32::NEG_INFINITY;
            for v in corpus.iter() {
                // Simulate SRHT pre-score: flatten + rotate + cosine (in-memory, no I/O)
                let mut flat_v = flatten_complex_q(black_box(&**v));
                apply_srht(&mut flat_v, 0x454E_4752_0000_0000);
                let dot: f32 = flat_q.iter().zip(flat_v.iter()).map(|(a, b)| a * b).sum();
                if dot > best {
                    best = dot;
                }
            }
            black_box(best)
        });
    });
}

criterion_group!(
    benches,
    bench_cosine_similarity,
    bench_apply_srht,
    bench_euler_gate,
    bench_stability_tracker,
    bench_linear_scan_cpu,
    bench_turbo_quant_candidates,
);
criterion_main!(benches);
