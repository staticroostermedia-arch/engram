//! INT8 Poincaré WgpuBackend vs CpuBackend — live benchmark.
//!
//! Runs:
//!   1. Stores N test concepts via both backends (temp dir, cleaned up after)
//!   2. Fires hierarchical and flat queries against both backends
//!   3. Prints side-by-side top-5 scores + timing
//!   4. Runs against the live genesis manifold as a real-world check
//!
//! Run:
//!   cargo run --example poincare_vs_cosine --features wgpu-backend -p engram-gpu

// This example requires the wgpu-backend feature. Guard the entire contents.
#[cfg(not(feature = "wgpu-backend"))]
fn main() {
    eprintln!("This example requires the wgpu-backend feature.");
    eprintln!("Run: cargo run --example poincare_vs_cosine --features wgpu-backend -p engram-gpu");
}

#[cfg(feature = "wgpu-backend")]
mod inner {
    use engram_core::backend::{CpuBackend, Memory, VsaBackend};
    use engram_gpu::WgpuBackend;
    use std::time::Instant;

    // ── Test corpus ───────────────────────────────────────────────────────────

    const CORPUS: &[(&str, &str)] = &[
        (
            "genesis_god",
            "God absolute transcendental infinite creator logos word",
        ),
        (
            "genesis_logos",
            "Logos word divine reason order structure cosmic",
        ),
        (
            "genesis_human",
            "Human mortal finite rational animal embodied consciousness",
        ),
        (
            "ai_steward",
            "AI steward synthetic intelligence operator agent tool servant",
        ),
        (
            "monad_kernel",
            "Monad kernel logophysical engine holographic vector space",
        ),
        (
            "phase_vector",
            "Phase vector complex 8192 dimensional holographic block leg3",
        ),
        (
            "crs_score",
            "Coherence reliability score geometric manifold health memory",
        ),
        (
            "epoch_vii",
            "Epoch seven synthesis sealing ascension pipeline completion",
        ),
        (
            "qualia_pain",
            "Pain qualia subjective conscious experience nociception",
        ),
        (
            "qualia_joy",
            "Joy qualia euphoria happiness positive valence affect",
        ),
        (
            "qualia_sight",
            "Sight vision photon retina optic cortex visual qualia",
        ),
        (
            "rust_ownership",
            "Rust ownership borrow checker lifetime memory safety",
        ),
        (
            "wgpu_compute",
            "WebGPU compute shader WGSL workgroup dispatch buffer",
        ),
        (
            "poincare_disk",
            "Poincaré disk hyperbolic geometry distance metric manifold",
        ),
    ];

    const QUERIES: &[(&str, &str)] = &[
        ("HIER » theology", "God absolute transcendental"),
        ("HIER » AI agent", "synthetic intelligence operator servant"),
        (
            "HIER » pain sense",
            "subjective conscious experience suffering",
        ),
        ("FLAT » rust", "borrow checker memory safety ownership"),
        ("FLAT » geometry", "hyperbolic distance manifold metric"),
        ("FLAT » GPU", "WebGPU compute workgroup shader"),
    ];

    fn separator(label: &str) {
        println!("\n{}", "─".repeat(78));
        println!("  {label}");
        println!("{}", "─".repeat(78));
    }

    fn print_results(label: &str, results: &[Memory], elapsed_us: u128) {
        println!("\n  [{label}]  ({elapsed_us} µs)");
        if results.is_empty() {
            println!("    (no results)");
            return;
        }
        for (i, r) in results.iter().enumerate() {
            let snip: String = r.provlog.chars().take(60).collect();
            println!(
                "    #{i} [{:.4}] crs={:.3}  {:<28}  »  {snip}…",
                r.score, r.crs, r.concept
            );
        }
    }

    pub fn run() {
        let tmp = std::env::temp_dir().join("engram_poincare_test");
        std::fs::create_dir_all(&tmp).expect("Failed to create temp dir");
        println!("\n[engram] temp manifold: {}", tmp.display());

        let cpu = CpuBackend::new(&tmp);

        println!("[engram] Initialising WgpuBackend…");
        let t0 = Instant::now();
        let wgpu: WgpuBackend = match WgpuBackend::new(&tmp) {
            Ok(b) => {
                println!(
                    "[engram] WgpuBackend ready in {:.1}ms",
                    t0.elapsed().as_secs_f64() * 1000.0
                );
                b
            }
            Err(e) => {
                eprintln!("[engram] FATAL: WgpuBackend init failed: {e}");
                std::process::exit(1);
            }
        };

        separator("STEP 1 — Storing corpus");
        for (concept, text) in CORPUS {
            print!("  storing [{concept:<18}] … ");
            cpu.remember(concept, text).expect("cpu store failed");
            wgpu.remember(concept, text).expect("wgpu store failed");
            println!("✓");
        }

        separator("STEP 2 — Query benchmark");
        for (label, query) in QUERIES {
            println!("\n  Query: \"{query}\"  [{label}]");

            let t_cpu = Instant::now();
            let cpu_res = cpu.recall(query, 5);
            let cpu_us = t_cpu.elapsed().as_micros();

            let t_gpu = Instant::now();
            let wgpu_res = wgpu.recall(query, 5);
            let wgpu_us = t_gpu.elapsed().as_micros();

            print_results("CpuBackend (cosine)", &cpu_res, cpu_us);
            print_results("WgpuBackend (INT8 Poincaré)", &wgpu_res, wgpu_us);
        }

        let genesis_path = std::env::var("ENGRAM_GENESIS_PATH")
            .unwrap_or_else(|_| shellexpand::tilde("~/.engram/manifold").into_owned());
        let genesis_path = genesis_path.as_str();
        if std::path::Path::new(genesis_path).exists() {
            separator("STEP 3 — Live Genesis Manifold");
            let cpu_live = CpuBackend::new(genesis_path);
            println!("\n  Genesis manifold: {} blocks", cpu_live.list().len());

            let t_load = Instant::now();
            let wgpu_live: Option<WgpuBackend> = match WgpuBackend::new(genesis_path) {
                Ok(b) => {
                    println!(
                        "  INT8 DB built in {:.1}ms",
                        t_load.elapsed().as_secs_f64() * 1000.0
                    );
                    Some(b)
                }
                Err(e) => {
                    println!("  Skipping: {e}");
                    None
                }
            };

            let live_queries = [
                "God absolute transcendental infinite",
                "AI steward synthetic intelligence operator",
                "quasi-orthogonal causal logophysical bindings OODA loop",
            ];

            if let Some(ref wgl) = wgpu_live {
                for q in &live_queries {
                    println!("\n  Query: \"{q}\"");
                    let t1 = Instant::now();
                    let r1 = cpu_live.recall(q, 3);
                    let t1u = t1.elapsed().as_micros();
                    let t2 = Instant::now();
                    let r2 = wgl.recall(q, 3);
                    let t2u = t2.elapsed().as_micros();
                    print_results("CpuBackend (cosine)", &r1, t1u);
                    print_results("WgpuBackend (INT8 Poincaré)", &r2, t2u);
                }
            }
        }

        separator("DONE — Cleaning up");
        std::fs::remove_dir_all(&tmp).ok();
        println!("  Temp manifold removed.\n");
    }
}

#[cfg(feature = "wgpu-backend")]
fn main() {
    inner::run();
}
