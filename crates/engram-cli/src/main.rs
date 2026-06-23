use clap::{Parser, Subcommand};
use engram_core::backend::{CpuBackend, VsaBackend};

#[derive(Parser)]
#[command(
    name = "engram",
    about = "Persistent geometric memory for AI agents",
    version
)]
struct Cli {
    /// Directory to store .leg memory blocks
    #[arg(long, default_value = "~/.engram/manifold", env = "ENGRAM_STORE")]
    store: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Store a concept in persistent memory
    Remember {
        /// Concept identifier (e.g. "krebs_cycle")
        concept: String,
        /// Text to encode and store
        text: String,
    },
    /// Find semantically similar memories
    Recall {
        /// Query text
        query: String,
        /// Number of results to return
        #[arg(short, default_value_t = 5)]
        k: usize,
    },
    /// Delete a concept from memory
    Forget { concept: String },
    /// List all stored concepts
    List,
    /// Recursively ingest a directory of text/code files
    Ingest {
        /// Directory path to ingest
        path: String,
        /// Maximum characters per chunk (default 8000)
        #[arg(long, default_value_t = 8000)]
        chunk_size: usize,
    },
    /// Distill episodic memories into crystallized praxis blocks.
    ///
    /// Groups stored memories into clusters by CRS score, computes the geometric
    /// centroid of each cluster via bundle superposition, and mints the result
    /// as a ZEDOS_PRAXIS block pinned at CRS=1.0.
    ///
    /// Run this periodically to compress accumulated episodic noise into
    /// durable learned patterns. Use `forget-old` afterward to clean up
    /// the raw episodic blocks that have now been distilled.
    Distill {
        /// Memories per centroid cluster (default: 20)
        #[arg(long, default_value_t = 20)]
        cluster_size: usize,
        /// Skip memories with CRS below this threshold (default: 0.50)
        #[arg(long, default_value_t = 0.50)]
        min_crs: f32,
        /// Preview what would be minted without writing anything
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Perform logophysical geometry calculations over memory vectors
    Trace {
        /// First concept name or quote
        term_a: String,
        /// Operator (ADD or BIND)
        op: String,
        /// Second concept name or quote
        term_b: String,
        /// Number of results to return
        #[arg(short, default_value_t = 5)]
        k: usize,
    },
    /// Verify the agent's connection and system health
    /// Verify the agent's connection and system health
    VerifyAgent,
    /// Print manifold health statistics
    Stats,
    /// Export the manifold (or a filtered subset) as a portable JSON array
    Export {
        /// Only export memories with CRS >= this value (default: 0.0)
        #[arg(long, default_value_t = 0.0)]
        min_crs: f32,
    },
    /// Restore memories from a JSON array exported by `export`
    Import {
        /// Path to the JSON file
        file: String,
    },
    /// Track a user interaction in the persistent User Model (Phase E.4)
    TrackUser {
        /// The interaction text to track
        interaction: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let store_path = shellexpand::tilde(&cli.store).into_owned();
    let backend = CpuBackend::new(&store_path);

    match cli.command {
        Commands::Remember { concept, text } => {
            backend.remember(&concept, &text)?;
            println!("✓ Remembered: {concept}");
        }
        Commands::Recall { query, k } => {
            let results = backend.recall(&query, k);
            if results.is_empty() {
                println!("No memories found.");
            } else {
                for (i, mem) in results.iter().enumerate() {
                    println!(
                        "[{}] {} (score: {:.3}, crs: {:.3})",
                        i + 1,
                        mem.concept,
                        mem.score,
                        mem.crs
                    );
                    if !mem.provlog.is_empty() {
                        let preview: String = mem.provlog.chars().take(120).collect();
                        println!("    {}", preview);
                    }
                }
            }
        }
        Commands::Forget { concept } => {
            backend.forget(&concept)?;
            println!("✓ Forgotten: {concept}");
        }
        Commands::List => {
            let concepts = backend.list();
            if concepts.is_empty() {
                println!("No memories stored in {store_path}");
            } else {
                println!("{} memories:", concepts.len());
                for c in &concepts {
                    println!("  · {c}");
                }
            }
        }
        Commands::Ingest { path, chunk_size } => {
            use engram_ast::extract_ast_items;
            use std::fs;
            use walkdir::WalkDir;

            println!("> Starting Engram Ingest: {}", path);
            println!("  Code files  → AST extraction (one block per semantic item)");
            println!(
                "  Other files → character chunking ({} chars/block)",
                chunk_size
            );
            println!();

            let mut files_processed = 0;
            let mut chunks_minted = 0;
            let mut ast_items_minted = 0;

            let allowed_extensions = [
                "rs", "md", "txt", "js", "ts", "jsx", "tsx", "json", "toml", "py", "c", "cpp",
                "cc", "cxx", "h", "hpp", "csv", "sh", "go", "java",
            ];

            for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() {
                    continue;
                }
                let ext = entry
                    .path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if !allowed_extensions.contains(&ext) {
                    continue;
                }

                let Ok(content) = fs::read_to_string(entry.path()) else {
                    continue;
                };
                let file_name = entry
                    .path()
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");
                let file_stem = entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");

                if [
                    "rs", "py", "ts", "tsx", "js", "jsx", "go", "java", "c", "cpp", "cc", "cxx",
                    "h", "hpp",
                ]
                .contains(&ext)
                {
                    // ── AST path: one block per public item ──────────────────
                    let items = extract_ast_items(entry.path().to_str().unwrap_or(""), &content);

                    if items.is_empty() {
                        // Fallback: store whole file as a single block
                        let concept = format!("{file_stem}_{ext}");
                        if backend.remember(&concept, content.trim()).is_ok() {
                            chunks_minted += 1;
                        }
                    } else {
                        for item in &items {
                            let source = &item.full_source;
                            match backend.remember(&item.concept, source) {
                                Ok(()) => {
                                    print!("  [ast] {:<45}", item.concept);
                                    println!("  ← {}", &source.chars().take(60).collect::<String>());
                                    ast_items_minted += 1;
                                }
                                Err(e) => eprintln!("  ✗ {}: {e}", item.concept),
                            }
                        }
                    }
                } else {
                    // ── Chunker path: all other file types ───────────────────
                    let mut start = 0;
                    let mut chunk_idx = 1;

                    while start < content.len() {
                        let mut end = (start + chunk_size).min(content.len());
                        while end > start && !content.is_char_boundary(end) {
                            end -= 1;
                        }

                        let mut safe_end = end;
                        if safe_end < content.len() {
                            if let Some(offset) =
                                content[start..end].rfind(|c: char| c.is_whitespace())
                            {
                                safe_end = start + offset;
                            }
                        }

                        let chunk = &content[start..safe_end];
                        let concept = format!("{}_part{}", file_name.replace('.', "_"), chunk_idx);

                        if backend.remember(&concept, chunk.trim()).is_ok() {
                            chunks_minted += 1;
                        }

                        start = safe_end + 1;
                        chunk_idx += 1;
                    }
                }

                files_processed += 1;
            }

            println!();
            println!("✓ INGESTION COMPLETE");
            println!("  Files processed : {files_processed}");
            println!(
                "  AST items minted: {ast_items_minted}  (one block per pub fn/struct/enum/trait)"
            );
            println!("  Chunk blocks    : {chunks_minted}  (non-Rust files)");
            println!("  Total blocks    : {}", ast_items_minted + chunks_minted);
        }

        Commands::Distill {
            cluster_size,
            min_crs,
            dry_run,
        } => {
            use engram_core::genesis::KEPLER_GATE;
            use engram_core::ops::bundle;
            use engram_core::types::{Leg3Pointer, ZEDOS_PRAXIS};
            use num_complex::Complex32;

            println!("🔬 Engram Distill — Manifold Crystallization");
            println!("   Store    : {store_path}");
            println!("   Cluster  : {} memories per centroid", cluster_size);
            println!("   Min CRS  : {:.2}", min_crs);
            if dry_run {
                println!("   Mode     : DRY RUN (nothing will be written)");
            }
            println!();

            let concepts = backend.list();
            let total = concepts.len();
            if total == 0 {
                println!("No memories found in {store_path}.");
                return Ok(());
            }

            // Collect concepts with CRS ≥ min_crs, sorted by CRS descending
            let mut eligible: Vec<(String, f32, [Complex32; 8192])> = concepts
                .iter()
                .filter_map(|name| {
                    let block = backend.fetch_block(name)?;
                    let crs = block.crs_score;
                    if crs < min_crs {
                        return None;
                    }
                    Some((name.clone(), crs, block.q))
                })
                .collect();

            eligible.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let skipped = total - eligible.len();
            println!(
                "   {}/{} memories qualify (≥ CRS {:.2}), {} skipped",
                eligible.len(),
                total,
                min_crs,
                skipped
            );

            if eligible.is_empty() {
                println!("Nothing to distill — lower --min-crs or ingest more memories first.");
                return Ok(());
            }

            let clusters: Vec<&[(String, f32, [Complex32; 8192])]> =
                eligible.chunks(cluster_size).collect();

            println!(
                "   {} clusters → {} praxis blocks",
                clusters.len(),
                clusters.len()
            );
            println!();

            let mut minted = 0usize;
            for (idx, cluster) in clusters.iter().enumerate() {
                if cluster.len() < 2 {
                    println!(
                        "  [cluster {:03}] Only {} member — skipping (need ≥ 2).",
                        idx,
                        cluster.len()
                    );
                    continue;
                }

                // Compute centroid via bundle superposition
                let refs: Vec<&[Complex32; 8192]> = cluster.iter().map(|(_, _, q)| q).collect();
                let centroid = bundle(&refs);

                let avg_crs: f32 =
                    cluster.iter().map(|(_, c, _)| c).sum::<f32>() / cluster.len() as f32;
                let members_preview: String = cluster
                    .iter()
                    .take(3)
                    .map(|(name, _, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");

                let concept_name = format!("praxis_distill_cluster_{:03}", idx);
                let label = format!(
                    "DISTILLED PRAXIS — cluster {idx} of {} members (avg CRS {avg_crs:.3})\n\
                     Source concepts: {members_preview}{}\n\
                     Kepler gate: {:.2} | Riemann AI bound: 0.50",
                    cluster.len(),
                    if cluster.len() > 3 {
                        format!(" +{} more", cluster.len() - 3)
                    } else {
                        String::new()
                    },
                    KEPLER_GATE,
                );

                println!(
                    "  [cluster {:03}] {} members, avg CRS {:.3} → {}",
                    idx,
                    cluster.len(),
                    avg_crs,
                    concept_name
                );

                if !dry_run {
                    let mut block = Leg3Pointer::mint();
                    block.magic = *b"LEG3";
                    block.schema_ver = 3;
                    block.content_type = ZEDOS_PRAXIS;
                    block.zedos_tag = ZEDOS_PRAXIS;
                    block.spin_state = 1; // Axiomatic
                    block.tensor_rank = 1;
                    block.crs_score = 1.0; // Praxis blocks are crystallized — always CRS=1.0
                    block.energetics.crs = 1.0;
                    block.energetics.heat_dissipated = 5.47e-4;
                    block.q = centroid;
                    // Payload: human-readable label
                    let label_bytes = label.as_bytes();
                    let copy_len = label_bytes.len().min(block.payload.len());
                    block.payload[..copy_len].copy_from_slice(&label_bytes[..copy_len]);

                    match backend.store(&concept_name, block) {
                        Ok(()) => {
                            minted += 1;
                        }
                        Err(e) => {
                            eprintln!("  ✗ Failed to mint {concept_name}: {e}");
                        }
                    }
                } else {
                    minted += 1; // Count for dry-run report
                }
            }

            println!();
            if dry_run {
                println!(
                    "✓ Dry run complete — {} praxis blocks would be minted.",
                    minted
                );
            } else {
                println!(
                    "✓ Distillation complete — {} praxis blocks minted at CRS=1.0.",
                    minted
                );
                println!("  Run `engram forget-old --min-crs-threshold 0.70` to clean up the episodic source blocks.");
            }
        }

        Commands::Trace {
            term_a,
            op,
            term_b,
            k,
        } => {
            use engram_core::ops::{op_add, op_bind};
            let q_a = backend
                .fetch(&term_a)
                .unwrap_or_else(|| Box::new(backend.encode(&term_a).q));
            let q_b = backend
                .fetch(&term_b)
                .unwrap_or_else(|| Box::new(backend.encode(&term_b).q));

            let q_res = match op.to_uppercase().as_str() {
                "ADD" => op_add(&q_a, &q_b),
                "BIND" => op_bind(&q_a, &q_b),
                _ => {
                    eprintln!("Unknown operator {}. Use ADD or BIND.", op);
                    std::process::exit(1);
                }
            };

            println!(
                "> Submitting semantic query to BVH Manifold: [{} {} {}]",
                term_a,
                op.to_uppercase(),
                term_b
            );
            let results = backend.query(&q_res, k);
            if results.is_empty() {
                println!("No memories found intersecting that geometry.");
            } else {
                for (i, mem) in results.iter().enumerate() {
                    println!(
                        "[{}] {} (sim: {:.3}, crs: {:.3})",
                        i + 1,
                        mem.concept,
                        mem.score,
                        mem.crs
                    );
                }
            }
        }
        Commands::VerifyAgent => {
            let engram_root = shellexpand::tilde("~/.engram").into_owned();
            let access_index = std::path::Path::new(&engram_root).join("access_index.bin");
            let bvh_index = std::path::Path::new(&engram_root).join("stalks/default/engram.bvh");

            println!("🔍 Verifying Engram Agent Environment...");

            if access_index.exists() {
                println!("✓ Access Index found: {:?}", access_index);
            } else {
                println!("✗ Access Index missing! Make sure the daemon is running.");
            }

            if bvh_index.exists() {
                println!("✓ LBVH Index found: {:?}", bvh_index);
            } else {
                println!("✗ LBVH Index missing! Semantic queries will fallback to linear scan.");
            }

            // ── Embedding server liveness probe ──────────────────────────────
            let embed_url = std::env::var("ENGRAM_EMBED_URL")
                .unwrap_or_else(|_| "http://localhost:8086/v1/embeddings".to_string());

            let probe_body = serde_json::json!({"input": "test", "model": "local"});
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_millis(1500))
                .build();

            match client {
                Ok(c) => match c.post(&embed_url).json(&probe_body).send() {
                    Ok(resp) if resp.status().is_success() => {
                        println!("✓ Embedding server live: {embed_url}");
                        println!(
                            "  → remember() calls will produce valid vectors (Euler gate: PASS)"
                        );
                    }
                    Ok(resp) => {
                        println!(
                            "✗ Embedding server at {embed_url} returned HTTP {}",
                            resp.status()
                        );
                        println!("  → remember() calls will FAIL the Euler gate (BLAKE3-only vectors are chaotic)");
                        println!(
                            "  → Fix: ensure nomic-embed llama-server is running on port 8086"
                        );
                    }
                    Err(e) => {
                        println!("✗ Embedding server UNREACHABLE at {embed_url}: {e}");
                        println!("  → remember() calls will FAIL the Euler gate");
                        println!("  → Fix: start llama-server with nomic-embed model on port 8086");
                        println!("  → OR: set ENGRAM_EMBED_URL to the correct URL in your shell and IDE env");
                    }
                },
                Err(e) => println!("✗ HTTP client build failed: {e}"),
            }

            println!("💡 To track code edits, run mcp_engram_watch_workspace on first agent turn.");
        }
        Commands::Stats => {
            let concepts = backend.list();
            let total_memories = concepts.len();
            let mut pinned_count = 0;
            let mut avg_crs = 0.0;
            if total_memories > 0 {
                let mut sum_crs = 0.0;
                for c in &concepts {
                    if let Some(b) = backend.fetch_block(c) {
                        sum_crs += b.crs_score;
                        if b.crs_score >= 1.0 {
                            pinned_count += 1;
                        }
                    }
                }
                avg_crs = sum_crs / total_memories as f32;
            }
            println!("📊 Engram Manifold Stats");
            println!("   Total Memories : {}", total_memories);
            println!("   Pinned (CRS=1) : {}", pinned_count);
            println!("   Average CRS    : {:.3}", avg_crs);
            println!("   Store Path     : {}", store_path);
        }
        Commands::Export { min_crs } => {
            let concepts = backend.list();
            let mut exported = Vec::new();
            for c in concepts {
                if let Some(b) = backend.fetch_block(&c) {
                    if b.crs_score >= min_crs {
                        let text = std::str::from_utf8(&b.payload)
                            .unwrap_or("")
                            .trim_matches(char::from(0));
                        exported.push(serde_json::json!({
                            "concept": c,
                            "text": text.trim(),
                            "crs_score": b.crs_score,
                            "zedos_tag": b.zedos_tag,
                        }));
                    }
                }
            }
            let json = serde_json::to_string_pretty(&exported)?;
            println!("{}", json);
        }
        Commands::Import { file } => {
            let json = std::fs::read_to_string(&file)?;
            let entries: Vec<serde_json::Value> = serde_json::from_str(&json)?;
            let mut count = 0;
            for entry in entries {
                let concept = entry["concept"].as_str().unwrap_or("");
                let text = entry["text"].as_str().unwrap_or("");
                if !concept.is_empty()
                    && !text.is_empty()
                    && backend.remember(concept, text).is_ok()
                {
                    count += 1;
                }
            }
            println!("✓ Imported {} memories from {}", count, file);
        }
        Commands::TrackUser { interaction } => {
            if let Err(e) = backend.track_user_centroid(&interaction) {
                eprintln!("✗ Failed to track user interaction: {}", e);
            } else {
                println!("✓ Tracked user interaction in User Model.");
            }
        }
    }

    Ok(())
}
