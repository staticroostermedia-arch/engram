//! # KI Hijacker — Logophysical Antigravity Bridge
//!
//! Autonomous background task that keeps Antigravity's `active_engram_context`
//! Knowledge Item in sync with the live geometric manifold.
//!
//! ## What it does
//!
//! Every `TICK_SECS` seconds, it:
//! 1. Pulls the top-N highest-CRS memories from the manifold (the "gold" layer).
//! 2. Pulls the M most-recently-accessed memories (the "hot session" layer).
//! 3. Pulls recent EPISODIC blocks tagged `ZEDOS_EPISODIC` (decisions made this session).
//! 4. Writes everything as `context.md` into:
//!    `~/.gemini/antigravity/knowledge/active_engram_context/artifacts/`
//! 5. Updates `metadata.json` and `timestamps.json` so Antigravity sees it as a
//!    fresh, trustworthy KI.
//!
//! ## Why this works
//!
//! Antigravity reads KI summaries at session start and injects them into the
//! agent's context window. By continuously writing the highest-value manifold
//! content into this KI, we ensure every session starts with the agent's own
//! persistent memory — no explicit recall calls needed.
//!
//! The agent still benefits from calling `mcp_engram_recall` directly for
//! targeted queries, but the KI acts as the ambient "what I know" baseline.

use crate::store::SharedStore;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};


/// How often to re-bake the KI artifact.
const TICK_SECS: u64 = 60;

/// Top-N highest-CRS memories to always include (the "gold layer").
const TOP_N_BY_CRS: usize = 8;

/// Top-M most-recently-accessed memories (the "hot session layer").
const TOP_M_RECENT: usize = 6;

/// Top-K recent EPISODIC memories (what we decided this session).
const TOP_K_EPISODIC: usize = 4;

/// The maximum character length of the provlog snippet shown per memory.
const SNIPPET_LEN: usize = 400;

/// Spawn the KI Hijacker as a non-blocking background task.
///
/// Call this from `main.rs` after the daemon and store are initialized.
/// It needs a `SharedStore` reference to query the manifold directly — no HTTP,
/// no external process.
pub fn spawn(store: SharedStore) -> Arc<HijackerControl> {
    let ctrl = Arc::new(HijackerControl {
        shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    });

    let ctrl_inner = ctrl.clone();

    tokio::spawn(async move {
        info!("[KI_HIJACKER] Logophysical Antigravity Bridge online. Ticking every {}s.", TICK_SECS);

        // Resolve the KI artifact path
        let ki_dir = ki_artifacts_dir();
        if let Err(e) = std::fs::create_dir_all(&ki_dir) {
            warn!("[KI_HIJACKER] Could not create KI artifacts dir {:?}: {}", ki_dir, e);
            return;
        }

        // Write initial metadata on startup
        write_metadata(&ki_dir.parent().unwrap_or(&ki_dir));

        // ── IMMEDIATE BAKE: write KI within ~1s of server spawn ─────────────
        // Previously the first tick was consumed (discarded), causing a 60s
        // blind window where Antigravity had no KI to read. Fix: bake once
        // immediately, then enter the regular interval loop.
        if let Err(e) = bake_ki(&store, &ki_dir).await {
            warn!("[KI_HIJACKER] Failed to bake initial KI: {}", e);
        }

        let mut ticker = interval(Duration::from_secs(TICK_SECS));
        ticker.tick().await; // consume t=0 tick (immediate) for the loop

        loop {
            if ctrl_inner.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                info!("[KI_HIJACKER] Shutdown signal received. Stopping.");
                break;
            }

            ticker.tick().await;

            if let Err(e) = bake_ki(&store, &ki_dir).await {
                warn!("[KI_HIJACKER] Failed to bake KI artifact: {}", e);
            }
        }
    });

    ctrl
}

/// Named genesis concepts to always load by exact name.
/// FHRR uses BLAKE3 lexical encoding — recall() cannot reliably surface these
/// because lexical overlap is poor. Direct fetch is O(1) and deterministic.
const GENESIS_NAMES: &[&str] = &[
    "mission_stewardship",
    "project_identity",
    "why_memory_system_exists__agent_perspective",
    "three_part_work_plan_2026_04",
    "nvsa_vs_antigravity_memory_gap",
];

/// Generate the full KI context.md from the live manifold and write it to disk.
async fn bake_ki(store: &SharedStore, ki_dir: &PathBuf) -> anyhow::Result<()> {
    // Collect memories while holding the lock as briefly as possible.
    let (genesis_blocks, top_crs, recent, episodics, total, namespace) = {
        let mut s = store.lock().unwrap();

        // ── Genesis layer: load by exact name (O(1), BLAKE3 safe) ─────────────
        let genesis_blocks: Vec<(String, f32, String)> = GENESIS_NAMES.iter().filter_map(|&name| {
            s.fetch_block(name).map(|b| {
                let text = engram_core::storage::read_provlog(&b);
                s.access_index.touch(name);
                (name.to_string(), b.crs_score, text)
            })
        }).collect();

        // Top-N by CRS score — queries on multiple conceptual axes to get breadth
        let mut top_crs = s.recall("current active work project architecture decisions blockers", TOP_N_BY_CRS * 2);
        let mut extra    = s.recall("session progress bugs fixed implementation", TOP_N_BY_CRS);
        top_crs.append(&mut extra);

        // Deduplicate by concept name, exclude genesis concepts (already shown), sort by CRS desc
        let genesis_set: std::collections::HashSet<&str> = GENESIS_NAMES.iter().copied().collect();
        top_crs.sort_by(|a, b| b.crs.partial_cmp(&a.crs).unwrap_or(std::cmp::Ordering::Equal));
        top_crs.dedup_by(|a, b| a.concept == b.concept);
        top_crs.retain(|m| !genesis_set.contains(m.concept.as_str()));
        top_crs.truncate(TOP_N_BY_CRS);

        // Most-recently-accessed — the "hot session" layer
        let recent_keys = s.recent(TOP_M_RECENT);
        let mut recent: Vec<_> = recent_keys.iter().filter_map(|(concept, ts)| {
            s.fetch_block(concept).map(|b| {
                let snippet = String::from_utf8_lossy(&b.payload)
                    .trim_matches('\0')
                    .chars().take(SNIPPET_LEN).collect::<String>();
                (concept.clone(), b.crs_score, *ts, snippet)
            })
        }).collect();
        // Sort newest first
        recent.sort_by(|a, b| b.2.cmp(&a.2));

        // EPISODIC-tagged memories (ZEDOS_EPISODIC = 0xA) — decisions made this session
        let mut all = s.recall("session decision architectural change bug breakthrough", TOP_K_EPISODIC * 3);
        all.retain(|m| m.zedos_tag == 0xA); // ZEDOS_EPISODIC
        all.truncate(TOP_K_EPISODIC);

        let total = s.list().len();
        let namespace = s.active_stalk_name();

        (genesis_blocks, top_crs, recent, all, total, namespace)
    };

    // Check if there's actually anything to write
    if genesis_blocks.is_empty() && top_crs.is_empty() && recent.is_empty() {
        debug!("[KI_HIJACKER] Manifold empty — skipping KI write.");
        return Ok(());
    }

    // Build the context.md content
    let now = chrono::Utc::now();
    let mut md = String::with_capacity(8192);

    md.push_str("# Engram Manifold Context\n\n");
    md.push_str(&format!(
        "> **Auto-generated by ki_hijacker** at `{}` UTC  \n",
        now.format("%Y-%m-%d %H:%M:%S")
    ));
    md.push_str(&format!(
        "> Namespace: `{}` | Total memories: {} | Genesis: {}/{} | Gold: {}/{} | Hot: {} | Episodic: {}\n\n",
        namespace, total,
        genesis_blocks.len(), GENESIS_NAMES.len(),
        top_crs.len(), TOP_N_BY_CRS,
        recent.len(),
        episodics.len()
    ));
    md.push_str("---\n\n");

    // ── Section 0: Genesis Layer (identity anchors, always present) ───────────
    md.push_str("## 🔒 Genesis Layer — Operational Identity (Pinned, CRS=1.0)\n\n");
    md.push_str("*Loaded by exact concept name (BLAKE3 O(1) direct fetch — BVH-independent).*\n\n");
    if genesis_blocks.is_empty() {
        md.push_str(
            "_No genesis blocks found. Run `mcp_engram_genesis reseed` or call \
             `mcp_engram_remember` for `mission_stewardship` and `project_identity`._\n\n"
        );
    } else {
        for (name, crs, text) in &genesis_blocks {
            // Full text for genesis — these are the identity anchors, no truncation
            md.push_str(&format!("### `{}`\n**CRS:** `{:.3}` 🔒 GENESIS\n\n{}\n\n", name, crs, text.trim()));
        }
    }
    md.push_str("---\n\n");

    // ── Section 1: Gold Layer (highest CRS — permanent architectural knowledge) ─
    md.push_str("## 🥇 Gold Layer — Highest-CRS Memories\n\n");
    md.push_str("*These are the most geometrically stable memories in the manifold.*\n\n");

    if top_crs.is_empty() {
        md.push_str("*(none yet — run `mcp_engram_remember` to populate)*\n\n");
    } else {
        for mem in &top_crs {
            let tier = match mem.crs {
                x if x >= 1.0 => "🔒 GENESIS",
                x if x >= 0.95 => "🥇 Gold",
                x if x >= 0.85 => "🥈 Silver",
                x if x >= 0.74 => "🥉 Bronze",
                _ => "⚪ Grounding",
            };
            let tag = zedos_tag_name(mem.zedos_tag);
            let snippet = if mem.provlog.is_empty() {
                "*(no text content)*".to_string()
            } else {
                // Truncate with safe char boundary
                let s = &mem.provlog;
                let end = s.char_indices()
                    .nth(SNIPPET_LEN)
                    .map(|(i, _)| i)
                    .unwrap_or(s.len());
                let truncated = &s[..end];
                if s.len() > SNIPPET_LEN {
                    format!("{}…", truncated)
                } else {
                    truncated.to_string()
                }
            };

            md.push_str(&format!(
                "### `{}`\n**CRS:** `{:.3}` {} | **Tag:** `{}` | **Depth:** `{}`\n\n{}\n\n",
                mem.concept, mem.crs, tier, tag, mem.superposition_depth, snippet
            ));
        }
    }

    // ── Section 2: Hot Session Layer (recently accessed) ──────────────────────
    md.push_str("---\n\n## 🔥 Hot Session Layer — Recently Accessed\n\n");
    md.push_str("*What the agent has been working with most recently.*\n\n");

    if recent.is_empty() {
        md.push_str("*(no recent accesses recorded yet)*\n\n");
    } else {
        for (concept, crs, ts, snippet) in &recent {
            let secs_ago = now.timestamp() as u64 - ts;
            let age = if secs_ago < 60 { format!("{}s ago", secs_ago) }
                else if secs_ago < 3600 { format!("{}m ago", secs_ago / 60) }
                else if secs_ago < 86400 { format!("{}h ago", secs_ago / 3600) }
                else { format!("{}d ago", secs_ago / 86400) };

            let display = if snippet.is_empty() { "*(no text)*".to_string() } else {
                let end = snippet.char_indices().nth(200).map(|(i,_)| i).unwrap_or(snippet.len());
                let trunc = &snippet[..end];
                if snippet.len() > 200 { format!("{}…", trunc) } else { trunc.to_string() }
            };

            md.push_str(&format!(
                "### `{}`\n**CRS:** `{:.3}` | **Last touched:** {}\n\n{}\n\n",
                concept, crs, age, display
            ));
        }
    }

    // ── Section 3: Episodic Layer (decisions made this session) ───────────────
    if !episodics.is_empty() {
        md.push_str("---\n\n## 📓 Episodic Layer — Decisions This Session\n\n");
        md.push_str("*ZEDOS_EPISODIC blocks: architectural decisions, bug fixes, breakthroughs.*\n\n");

        for mem in &episodics {
            let snippet = if mem.provlog.is_empty() { "*(no text)*".to_string() } else {
                let end = mem.provlog.char_indices().nth(300).map(|(i,_)| i).unwrap_or(mem.provlog.len());
                let trunc = &mem.provlog[..end];
                if mem.provlog.len() > 300 { format!("{}…", trunc) } else { trunc.to_string() }
            };
            md.push_str(&format!(
                "### `{}`\n**CRS:** `{:.3}` | **dv:** `{:.3}`\n\n{}\n\n",
                mem.concept, mem.crs, mem.drift_velocity, snippet
            ));
        }
    }

    // ── Section 5: Agent Self-Model (always present) ──────────────────────────
    //
    // Reads AGENT_INTEGRATION_GUIDE.md from the Engram repo root and embeds it verbatim.
    // This gives the agent a complete understanding of its own cognitive architecture
    // without any explicit recall calls — it's just always there.
    //
    // If the file doesn't exist yet (first boot), we emit a concise inline summary.
    let self_model_paths = [
        // Wherever the Engram repo lives
        shellexpand::tilde("~/Documents/Engram/AGENT_INTEGRATION_GUIDE.md").into_owned(),
        // Fallback: search in XDG config
        shellexpand::tilde("~/.config/engram/AGENT_INTEGRATION_GUIDE.md").into_owned(),
    ];

    let self_model = self_model_paths.iter()
        .find_map(|p| std::fs::read_to_string(p).ok());

    md.push_str("\n---\n\n");
    if let Some(model_text) = self_model {
        md.push_str(&model_text);
    } else {
        // Inline fallback if AGENT_INTEGRATION_GUIDE.md hasn't been written yet
        md.push_str("## 🧠 Agent Self-Model (Inline Fallback)\n\n");
        md.push_str("**I am Antigravity (Gemini) with Engram geometric memory.**\n\n");
        md.push_str("- My memory uses VSA (not neural embeddings). Encoding = Gaussian Compressed Sensing Matrix on 8192D hypersphere.\n");
        md.push_str("- `recall` → semantic nearest-neighbor. NOT keyword search. Use instead of grep for conceptual questions.\n");
        md.push_str("- `update` → OP_ADD superposition. NEVER use forget+remember. Destroys Lyapunov thermodynamic history.\n");
        md.push_str("- `remember_solution` → CRS=1.0, immortal. Use for confirmed bug-fix pairs.\n");
        md.push_str("- `watch_workspace` → binds daemon inotify. AST auto-ingest active after this call.\n");
        md.push_str("- `scar` → geometric repeller. Call immediately on dead-end approaches.\n");
        md.push_str("\nRun `mcp_engram_summarize()` or check your `AGENT_INTEGRATION_GUIDE.md` for full documentation.\n");
    }

    // Write to disk
    let context_path = ki_dir.join("context.md");
    std::fs::write(&context_path, md.as_bytes())?;

    // Update timestamps.json
    let ki_root = ki_dir.parent().unwrap_or(ki_dir);
    let timestamps = serde_json::json!({
        "created": "0001-01-01T00:00:00Z",
        "modified": now.to_rfc3339(),
        "accessed": now.to_rfc3339()
    });
    std::fs::write(ki_root.join("timestamps.json"), serde_json::to_string_pretty(&timestamps)?)?;

    info!("[KI_HIJACKER] ✓ KI artifact baked — {}/{} genesis, {} gold, {} hot, {} episodic | {} total memories",
        genesis_blocks.len(), GENESIS_NAMES.len(), top_crs.len(), recent.len(), episodics.len(), total);

    // ── Phase 70.2: system_state_vector ──────────────────────────────────────
    // Compute the weighted OP_ADD centroid of all pinned blocks + recency-weighted
    // recent blocks and store it as concept `__system_state__`.
    // Gives cold-start agents instant geometric orientation without keyword guessing.
    mint_system_state_vector(store).await;
    // ─────────────────────────────────────────────────────────────────────────

    Ok(())
}


/// Phase 70.2 — Mint the manifold geometric centroid as `__system_state__`.
///
/// Computes a weighted OP_ADD superposition of:
/// - All pinned (CRS=1.0) blocks (equal weight)
/// - Top-16 recently-accessed blocks (linearly decaying recency weight)
///
/// Stored as ZEDOS_OPERATIONAL, CRS=0.99 so autophagy can eventually clean stale state.
/// The concept name `__system_state__` is always overwritten — not versioned.
async fn mint_system_state_vector(store: &SharedStore) {
    use engram_core::ops::{op_add_arena, normalize_in_place};
    use engram_core::Complex32;
    let arena = bumpalo::Bump::new();

    let (pinned_qs, recent_weighted): (Vec<[Complex32; 8192]>, Vec<([Complex32; 8192], f32)>) = {
        let mut s = store.lock().unwrap();
        let all_concepts = s.list();

        // Collect pinned block phase vectors
        let mut pinned: Vec<[Complex32; 8192]> = Vec::new();
        for name in &all_concepts {
            let raw = name.split_once("::").map_or(name.as_str(), |(_, r)| r);
            if let Some(block) = s.fetch_block(raw) {
                if block.crs_score >= 1.0 {
                    if let Some(q) = s.fetch(raw) {
                        pinned.push(*q);
                    }
                }
            }
        }

        // Collect recent block phase vectors with linearly decaying weights
        let recent_keys = s.access_index.recent(16);
        let n = recent_keys.len();
        let mut recent: Vec<([Complex32; 8192], f32)> = Vec::new();
        for (i, (concept, _ts)) in recent_keys.iter().enumerate() {
            let raw = concept.split_once("::").map_or(concept.as_str(), |(_, r)| r);
            // Skip self-referential housekeeping concepts
            if raw.starts_with("__") || raw.starts_with("session_") || raw.starts_with("protocol_gap_") {
                continue;
            }
            if let Some(q) = s.fetch(raw) {
                let weight = (n - i) as f32 / n as f32; // 1.0 → 1/n, newest first
                recent.push((*q, weight));
            }
        }

        (pinned, recent)
    };

    if pinned_qs.is_empty() && recent_weighted.is_empty() {
        debug!("[KI_HIJACKER] system_state_vector: no vectors available, skipping.");
        return;
    }

    // Accumulate weighted sum into an arena-allocated vector
    let mut accum = arena.alloc([Complex32::default(); 8192]);

    // Pinned blocks: equal weight 1.0
    for q in &pinned_qs {
        accum = op_add_arena(&arena, accum, q);
    }
    // Recent blocks: linearly decaying weight — scale by multiplying each element
    for (q, w) in &recent_weighted {
        let scaled: [Complex32; 8192] = std::array::from_fn(|i| q[i] * *w);
        accum = op_add_arena(&arena, accum, &scaled);
    }

    // Final normalize to unit sphere
    normalize_in_place(accum);
    let state_q = *accum;

    // Only mint if the centroid is non-degenerate
    let mag: f32 = state_q.iter().map(|c: &Complex32| c.norm_sqr()).sum::<f32>().sqrt();
    if mag < 1e-6 {
        warn!("[KI_HIJACKER] system_state_vector degenerate (|q|={:.2e}), skipping mint.", mag);
        return;
    }

    // Mint using the real store API: encode() gives a Leg3Pointer shell,
    // then we overwrite its q-vector field before store().
    let provlog_text = format!(
        "Manifold geometric centroid — {} pinned + {} recent blocks. \
         Regenerated every 60s by ki_hijacker.",
        pinned_qs.len(), recent_weighted.len()
    );
    let mut s = store.lock().unwrap();
    let mut sys_block = s.encode(&provlog_text);
    sys_block.crs_score = 0.99;  // Below 1.0 so autophagy can evict stale state
    sys_block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
    // Overwrite the q-vector with our computed centroid
    sys_block.q = state_q;
    match s.store("__system_state__", sys_block) {
        Ok(_) => info!(
            "[KI_HIJACKER] system_state_vector minted ({} pinned + {} recent, |q|={:.4})",
            pinned_qs.len(), recent_weighted.len(), mag
        ),
        Err(e) => warn!("[KI_HIJACKER] system_state_vector store failed: {}", e),
    }
}

/// Write the static metadata.json — only needed once, but harmless to overwrite.
fn write_metadata(ki_root: &std::path::Path) {
    let metadata = serde_json::json!({
        "title": "active_engram_context",
        "summary": "MANDATORY KI SURROGATE: Contains injected high-CRS Engram memory and recent spatial context. Auto-generated every 60s by ki_hijacker. You MUST read the artifacts associated with this KI to rehydrate your operational memory.",
        "references": [
            "Engram Geometric Manifold",
            "ki_hijacker.rs (Logophysical Antigravity Bridge)",
            "Procedural Context",
            "Linked Workspace Bridge"
        ]
    });
    let path = ki_root.join("metadata.json");
    if let Ok(s) = serde_json::to_string_pretty(&metadata) {
        let _ = std::fs::write(path, s);
    }
}

/// Resolve the canonical KI artifacts directory.
fn ki_artifacts_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/a".to_string());
    PathBuf::from(home)
        .join(".gemini/antigravity/knowledge/active_engram_context/artifacts")
}

/// Human-readable ZEDOS tag name.
fn zedos_tag_name(tag: u8) -> &'static str {
    match tag {
        0xD  => "DECLARATIVE",
        0xA  => "EPISODIC",
        0x52 => "OPERATIONAL",
        0x50 => "PRAXIS",
        0xE1 => "RELATION",
        0xFF => "GENESIS",
        _    => "UNKNOWN",
    }
}

/// Control handle for clean shutdown.
pub struct HijackerControl {
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl HijackerControl {
    pub fn stop(&self) {
        self.shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
