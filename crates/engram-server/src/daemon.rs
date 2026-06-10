use crate::store::SharedStore;
use notify_debouncer_full::{new_debouncer, notify::*};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info};

// ── Shadow Basis (Phase 1 — 768-dim genesis anchor) ────────────────────────
//
// On daemon startup, attempt to load the pre-generated 768-dim shadow vectors
// for the genesis pillars from ~/.engram/genesis_shadow/.
// If the directory doesn't exist yet, run gen_shadow_basis first.

/// Load a 768-dim shadow genesis vector from disk.
/// Path: ~/.engram/genesis_shadow/{concept}.bin  (768 × f32 LE bytes)
fn load_shadow_vector(concept: &str) -> Option<Vec<f32>> {
    let path = std::env::var("HOME").ok()
        .map(|h| PathBuf::from(h)
            .join(".engram")
            .join("genesis_shadow")
            .join(format!("{}.bin", concept)))?;
    let bytes = std::fs::read(&path).ok()?;
    if bytes.len() % 4 != 0 { return None; }
    let vec: Vec<f32> = bytes.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    if vec.is_empty() { None } else { Some(vec) }
}

/// OP_ADD(block.q, shadow) → L2 normalize — anchors an 8192-dim block to the genesis basis.
/// The shadow vector is 768-dim; it is applied to the first 768 real components of q.
/// This biases the block's semantic position toward the genesis pillar in the real subspace.
fn apply_shadow_anchor(block: &mut engram_core::types::HolographicBlock, shadow: &[f32]) {
    let len = shadow.len().min(block.q.len());
    for i in 0..len {
        block.q[i].re += shadow[i];
    }
    // L2 normalize the full 8192-dim q after anchoring
    let norm: f32 = block.q.iter().map(|c| c.re * c.re + c.im * c.im).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        block.q.iter_mut().for_each(|c| { c.re /= norm; c.im /= norm; });
    }
}

/// Read .engramignore files from the watched workspace root and ~/.engram/.
/// Returns a list of path patterns that the daemon should never watch or encode.
/// To customise: create a `.engramignore` in your project root or in `~/.engram/`.
fn load_engramignore() -> Vec<String> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    // ~/.engram/.engramignore — user-level global ignore list
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(std::path::PathBuf::from(&home).join(".engram").join(".engramignore"));
    }

    // $ENGRAM_LINKED_WORKSPACE/.engramignore — project-level ignore list
    if let Ok(ws) = std::env::var("ENGRAM_LINKED_WORKSPACE") {
        candidates.push(std::path::PathBuf::from(&ws).join(".engramignore"));
    }

    // CWD + ENGRAM_WORKSPACE so project-local .engramignore (e.g. this repo's) is honored for passive watch/force
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(".engramignore"));
    }
    if let Ok(ws2) = std::env::var("ENGRAM_WORKSPACE") {
        candidates.push(std::path::PathBuf::from(&ws2).join(".engramignore"));
    }

    let mut ignored = Vec::new();
    for path in &candidates {
        if let Ok(text) = std::fs::read_to_string(path) {
            for line in text.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    ignored.push(trimmed.to_string());
                }
            }
        }
    }
    // Built-in defaults (supplement .engramignore) to keep passive ingest clean
    for def in ["node_modules/", "extensions/vscode/node_modules/", "/dist/", "/build/"] {
        if !ignored.iter().any(|p| p.contains(def)) {
            ignored.push(def.to_string());
        }
    }
    ignored
}

/// Starts the global agentic background daemon attached to the MCP / REST Server.
///
/// Autophagy GC is DISABLED. Nothing is ever evicted automatically.
/// The daemon runs three autonomous loops:
///   1. File watcher  — inotify/fsevents → AST extraction → live project indexing
///   2. NREM cycle    — periodic ego narrative tensor consolidation (ego.leg3)
///   3. Health watchdog — config-driven process monitor (~/.engram/watchdog.toml)
pub fn spawn(store: SharedStore) -> Arc<DaemonControl> {
    // Load shadow basis vectors at spawn time (once — not per file event)
    let shadow_cybernetics: Option<Vec<f32>> = load_shadow_vector("cybernetics");
    match &shadow_cybernetics {
        Some(v) => info!("[ShadowBasis] Loaded cybernetics_768 ({} floats)", v.len()),
        None    => info!("[ShadowBasis] cybernetics_768 not found — run gen_shadow_basis to enable genesis anchoring"),
    }
    let engramignore = load_engramignore();
    if !engramignore.is_empty() {
        info!("[.engramignore] Excluding {} path patterns from watch", engramignore.len());
    }

    // ── Load health watchdog config (~/.engram/watchdog.toml) ────────────────
    // This is the ONLY place Engram learns about consumer-specific processes.
    // If the file doesn't exist, the watchdog is a no-op — zero coupling.
    let watchdog_cfg = crate::watchdog::WatchdogConfig::load();
    let watchdog_proposals_path = watchdog_cfg.resolved_proposals_path();
    if watchdog_cfg.watch.is_empty() {
        info!("[Watchdog] No processes configured — health watchdog is a no-op.");
    } else {
        info!("[Watchdog] Monitoring {} process(es). Proposals → {}",
            watchdog_cfg.watch.len(), watchdog_proposals_path.display());
    }

    let (watch_tx, watch_rx) = flume::unbounded::<PathBuf>();

    let daemon = Arc::new(DaemonControl {
        active_watch: Arc::new(tokio::sync::RwLock::new(None)),
        shutdown: Arc::new(AtomicBool::new(false)),
        watch_tx,
    });

    let ctrl = daemon.clone();

    tokio::spawn(async move {
        let (tx, rx) = flume::unbounded();

        let mut debouncer = new_debouncer(Duration::from_millis(1500), None, move |res| {
            if let Ok(events) = res {
                for e in events {
                    let _ = tx.send(e);
                }
            }
        }).unwrap();

        info!("Agentic Daemon (Phase 7) online. Autophagy GC DISABLED — watcher only.");

        // Flush hot access timestamps to disk every 60 seconds (Mac hardening)
        let mut flush_interval = tokio::time::interval(Duration::from_secs(60));

        // ── Phase 3 Epoch IX: NREM Dream Consolidation (Item 1 Ego Intent) ─────
        // Frequency is configurable via ENGRAM_NREM_INTERVAL_MINUTES (default 360 = 6h for
        // long-term stability on large manifolds). For responsive Primary Intent / goal
        // influence during daily TUI use, 60–120 minutes is recommended now that we have
        // mass capping + relation-driven goal biasing.
        //
        // On each tick:
        //   1. Scan high-CRS blocks.
        //   2. OP_ADD-superpose (with goal bias + mass cap applied).
        //   3. Write to `~/.engram/ego.leg3`.
        //   4. Hot-swap via refresh_ego_q().
        let nrem_minutes: u64 = std::env::var("ENGRAM_NREM_INTERVAL_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(360);
        let mut nrem_interval = tokio::time::interval(Duration::from_secs(nrem_minutes * 60));
        nrem_interval.tick().await; // skip the immediate first tick at startup

        // If ego.leg3 does not exist yet, trigger an immediate NREM pass to seed it.
        let ego_path = std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".engram").join("ego.leg3"))
            .unwrap_or_default();
        if !ego_path.exists() {
            info!("[NREM] ego.leg3 missing on startup — triggering immediate genesis consolidation.");
            run_nrem_consolidation(&store);
        }

        // ── Integration Inbox Scanner (Phase 5) ──────────────────────────────
        // Poll ~/.engram/stalks/default/inbox/ every 5 seconds for
        // `integration_req_*.json` files written by the Cockpit when the operator
        // clicks INTEGRATE on an escalation report.
        // Format: { "concept": "...", "text": "...", "source": "escalation_id" }
        // On success: writes the .leg block, then DELETES the request file.
        let mut inbox_interval = tokio::time::interval(Duration::from_secs(5));

        // The inbox dir is adjacent to the default stalk — always exists.
        let inbox_dir = {
            let lock = store.lock().unwrap();
            PathBuf::from(lock.store_path()).join("inbox")
        };
        std::fs::create_dir_all(&inbox_dir).ok();
        info!("Integration inbox watching: {}", inbox_dir.display());

        // ── System Health Watchdog ────────────────────────────────────────────
        // Every 5 minutes, check each process in ~/.engram/watchdog.toml.
        // If the file doesn't exist, watch list is empty and this is a no-op.
        let mut health_interval = tokio::time::interval(Duration::from_secs(5 * 60));
        health_interval.tick().await; // skip first tick on startup

        loop {
            if ctrl.shutdown.load(Ordering::Relaxed) {
                break;
            }

            tokio::select! {
                new_watch = watch_rx.recv_async() => {
                    if let Ok(p) = new_watch {
                        // ENGRAM_DEFER_WATCH_INGEST=1 (MCP default): record path only — no recursive
                        // OS watch and no force_ingest. Recursive debouncer.watch on large repos
                        // floods modify/create events → per-file AST store → 10–40GB RSS + MCP death.
                        // Lean wake uses incremental_spatial_ingest; enable full watch by unsetting env.
                        let defer_ingest = std::env::var("ENGRAM_DEFER_WATCH_INGEST")
                            .as_deref() == Ok("1");
                        if defer_ingest {
                            info!(
                                "Watch path recorded (ENGRAM_DEFER_WATCH_INGEST=1): {} — \
                                 OS watcher NOT bound; use incremental_spatial_ingest / force_spatial_ingest",
                                p.display()
                            );
                        } else if let Err(e) = debouncer.watch(&p, RecursiveMode::Recursive) {
                            error!("Daemon failed to bind OS watcher to {}: {}", p.display(), e);
                        } else {
                            info!("Daemon dynamically bound OS watcher to: {}", p.display());
                            let ingest_res = {
                                let mut lock = store.lock().unwrap();
                                lock.force_ingest_path(&p.to_string_lossy(), true)
                            };
                            match ingest_res {
                                Ok((count, _details)) => {
                                    info!("Passive initial AST ingest on watch bind: {} items from {}", count, p.display());
                                }
                                Err(e) => {
                                    error!("Passive initial ingest failed for {}: {}", p.display(), e);
                                }
                            }
                        }
                    }
                }

                _ = flush_interval.tick() => {
                    // Flush hot access timestamps to disk every 60 seconds
                    let mut lock = store.lock().unwrap();
                    lock.access_index.flush_if_dirty();
                }

                _ = nrem_interval.tick() => {
                    run_nrem_consolidation(&store);
                }

                _ = health_interval.tick() => {
                    run_health_watchdog(&watchdog_cfg, &watchdog_proposals_path);
                }

                _ = inbox_interval.tick() => {
                    // ── Integration Inbox: sweep and process ─────────────────────────
                    if let Ok(entries) = std::fs::read_dir(&inbox_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            let fname = path.file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("");
                            if !fname.starts_with("integration_req_") || !fname.ends_with(".json") {
                                continue;
                            }
                            match std::fs::read_to_string(&path) {
                                Ok(raw) => {
                                    if let Ok(req) = serde_json::from_str::<serde_json::Value>(&raw) {
                                        let concept = req["concept"].as_str().unwrap_or("").to_string();
                                        let text    = req["text"].as_str().unwrap_or("").to_string();
                                        let source  = req["source"].as_str().unwrap_or("").to_string();

                                        if concept.is_empty() || text.is_empty() {
                                            error!("[INBOX] Malformed integration request {}: concept/text missing", fname);
                                            std::fs::remove_file(&path).ok();
                                            continue;
                                        }

                                        let mut lock = store.lock().unwrap();
                                        match lock.remember(&concept, &text) {
                                            Ok(_) => {
                                                info!("[INBOX] Integrated escalation → Engram: '{}' (from {})", concept, source);
                                                std::fs::remove_file(&path).ok();
                                            }
                                            Err(e) => {
                                                error!("[INBOX] remember() failed for '{}': {}", concept, e);
                                                // Leave the file for retry on next tick
                                            }
                                        }
                                    } else {
                                        error!("[INBOX] JSON parse error in {}", fname);
                                        std::fs::remove_file(&path).ok();
                                    }
                                }
                                Err(e) => {
                                    error!("[INBOX] Failed to read {}: {}", fname, e);
                                }
                            }
                        }
                    }
                }

                event = rx.recv_async() => {
                    if let Ok(ev) = event {
                        if ev.kind.is_modify() || ev.kind.is_create() {
                            for path in &ev.paths {
                                if !path.is_file() { continue; }
                                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                                let allowed_exts = [
                                    "rs", "md", "txt", "js", "ts", "json", "toml", "py",
                                    "c", "cpp", "h", "csv", "sh", "go", "java", "rb",
                                    "zig", "php", "html", "css", "yml", "yaml", "sql",
                                    "ex", "exs", "swift",
                                ];

                                // .engramignore: skip paths owned by ouroboros daemon
                                let path_str = path.to_string_lossy();
                                let is_ignored = engramignore.iter()
                                    .any(|pat| path_str.contains(pat.as_str()));

                                if allowed_exts.contains(&ext)
                                    && !path_str.contains("/target/")
                                    && !path_str.contains("/.git/")
                                    && !is_ignored
                                {
                                    if let Ok(content) = std::fs::read_to_string(path) {
                                        let mut lock = store.lock().unwrap();

                                        // ── Namespace separation ────────────────────────────────
                                        // If ENGRAM_LINKED_WORKSPACE is set, AST blocks from that
                                        // workspace go into a dedicated '<workspace_name>_ast' stalk.
                                        // This keeps auto-ingested code blocks separate from agent
                                        // episodic memories in the default stalk.
                                        let linked_ws = std::env::var("ENGRAM_LINKED_WORKSPACE").ok();
                                        let (is_linked, ast_stalk) = match &linked_ws {
                                            Some(ws) if path_str.contains(ws.as_str()) => {
                                                let name = std::path::Path::new(ws)
                                                    .file_name()
                                                    .and_then(|n| n.to_str())
                                                    .unwrap_or("linked")
                                                    .to_lowercase();
                                                (true, format!("{}_ast", name))
                                            }
                                            _ => (false, String::new()),
                                        };
                                        if is_linked {
                                            lock.set_active_stalk(&ast_stalk);
                                        }

                                        let items = engram_ast::extract_ast_items(path.to_str().unwrap_or(""), &content);

                                        let mut ast_concepts: Vec<String> = Vec::new();

                                        if !items.is_empty() {
                                            for item in items {
                                                let mut block = lock.encode(&item.embed_label());

                                                // Map 2D file coordinates into the 3D logophysical bounding box
                                                block.aabb_min = [item.start_pos.0 as f32, item.start_pos.1 as f32, 0.0];
                                                block.aabb_max = [item.end_pos.0 as f32,   item.end_pos.1 as f32,   0.0];

                                                // Store full unbroken source in ProvLog
                                                engram_core::storage::write_provlog(&mut block, &item.full_source);

                                                // ── Genesis Shadow Anchor ──────────────────────
                                                // OP_ADD(block.q, v_cybernetics_768) → L2 normalize
                                                // Biases the AST block toward the genesis coordinate
                                                // system within the 768-dim shadow manifold.
                                                // Both vectors are 768-dim so dimensions match.
                                                if let Some(ref shadow) = shadow_cybernetics {
                                                    apply_shadow_anchor(&mut block, shadow);
                                                }

                                                if let Err(e) = lock.store(&item.concept, block) {
                                                    error!("Daemon failed to auto-sync AST {}: {}", item.concept, e);
                                                } else {
                                                    debug!("Daemon: Auto-synced AST {} (shadow_anchor={})",
                                                        item.concept, shadow_cybernetics.is_some());
                                                    ast_concepts.push(item.concept.clone());
                                                }
                                            }

                                            // ── Automatic relational gluing for spatial sheaf (Gap 1 resolution) ──
                                            // Creates "defined_in_file" and "next_sibling_in_file" relations so
                                            // AABB AST blocks participate in the knowledge graph used by
                                            // search_by_relation / visualize / impact analysis.
                                            // This makes Pre-Edit/Post-Delta in the spatial ritual fantastically effective.
                                            if !ast_concepts.is_empty() {
                                                let file_stem = ast_concepts[0]
                                                    .split("__")
                                                    .next()
                                                    .unwrap_or("unknown")
                                                    .to_string();
                                                let file_container = format!("{}_file", file_stem);

                                                // Ensure a lightweight file container exists
                                                if lock.fetch_block(&file_container).is_none() {
                                                    let container_text = format!(
                                                        "AST container for file stem '{}'. All top-level items (fn/struct/impl/etc.) extracted from this file via Tree-Sitter are related here.",
                                                        file_stem
                                                    );
                                                    let _ = lock.remember(&file_container, &container_text);
                                                }

                                                // Relate every AST item to the file container
                                                for c in &ast_concepts {
                                                    let _ = lock.relate(&file_container, c, "defines");
                                                }

                                                // Sibling chaining in source order (fantastic for impact: "what else is in this file?")
                                                for i in 1..ast_concepts.len() {
                                                    let _ = lock.relate(&ast_concepts[i-1], &ast_concepts[i], "next_sibling_in_file");
                                                    let _ = lock.relate(&ast_concepts[i], &ast_concepts[i-1], "prev_sibling_in_file");
                                                }

                                                debug!("Daemon: Created spatial relations for {} AST items in '{}'", ast_concepts.len(), file_stem);

                                                // ── Ritual-aware auto-bridging (next layer for Gap 4) ──
                                                // When we are ingesting spatial code that is core to the impact ritual itself,
                                                // automatically relate the file container to the living praxis anchor.
                                                // This creates real, automatic gluing between the AST world and the ritual/praxis world.
                                                let ritual_relevant_stems = [
                                                    "daemon", "mcp", "store", "engram_ast", "working_memory",
                                                    "context_for_file", "recall_in_file"
                                                ];
                                                if ritual_relevant_stems.iter().any(|s| file_stem.contains(s)) {
                                                    let praxis_anchor = "praxis:spatial_manifold_impact_analysis";
                                                    if lock.fetch_block(praxis_anchor).is_some() {
                                                        let _ = lock.relate(&file_container, praxis_anchor, "exercises_spatial_ritual");
                                                        debug!("Daemon: Auto-bridged '{}' to spatial praxis anchor", file_stem);
                                                    }
                                                }
                                            }
                                        } else {
                                            // Fallback chunking
                                            let file_name = path
                                                .file_name()
                                                .and_then(|s| s.to_str())
                                                .unwrap_or("unknown");
                                            let concept_name = format!(
                                                "{}_daemon",
                                                file_name.replace('.', "_")
                                            );

                                            let safe_end = content.len().min(8000);
                                            let mut end = safe_end;
                                            while end > 0 && !content.is_char_boundary(end) {
                                                end -= 1;
                                            }

                                            if let Err(e) = lock.remember(&concept_name, &content[..end]) {
                                                error!(
                                                    "Daemon failed to auto-sync file {}: {}",
                                                    path.display(),
                                                    e
                                                );
                                            } else {
                                                debug!(
                                                    "Daemon: Auto-synced fallback {}",
                                                    path.display()
                                                );
                                            }

                                        // Restore default namespace after linked-workspace block
                                        if is_linked {
                                            lock.set_active_stalk("default");
                                        }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    daemon
}

// ── Phase 3 Epoch IX: NREM Dream Consolidation ───────────────────────────────
//
// Called every 6 hours by the circadian timer inside spawn().
//
// Algorithm:
//   1. List all concepts in the active manifold.
//   2. For each concept, fetch_block() and inspect CRS.
//   3. Blocks with CRS ≥ NREM_CRS_THRESHOLD contribute their q-vector to a
//      running OP_ADD accumulator (equal weights — mass proportional to signal).
//   4. L2-normalize the accumulated centroid to keep it on the unit hypersphere.
//   5. Mint a fresh HolographicBlock (Leg3Pointer::mint()), write the centroid
//      into its q-field, mark it ZEDOS_EPISODIC, and persist it to ego.leg3.
//   6. Call StoreHandle::refresh_ego_q() — the live recall path immediately
//      picks up the updated interpretive frame.
//
// WS2-B (Phase 2 tile child goal sub1) + Phase 2.5 432Hz Symplectic Harmonics (goal:1780185084..._sub4):
// blocks with zedos_tag == ZEDOS_TRAINING (now 8+1 harmonic-augmented) or containing "harmonic_432hz" / "432Hz" marker
// receive NREM_TRAINING_BIAS_FACTOR (2.0) + extra harmonic preference weight (see body).
// Higher consolidation influence to richer harmonic-rich CLS TRAINING + ego.leg3 trajectories (for recursive LoRA training medium).
// Hot promotion (mark_hot) extended for harmonic-rich. No HolographicBlock layout or invariants change.
// Coordinates 2.1 (geo/SymplecticState notes in tuples) + 2.3 (hot residency of harmonic blocks via NREM/ki_hijacker).
//
// Safety: the store mutex is released between fetches so the server stays
// responsive. We accumulate into a CPU-side array, never holding the lock
// across the (slow) list() traversal.

const NREM_CRS_THRESHOLD: f32 = 0.85;

/// Tunable constants for Item 1 NREM ego/goal biasing expansion (recency-weighted + mass-capped phase).
/// These replace the original hardcoded 2x crude payload heuristic.
/// See design intent from ego_goal_nrem_biasing_sketch + Item 1 checkpoint B.
const NREM_GOAL_BIAS_FACTOR: f32 = 2.5;
/// Maximum fraction of the final pre-normalize accumulator mass that can come from goal-biased blocks.
/// Prevents a small number of high-CRS active goals from dominating the ego centroid.
const NREM_GOAL_BIASED_MASS_CAP: f32 = 0.40;

/// Bias factor for ZEDOS_TRAINING-tagged blocks (WS2-B / Phase 2 execution plan tile + child goal:1780165889_substrate-cs--richer-cls-8-property-trai_sub1)
/// + Phase 2.5 432Hz Symplectic (goal:1780185084..._sub4).
/// TRAINING + harmonic-rich blocks (now 8+1 prop with 432Hz phase relations / sacred freq multiples in payload + energetics advisory)
/// receive elevated weight (2.0 * 1.15 for harmonic) during NREM so superior harmonic-rich training data preferentially shapes
/// ego.leg3 trajectories / persistent self-model as recursive LoRA medium. Coordinates 2.1 geo + 2.3 hot residency.
/// Pure additive bias (no new mass cap). Value < goal factor to preserve goal primacy.
const NREM_TRAINING_BIAS_FACTOR: f32 = 2.0;

fn run_nrem_consolidation(store: &crate::store::SharedStore) {
    info!("[NREM] Starting dream consolidation pass (CRS threshold = {}) — CodeLand-enhanced (ego-friction + Riemannian + Tier5 ZEDOS + Logenergetics)…", NREM_CRS_THRESHOLD);

    // Collect all concept names while holding the lock briefly.
    let concepts: Vec<String> = {
        let lock = store.lock().unwrap();
        lock.list()
    };

    // ── Item 1 NREM Ego/Goal Bias Pre-computation (recency-weighted + mass-capped) ──
    // Collect active goals + the traces that serve them via explicit "serves" relations.
    // This is the authoritative, relation-driven set (much stronger than payload scanning).
    // Performed once at the start of the infrequent NREM pass.
    let (active_goal_concepts, serving_trace_concepts): (std::collections::HashSet<String>, std::collections::HashSet<String>) = {
        let mut lock = store.lock().unwrap();
        let active_goals: Vec<String> = lock.recall("goal:", 30)
            .into_iter()
            .filter(|m| crate::store::goal_status_is_active(&m.provlog))
            .map(|m| m.concept)
            .collect();

        let mut serving = std::collections::HashSet::new();
        for g in &active_goals {
            let edges = lock.search_relations(g, Some("serves"), "to");
            for (trace_concept, _) in edges {
                serving.insert(trace_concept);
            }
        }
        (
            active_goals.into_iter().collect(),
            serving
        )
    };
    // Note: recency weighting is implicit — only high-momentum/recently active serving traces stay high-CRS enough to be selected in recall + relations.
    // A future pass can add explicit timestamp / recent_access filtering on the serving set.

    // Phase 2.1 Geo Ubiquity: snapshot current SymplecticState ONCE for all NREM contributor logs + hot promotions.
    // This carries live geo frame (step/origin/lens) into the ego.leg3 provenance and per-artifact debug logs
    // for mark_hot of TRAINING/tile/trace/goal artifacts. Respects the 5th coordinate in hot cache paths.
    let (nrem_geo_origin, nrem_geo_step, nrem_geo_has_lens) = {
        let l = store.lock().unwrap();
        if let Some(state) = l.current_geosphere_state() {
            (state.frame_origin.unwrap_or_else(|| "native".to_string()), state.frame_step, state.current_lens.is_some())
        } else {
            ("native".to_string(), 0, false)
        }
    };

    // ═══ CodeLand Phase 4 (WS1-A): Load prev full ego block for friction gate + explicit q/p trajectory evolution ═══
    // Uses storage::read_block (O_DIRECT capable) for previous HolographicBlock to obtain prev_q (reference),
    // prev_p (momentum base), prev_step/prev_h for full Logenergetics evolution + provenance.
    // Zero/identity safe for genesis. Never mutates candidates. Preserves all invariants.
    let (reference_ego, prev_p, prev_step, prev_h_out): ([engram_core::Complex32; 8192], [engram_core::Complex32; 8192], u32, f32) = {
        let ego_path = std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".engram").join("ego.leg3"))
            .unwrap_or_default();
        if ego_path.exists() {
            match engram_core::storage::read_block(&ego_path) {
                Ok(prev_block) => (
                    prev_block.q,
                    prev_block.p,
                    prev_block.energetics.step,
                    prev_block.energetics.h_out,
                ),
                Err(_) => (
                    [engram_core::Complex32::new(0.0, 0.0); 8192],
                    [engram_core::Complex32::new(1.0, 0.0); 8192], // mint identity
                    0,
                    0.0,
                ),
            }
        } else {
            (
                [engram_core::Complex32::new(0.0, 0.0); 8192],
                [engram_core::Complex32::new(1.0, 0.0); 8192],
                0,
                0.0,
            )
        }
    };

    let mut accumulator = [engram_core::Complex32::new(0.0_f32, 0.0_f32); 8192];
    let mut contributors: u32 = 0;
    let mut biased_mass: f32 = 0.0;
    let mut friction_encountered = false;
    let mut total_heat: f32 = 0.0;

    for concept in &concepts {
        // Skip internal bookkeeping concepts.
        if concept.starts_with('_') { continue; }

        let block_opt: Option<engram_core::types::Leg3Pointer> = {
            let lock = store.lock().unwrap();
            lock.fetch_block(concept)
        };

        if let Some(block) = block_opt {
            if block.crs_score >= NREM_CRS_THRESHOLD {
                // ═══ CodeLand Ego-Friction Gate (hermitian_cos_magnitude vs reference_ego) ═══
                let cos = engram_core::ops::cosine_similarity(&block.q, &reference_ego);
                let herm_cos = engram_core::ops::hermitian_cos_magnitude(&block.q, &reference_ego);
                let is_friction = herm_cos < 0.30 || cos < 0.30;

                let mut weight: f32 = 1.0;
                let is_active_goal = active_goal_concepts.contains(concept);
                let is_serving = serving_trace_concepts.contains(concept);
                let is_training = block.zedos_tag == engram_core::ZEDOS_TRAINING;

                // Phase 2.5 harmonic-rich detection (lightweight, payload marker from enriched TRAINING/ego emissions)
                // + sacred 432Hz (genesis/ops) for NREM bias preference + hot residency (2.3). No layout impact.
                let block_payload_text = String::from_utf8_lossy(&block.payload);
                let is_harmonic_rich = is_training || block_payload_text.contains("harmonic_432hz") || block_payload_text.contains("432Hz") || block_payload_text.contains("sacred_freq=432");

                // WS1-B + Phase 2.5 (charter from tile:formal_spec_substrate-phase2-execution-plan-v1 + child goals incl. 1780185084..._sub4):
                // Systematically populate hot_set and call promote (via mark_hot) for high-CRS substrate artifacts
                // (tiles, rich traces, high-value relations/goal-serving participants, TRAINING blocks, harmonic-rich).
                // This ensures canonical fast path ... + harmonic-rich for 2.3 hot residency of 432Hz symplectic blocks.
                // No HolographicBlock layout changes.
                if concept.starts_with("tile:") ||
                   concept.starts_with("trace:") ||
                   concept.starts_with("goal:") ||
                   is_active_goal ||
                   is_serving ||
                   is_training ||
                   is_harmonic_rich {
                    let h = store.lock().unwrap();
                    h.mark_hot(concept);
                    debug!("[NREM][WS1-B+2.5][geo:{}@step={} lens={}] promoted high-CRS substrate artifact to hot_set: {} (crs={:.2}, goal/serving={}, training={}, harmonic={})",
                        nrem_geo_origin, nrem_geo_step, nrem_geo_has_lens, concept, block.crs_score, is_active_goal || is_serving, is_training, is_harmonic_rich);
                }

                if is_active_goal || is_serving {
                    weight = NREM_GOAL_BIAS_FACTOR;
                    let block_mass: f32 = block.q.iter().map(|c| c.re.abs() + c.im.abs()).sum();
                    biased_mass += block_mass * weight;
                }
                if is_training || is_harmonic_rich {
                    weight = NREM_TRAINING_BIAS_FACTOR * if is_harmonic_rich { 1.15 } else { 1.0 }; // extra preference for harmonic-rich (Phase 2.5)
                }
                let cap_mass = NREM_GOAL_BIASED_MASS_CAP * 8192.0 * 2.0;
                if (is_active_goal || is_serving) && biased_mass > cap_mass {
                    weight *= 0.55;
                }

                // ═══ Route: Friction → Abbreviated ADR/KDK recon (Tier5 SYNTHESIS delta path) ═══
                // Resonant → Riemannian RK4 + AttractionField geodesic pre-step (fidelity)
                let contrib_q: [engram_core::Complex32; 8192];
                if is_friction {
                    friction_encountered = true;
                    let (reconciled, crs_p, dl) = engram_core::ops::abbreviated_adr_kdk_reconcile(
                        &block.q, &reference_ego, 12, 0.30, 0.10
                    );
                    // Strict Tier5: only commit if Kepler-like gate + non-divergent dl (never pollute raw)
                    if crs_p >= 0.74 && dl > -0.01 {
                        contrib_q = reconciled;
                        total_heat += engram_core::LAW_CONSTANT;
                        // (In full: could mint separate ZEDOS_SYNTHESIS delta block here under "nrem_delta:..." concept
                        //  for purity — MVP logs the intent; ego itself stays clean NREM_CENTROID)
                    } else {
                        // apophatic/suspend: skip or minimal weight (preserves objective pool purity)
                        contrib_q = block.q; // fallback but will be low-wt or skipped in practice
                        weight *= 0.1;
                    }
                } else {
                    // Resonant path: geodesic pre-evolution toward current acc/ego for manifold fidelity
                    let pre_target = if contributors > 0 { &accumulator } else { &reference_ego };
                    let evolved = engram_core::ops::riemannian_nrem_pre_step(
                        &block.q, pre_target, 4, 0.1_f32, 0.4_f32
                    );
                    // Polysemy curvature detection for special handling (split to SYNTH if high conflict)
                    let curv = engram_core::ops::polysemy_curvature(&evolved, &block.q, pre_target);
                    if curv > 0.28 {
                        friction_encountered = true;
                        contrib_q = evolved; // treat as synthesis-grade
                        total_heat += engram_core::LAW_CONSTANT * 1.2; // higher cost for conflict
                    } else {
                        contrib_q = evolved;
                    }
                }

                // Accumulate (post-processed q)
                for i in 0..8192 {
                    accumulator[i].re += contrib_q[i].re * weight;
                    accumulator[i].im += contrib_q[i].im * weight;
                }
                contributors += 1;
                total_heat += engram_core::LAW_CONSTANT; // cost on every high-signal participant
            }
        }
    }

    if contributors == 0 {
        info!("[NREM] No blocks above CRS threshold — ego.leg3 not updated.");
        return;
    }

    // L2-normalize the accumulator onto the unit hypersphere.
    let norm_sq: f32 = accumulator.iter().map(|c| c.re * c.re + c.im * c.im).sum();
    let norm = norm_sq.sqrt();
    if norm < f32::EPSILON {
        info!("[NREM] Accumulator degenerate (zero norm) — ego.leg3 not updated.");
        return;
    }
    let inv = 1.0 / norm;
    for c in &mut accumulator {
        c.re *= inv;
        c.im *= inv;
    }

    // ═══ Mint ego_block with STRICT CodeLand Tier5 ZEDOS + differentiated Logenergetics + explicit q/p evolution (WS1-A) ═══
    // Never pollutes raw oracle/high-CRS pool (deltas handled via recon paths above; ego is always the reconciled centroid).
    // ego.leg3 is now first-class: full HolographicBlock with evolved p (momentum), complete Logenergetics (H/τ/tau/provenance), rich ProvLog.
    let mut ego_block = engram_core::types::Leg3Pointer::mint();
    ego_block.q = accumulator;

    // Explicit p evolution: momentum conjugate to q-trajectory during consolidation (symplectic-style discrete step).
    // p_new = normalize( decay * prev_p + scaled_delta_kick ). Preserves unit hypersphere invariant on p (and q already normalized).
    // Delta from reference (prev_q) captures the "velocity" of this NREM centroid update.
    let mut delta = [engram_core::Complex32::new(0.0, 0.0); 8192];
    for i in 0..8192 {
        delta[i].re = accumulator[i].re - reference_ego[i].re;
        delta[i].im = accumulator[i].im - reference_ego[i].im;
    }
    let dnorm: f32 = delta.iter().map(|c| c.re * c.re + c.im * c.im).sum::<f32>().sqrt();
    if dnorm > 1e-8 {
        let idn = 0.25 / dnorm; // conservative kick; keeps p evolution stable on manifold
        for c in &mut delta {
            c.re *= idn;
            c.im *= idn;
        }
    }
    let mut new_p = [engram_core::Complex32::new(0.0, 0.0); 8192];
    for i in 0..8192 {
        new_p[i].re = prev_p[i].re * 0.82 + delta[i].re; // momentum persistence + velocity from q-delta
        new_p[i].im = prev_p[i].im * 0.82 + delta[i].im;
    }
    let pnorm: f32 = new_p.iter().map(|c| c.re * c.re + c.im * c.im).sum::<f32>().sqrt();
    if pnorm > f32::EPSILON {
        let ip = 1.0 / pnorm;
        for c in &mut new_p {
            c.re *= ip;
            c.im *= ip;
        }
    }
    ego_block.p = new_p;

    // Primary tag: NREM_CENTROID (resonant synthesis). Friction paths already paid SYNTHESIS-grade costs upstream.
    ego_block.zedos_tag = engram_core::ZEDOS_NREM_CENTROID;
    ego_block.crs_score = 1.0;
    ego_block.superposition_count = contributors;

    // Full Logenergetics (H, τ, tau, provenance, all fields) per formal_spec + child goal.
    // ts/step/H/tau/work_verb/dv populated for rich trajectory + thermodynamic audit. No layout change.
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    ego_block.energetics.ts = now_ts;
    ego_block.energetics.step = prev_step + 1;
    ego_block.energetics.h_in = prev_h_out;
    let h_out = total_heat + (contributors as f32 * engram_core::LAW_CONSTANT * 0.08);
    ego_block.energetics.h_out = h_out;
    let work_proxy: f32 = (biased_mass.max(1.0) * 0.0008).min(10.0); // proxy for consolidation "work" (biased mass throughput)
    ego_block.energetics.work_verb = work_proxy;
    ego_block.energetics.heat_dissipated = total_heat.max(engram_core::LAW_CONSTANT);
    if friction_encountered {
        // Friction/synthesis-heavy: higher deny/reconcile (A/D/R pressure preserved)
        ego_block.energetics.alpha_d = 0.30;
        ego_block.energetics.alpha_r = 0.70;
        ego_block.energetics.alpha_a = 0.40;
    } else {
        // Pure resonant: retention-max (alpha_d=0)
        ego_block.energetics.alpha_d = 0.0;
        ego_block.energetics.alpha_r = 1.0;
        ego_block.energetics.alpha_a = 0.8;
    }
    ego_block.energetics.crs = 1.0;
    ego_block.energetics.dv = (1.0 - engram_core::ops::cosine_similarity(&accumulator, &reference_ego)).max(0.0);
    ego_block.energetics.control_action = if friction_encountered { 0x02 } else { 0x01 }; // recon vs direct
    ego_block.energetics.tau = if friction_encountered { 0.17 } else { 0.025 }; // torsion proxy from polysemy/friction
    ego_block.energetics.zpl_state = 0;

    // Provenance recording for the consolidation step (makes ego.leg3 a proper queryable HolographicBlock with rich energetics + audit trail).
    // Uses existing Cap'n Proto ProvLog path (also sets ego_coherence). Includes all key signals + invariant assertions.
    let prov_text = format!(
        "NREM consolidation (WS1-A charter): {} high-CRS contributors (goal-bias factor {} + TRAINING-bias {} applied under mass cap). Friction encountered: {}. Total heat cost: {:.6}. \
         q evolved as resonant centroid (riemannian_nrem_pre_step + abbreviated_adr_kdk_reconcile paths). p explicitly evolved as momentum (prev_p*decay + scaled delta_q kick from reference; both q/p L2-unit). \
         Full Logenergetics populated: ts={}, step={}, H_in={:.6}, H_out={:.6}, work_verb={:.6}, tau={:.4}, dv={:.4}, heat={:.6}, alphas(A/D/R)={:.2}/{:.2}/{:.2}, control=0x{:02X}. \
         Provenance: contributors from high-CRS + active goals (serves relations) + ZEDOS_TRAINING (Phase 2.5 harmonic-rich). Child goal: goal:1780165889_substrate-cs--embodiment-layer-hardening_sub0 + goal:1780185084_phase-2-5-432hz-symplectic-harmonics---r_sub4. \
         Harmonic 432Hz (Phase 2.5): sacred_freq=432.0 (genesis SACRED_FREQUENCY_HZ); phase relations via integer multiples of π/432 (ops::apply_temporal_phase); symplectic coupling to SymplecticState (2.1 geo + 2.3 hot); lightweight in ego.leg3 ProvLog + TRAINING payload (no layout); NREM bias + hot promotion for harmonic-rich trajectories (richer CLS recursive LoRA medium for Grok self-model). \
         Phase 2.1 Geo snapshot at NREM start: origin={}, frame_step={}, lens_present={}. All hot promotions (mark_hot for tiles/traces/goals/TRAINING) now carry geo via StoreHandle.hot_geo_context; contributor logs tagged with live geosphere (universal frame respect in hot embodiment). \
         Invariants preserved: .leg3 isomorphism, zero-copy hardware paths (O_DIRECT), unit hypersphere (q and p), Allowed Transforms, CRS semantics, no HolographicBlock layout/BLOCK_SIZE change. \
         Written via write_block + write_provlog. Source: run_nrem_consolidation@daemon.rs post spatial ritual + trace:1780170642 + Phase 2.5 432Hz embedding.",
        contributors,
        NREM_GOAL_BIAS_FACTOR,
        NREM_TRAINING_BIAS_FACTOR,
        friction_encountered,
        total_heat,
        ego_block.energetics.ts,
        ego_block.energetics.step,
        ego_block.energetics.h_in,
        ego_block.energetics.h_out,
        ego_block.energetics.work_verb,
        ego_block.energetics.tau,
        ego_block.energetics.dv,
        ego_block.energetics.heat_dissipated,
        ego_block.energetics.alpha_a,
        ego_block.energetics.alpha_d,
        ego_block.energetics.alpha_r,
        ego_block.energetics.control_action,
        nrem_geo_origin,
        nrem_geo_step,
        nrem_geo_has_lens
    );
    engram_core::storage::write_provlog(&mut ego_block, &prov_text);

    // Write to ~/.engram/ego.leg3 — canonical (layout-invariant).
    let ego_path = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h).join(".engram").join("ego.leg3"),
        Err(_) => {
            error!("[NREM] HOME env var not set — cannot write ego.leg3");
            return;
        }
    };

    match engram_core::storage::write_block(&ego_path, &ego_block) {
        Ok(_) => {
            info!("[NREM] ego.leg3 (WS1-A + Phase 2.5 432Hz harmonic: q/p evolved + full Logenergetics + provenance + harmonic marker) updated from {} blocks (norm={:.4}, friction={}, heat={:.6}, step={}, tau={:.4}, zedos=0x{:02X}). Harmonic-rich bias applied for richer CLS training medium. Refreshing Ego gate…",
                contributors, norm, friction_encountered, total_heat, ego_block.energetics.step, ego_block.energetics.tau, ego_block.zedos_tag);
            let mut lock = store.lock().unwrap();
            lock.refresh_ego_q();
            info!("[NREM] Ego gate refreshed (Tier5 deltas respected; explicit p momentum + rich energetics + ProvLog for queryable trajectory; geometric fidelity + continuity improved).");
        }
        Err(e) => {
            error!("[NREM] Failed to write ego.leg3: {}", e);
        }
    }
}

// ── System Health Watchdog ────────────────────────────────────────────────────
//
// Config-driven: reads ~/.engram/watchdog.toml for process names to monitor
// and where to write agency proposals. If the config file doesn't exist this
// function is a no-op — Engram has zero coupling to any consumer project.
//
// Called every 5 minutes from the daemon select! loop.

fn run_health_watchdog(
    cfg: &crate::watchdog::WatchdogConfig,
    proposals_path: &std::path::Path,
) {
    if cfg.watch.is_empty() {
        return; // no-op — watchdog.toml absent or empty
    }

    for process in &cfg.watch {
        if !crate::watchdog::is_process_alive(&process.name) {
            info!(
                "[Watchdog] '{}' not detected — minting agency proposal.",
                process.name
            );
            crate::watchdog::mint_proposal(process, proposals_path);
        } else {
            tracing::trace!("[Watchdog] '{}' alive ✓", process.name);
        }
    }
}


pub struct DaemonControl {
    pub active_watch: Arc<tokio::sync::RwLock<Option<PathBuf>>>,
    shutdown: Arc<AtomicBool>,
    watch_tx: flume::Sender<PathBuf>,
}

impl DaemonControl {
    pub async fn set_watch_workspace(&self, path: impl AsRef<Path>) {
        let p = path.as_ref().to_path_buf();
        let mut lock = self.active_watch.write().await;
        *lock = Some(p.clone());
        let _ = self.watch_tx.send(p);
    }
}
