//! Engram Backfill — re-embed all existing .leg blocks with the current embedding server.
//!
//! Reads every `.leg` file in the manifold, extracts the ProvLog text, calls the
//! embedding server to get a fresh neural vector, writes it cleanly into q[0..N].re,
//! and re-saves the block in-place.
//!
//! Blocks that already have a non-zero embedding (q[0].re != 0.0) are skipped
//! unless --force is passed.
//!
//! # Usage
//!
//!   # Standard (skip already-embedded blocks)
//!   cargo run --bin engram-backfill -- ~/.engram/manifold
//!
//!   # Force re-embed everything
//!   cargo run --bin engram-backfill -- ~/.engram/manifold --force
//!
//!   # Point at an alternate embedding server
//!   ENGRAM_EMBED_URL=http://localhost:8086/v1/embeddings \
//!     cargo run --bin engram-backfill -- ~/.engram/manifold

use engram_core::storage::{read_block, read_provlog, write_block};
use std::path::Path;

const DEFAULT_EMBED_URL: &str = "http://localhost:8085/v1/embeddings";

fn embed(text: &str, url: &str) -> Option<Vec<f32>> {
    use serde_json::json;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let body = json!({ "input": text, "model": "local" });

    let res: serde_json::Value = client.post(url).json(&body).send().ok()?.json().ok()?;

    // Handle error responses gracefully
    if res.get("error").is_some() {
        eprintln!(
            "  [skip] Server error: {}",
            res["error"]["message"].as_str().unwrap_or("unknown")
        );
        return None;
    }

    let emb = res["data"][0]["embedding"].as_array()?;
    let vec: Vec<f32> = emb
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();

    if vec.is_empty() {
        return None;
    }

    // L2-normalise
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1e-9 {
        return None;
    }
    Some(vec.into_iter().map(|x| x / norm).collect())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let manifold_path = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("~/.engram/manifold");
    let force = args.contains(&"--force".to_string());

    let expanded = if manifold_path.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        manifold_path.replacen('~', &home, 1)
    } else {
        manifold_path.to_string()
    };

    let manifold = Path::new(&expanded);

    if !manifold.exists() {
        eprintln!(
            "Error: manifold directory not found: {}",
            manifold.display()
        );
        std::process::exit(1);
    }

    let embed_url =
        std::env::var("ENGRAM_EMBED_URL").unwrap_or_else(|_| DEFAULT_EMBED_URL.to_string());

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║          ENGRAM BACKFILL — INT8 Poincaré Prep            ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!("  Manifold : {}", manifold.display());
    println!("  Embed URL: {embed_url}");
    println!(
        "  Mode     : {}",
        if force {
            "FORCE (re-embed all)"
        } else {
            "SKIP (skip already-embedded)"
        }
    );
    println!();

    // Probe the server first
    print!("  Probing embedding server… ");
    match embed("ping", &embed_url) {
        Some(v) => println!("✓ ({} dims)", v.len()),
        None => {
            println!("✗");
            eprintln!("\n  ERROR: Embedding server not reachable at {embed_url}");
            eprintln!("  Start a server with:");
            eprintln!("    llama-server -m /path/to/embedding.gguf --port 8086 --embeddings --pooling mean");
            eprintln!("  Or set: export ENGRAM_EMBED_URL=http://...");
            std::process::exit(1);
        }
    }

    // Collect .leg files
    let entries: Vec<_> = std::fs::read_dir(manifold)
        .expect("Failed to read manifold dir")
        .flatten()
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("leg"))
        .collect();

    println!("\n  Found {} .leg blocks\n", entries.len());

    let mut n_skipped = 0usize;
    let mut n_embedded = 0usize;
    let mut n_failed = 0usize;

    for entry in &entries {
        let path = entry.path();
        let concept = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();

        // Read the block
        let mut block = match read_block(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  [FAIL] {concept}: {e}");
                n_failed += 1;
                continue;
            }
        };

        // Check if already embedded (q[0].re != 0.0 means a neural vector is present)
        let already_embedded = block.q[0].re.abs() > 1e-6 && block.q[0].im.abs() < 1e-6;
        if already_embedded && !force {
            println!("  [skip] {concept:<30}  (q[0].re={:.4})", block.q[0].re);
            n_skipped += 1;
            continue;
        }

        // Extract ProvLog text
        let text = read_provlog(&block);
        if text.is_empty() {
            println!("  [skip] {concept:<30}  (empty provlog)");
            n_skipped += 1;
            continue;
        }

        // Embed
        print!("  [embed] {concept:<30}  … ");
        match embed(&text, &embed_url) {
            Some(vec) => {
                let dim = vec.len().min(8192);
                // Place cleanly into q[0..dim].re
                for (i, &v) in vec.iter().enumerate().take(dim) {
                    block.q[i].re = v;
                    block.q[i].im = 0.0;
                }
                // Write back
                match write_block(&path, &block) {
                    Ok(_) => {
                        println!("✓  ({dim}D → q[0..{dim}].re)");
                        n_embedded += 1;
                    }
                    Err(e) => {
                        eprintln!("WRITE FAIL: {e}");
                        n_failed += 1;
                    }
                }
            }
            None => {
                println!("✗  (embed failed)");
                n_failed += 1;
            }
        }
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Embedded : {n_embedded}");
    println!("  Skipped  : {n_skipped}");
    println!("  Failed   : {n_failed}");
    println!("  Total    : {}", entries.len());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("  Run the Poincaré benchmark now:");
    println!(
        "  cargo run --example poincare_vs_cosine --features engram-gpu/wgpu-backend -p engram-gpu"
    );
}
