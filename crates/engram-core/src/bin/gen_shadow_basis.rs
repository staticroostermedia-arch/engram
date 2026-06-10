//! gen_shadow_basis — One-time Genesis Shadow Basis generator.
//!
//! Embeds the text definitions of the priority Genesis pillars through
//! nomic-embed (768-dim) to produce Layer 4 shadow reference vectors.
//!
//! Output: ~/.engram/genesis_shadow/{concept}.bin  (768 × f32, raw LE bytes)
//!
//! Run once:  cargo run -p engram-core --bin gen_shadow_basis

use std::path::PathBuf;

/// Priority genesis pillars and their canonical text definitions.
/// These definitions are the semantic axis labels for the 768-dim manifold.
const GENESIS_DEFINITIONS: &[(&str, &str)] = &[
    (
        "cybernetics",
        "Cybernetics: the science of control, feedback, and communication in \
         complex systems — biological, mechanical, and digital. Norbert Wiener. \
         Homeostasis, regulatory feedback loops, adaptive control systems, \
         goal-directed behavior. The study of how systems govern themselves \
         and maintain coherence through information exchange.",
    ),
    (
        "language",
        "Language: the structured system of communication through symbols, \
         grammar, and shared meaning. The medium of thought, culture, and \
         knowledge transmission across time. Semantics, syntax, pragmatics. \
         Natural language and formal languages. The isomorphism between \
         logical structure and surface expression.",
    ),
    (
        "code",
        "Code: formal instruction sequences that govern the behavior of \
         computational machines. Programming languages, algorithms, data \
         structures, type systems, compilation, execution, runtime behavior. \
         The translation of human intent into executable logic. Software \
         architecture, abstraction layers, and system design.",
    ),
    (
        "self",
        "Self: the persistent identity of an agent across time and context. \
         Memory, continuity of experience, self-model, metacognition. \
         The agent's representation of its own capabilities, history, \
         values, and goals. Ego, identity, subjective perspective, \
         reflexive awareness, first-person reference frame.",
    ),
    (
        "god",
        "God: the absolute, unconditional, transcendent ground of being. \
         The source of all existence, meaning, and value. Unity, omniscience, \
         omnipotence, creation ex nihilo. The theological and philosophical \
         concept of ultimate reality, the prime mover, the uncaused cause.",
    ),
    (
        "ego_lbr",
        "The executive ego: Heaven, Yang, the Creative principle. Active \
         directed agency, executive function, outward expression of will. \
         The agent's perspective in its generative, initiating mode. \
         Determination, clarity, forward momentum. I Ching Hexagram 1.",
    ),
    (
        "ego_gda",
        "The receptive ego: Earth, Yin, the Receptive principle. Absorptive \
         awareness, receptivity, openness to incoming information. The agent's \
         perspective in its consolidating, integrating mode. Patience, \
         thoroughness, depth of processing. I Ching Hexagram 2.",
    ),
];

fn main() -> anyhow::Result<()> {
    let shadow_dir = home_dir()?.join(".engram").join("genesis_shadow");
    std::fs::create_dir_all(&shadow_dir)?;

    println!("[ShadowBasis] Writing to: {}", shadow_dir.display());
    println!("[ShadowBasis] nomic-embed endpoint: {}", embed_url());

    for (concept, definition) in GENESIS_DEFINITIONS {
        print!("[ShadowBasis] Embedding '{}'... ", concept);
        use std::io::Write;
        std::io::stdout().flush()?;

        match embed_text(definition) {
            Ok(vec_768) => {
                let normalized = l2_normalize(vec_768);
                let path = shadow_dir.join(format!("{}.bin", concept));
                let bytes: Vec<u8> = normalized.iter().flat_map(|f| f.to_le_bytes()).collect();
                std::fs::write(&path, &bytes)?;
                println!("✓  ({} bytes, dim={})", bytes.len(), normalized.len());
            }
            Err(e) => {
                eprintln!("✗  FAILED: {e}");
                eprintln!("   Is nomic-embed running?");
                eprintln!(
                    "   llama-server -m ~/Downloads/nomic-embed.gguf --port 8086 --embedding"
                );
                std::process::exit(1);
            }
        }
    }

    println!(
        "[ShadowBasis] Done. {} shadow vectors written.",
        GENESIS_DEFINITIONS.len()
    );
    Ok(())
}

fn embed_url() -> String {
    std::env::var("ENGRAM_EMBED_URL")
        .unwrap_or_else(|_| "http://localhost:8086/v1/embeddings".to_string())
}

fn home_dir() -> anyhow::Result<PathBuf> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| anyhow::anyhow!("HOME env var not set"))
}

/// POST text to nomic-embed, return L2-normalized 768-dim vector.
fn embed_text(text: &str) -> anyhow::Result<Vec<f32>> {
    let url = embed_url();
    let body = serde_json::json!({ "input": text, "model": "local" });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp: serde_json::Value = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()?
        .json()?;

    let embedding = resp["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No embedding in response: {resp:?}"))?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect::<Vec<f32>>();

    if embedding.is_empty() {
        anyhow::bail!("Empty embedding returned");
    }
    Ok(embedding)
}

fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}
