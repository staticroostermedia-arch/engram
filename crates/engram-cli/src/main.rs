use clap::{Parser, Subcommand};
use engram_core::backend::{CpuBackend, VsaBackend};

#[derive(Parser)]
#[command(name = "engram", about = "Persistent geometric memory for AI agents", version)]
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
    Forget {
        concept: String,
    },
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
                    println!("[{}] {} (score: {:.3}, crs: {:.3})", i + 1, mem.concept, mem.score, mem.crs);
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
            use walkdir::WalkDir;
            use std::fs;
            
            println!("> Starting Phase 5 Ingestion Protocol directed at: {}", path);
            let mut files_processed = 0;
            let mut chunks_minted = 0;

            for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() { continue; }
                let ext = entry.path().extension().and_then(|s| s.to_str()).unwrap_or("");
                
                // Only ingest text/code files
                let allowed_extensions = ["rs", "md", "txt", "js", "ts", "json", "toml", "py", "c", "cpp", "h", "csv", "sh"];
                if !allowed_extensions.contains(&ext) { continue; }

                if let Ok(content) = fs::read_to_string(entry.path()) {
                    let file_name = entry.path().file_name().and_then(|s| s.to_str()).unwrap_or("unknown");
                    let mut start = 0;
                    let mut chunk_idx = 1;

                    while start < content.len() {
                        let mut end = (start + chunk_size).min(content.len());
                        // Ensure we don't snap mid-multibyte char
                        while end > start && !content.is_char_boundary(end) {
                            end -= 1;
                        }

                        // try to break on whitespace if we're not at the very end
                        let mut safe_end = end;
                        if safe_end < content.len() {
                            if let Some(offset) = content[start..end].rfind(|c: char| c.is_whitespace() || c == '\n') {
                                safe_end = start + offset;
                            }
                        }
                        
                        let chunk = &content[start..safe_end];
                        // concept naming: "file_name_partN"
                        let concept_name = format!("{}_part{}", file_name.replace('.', "_"), chunk_idx);
                        
                        if let Err(e) = backend.remember(&concept_name, chunk.trim()) {
                            eprintln!("Failed to ingest chunk {}: {}", concept_name, e);
                        } else {
                            chunks_minted += 1;
                        }

                        start = safe_end + 1; // skip the space/newline
                        chunk_idx += 1;
                    }
                    files_processed += 1;
                }
            }
            println!("✓ INGESTION COMPLETE. Processed {} files into {} geometric holograms.", files_processed, chunks_minted);
        }
        Commands::Trace { term_a, op, term_b, k } => {
            use engram_core::ops::{op_add, op_bind};
            let q_a = backend.fetch(&term_a).unwrap_or_else(|| Box::new(backend.encode(&term_a).q));
            let q_b = backend.fetch(&term_b).unwrap_or_else(|| Box::new(backend.encode(&term_b).q));
            
            let q_res = match op.to_uppercase().as_str() {
                "ADD" => op_add(&q_a, &q_b),
                "BIND" => op_bind(&q_a, &q_b),
                _ => {
                    eprintln!("Unknown operator {}. Use ADD or BIND.", op);
                    std::process::exit(1);
                }
            };
            
            println!("> Submitting semantic query to BVH Manifold: [{} {} {}]", term_a, op.to_uppercase(), term_b);
            let results = backend.query(&q_res, k);
            if results.is_empty() {
                println!("No memories found intersecting that geometry.");
            } else {
                for (i, mem) in results.iter().enumerate() {
                    println!("[{}] {} (sim: {:.3}, crs: {:.3})", i + 1, mem.concept, mem.score, mem.crs);
                }
            }
        }
    }

    Ok(())
}
