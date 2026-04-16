use crate::store::SharedStore;
use notify_debouncer_full::{new_debouncer, notify::*};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

/// Starts the global agentic background daemon attached to the MCP / REST Server.
pub fn spawn(store: SharedStore) -> Arc<DaemonControl> {
    let (watch_tx, watch_rx) = flume::unbounded::<PathBuf>();

    let daemon = Arc::new(DaemonControl {
        active_watch: Arc::new(tokio::sync::RwLock::new(None)),
        shutdown: Arc::new(AtomicBool::new(false)),
        watch_tx,
    });

    let ctrl = daemon.clone();
    
    // Spawn background Autophagy & Watcher processing loop on Tokio
    tokio::spawn(async move {
        // We'll spin up a synchronous flume channel for `notify` since it's blocking
        let (tx, rx) = flume::unbounded();
        
        let mut debouncer = new_debouncer(Duration::from_millis(1500), None, move |res| {
            if let Ok(events) = res {
                for e in events {
                    let _ = tx.send(e);
                }
            }
        }).unwrap();

        info!("Agentic Daemon (Phase 7) online. Garbage Collection active.");

        // Loop checks both incoming watcher events, and periodically runs Garbage Collection
        let mut gc_interval = tokio::time::interval(Duration::from_secs(3600)); // Every hour
        let mut flush_interval = tokio::time::interval(Duration::from_secs(60)); // Flush access timestamps every 60s
        
        loop {
            if ctrl.shutdown.load(Ordering::Relaxed) {
                break;
            }

            tokio::select! {
                new_watch = watch_rx.recv_async() => {
                    if let Ok(p) = new_watch {
                        if let Err(e) = debouncer.watch(&p, RecursiveMode::Recursive) {
                            error!("Daemon failed to bind OS watcher to {}: {}", p.display(), e);
                        } else {
                            info!("Daemon dynamically bound OS watcher to: {}", p.display());
                        }
                    }
                }

                _ = flush_interval.tick() => {
                    // Flush hot access timestamps to disk every 60 seconds
                    let mut lock = store.lock().unwrap();
                    lock.access_index.flush_if_dirty();
                }

                _ = gc_interval.tick() => {
                    info!("Daemon: Initiating Tiered Decay Garbage Collection (Autophagy)...");
                    let mut lock = store.lock().unwrap();
                    let concepts = lock.list();
                    let mut culled = 0;
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                    
                    for c in concepts {
                        // We fetch the memory block. We skip anything pinned (crs == 1.0)
                        if let Some(mut m) = lock.fetch_block(&c) {
                            if m.crs_score < 1.0 {
                                let hours_stale = now.saturating_sub(m.last_accessed_timestamp) as f32 / 3600.0;
                                let mut decayed = false;

                                // Praxis-style tiered decay curve
                                if hours_stale > 168.0 {
                                    // > 7 days stale: aggressive 5% penalty per pass
                                    m.crs_score *= 0.95;
                                    decayed = true;
                                } else if hours_stale > 24.0 {
                                    // > 1 day stale: mild 2% penalty per pass
                                    m.crs_score *= 0.98;
                                    decayed = true;
                                }

                                if decayed {
                                    if m.crs_score < 0.05 {
                                        // Floor hit -> thermodynamic eviction
                                        let _ = lock.forget(&c);
                                        culled += 1;
                                    } else {
                                        // Save the decayed CRS back to NVMe
                                        let _ = lock.store(&c, m);
                                    }
                                }
                            }
                        }
                    }
                    if culled > 0 {
                        info!("Daemon: Autophagy swept {} dead/stale concept bindings.", culled);
                    }
                }

                event = rx.recv_async() => {
                    if let Ok(ev) = event {
                        if ev.kind.is_modify() || ev.kind.is_create() {
                            for path in &ev.paths {
                                if !path.is_file() { continue; }
                                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                                let allowed_exts = [
                                    "rs", "md", "txt", "js", "ts", "json", "toml", "py", "c", "cpp", "h", "csv", "sh",
                                    "go", "java", "rb", "zig", "php", "html", "css", "yml", "yaml", "sql", "ex", "exs", "swift"
                                ];
                                
                                if allowed_exts.contains(&ext) && !path.to_string_lossy().contains("/target/") && !path.to_string_lossy().contains("/.git/") {
                                    // Re-ingest the file directly into the manifold!
                                    if let Ok(content) = std::fs::read_to_string(path) {
                                        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown");
                                        let concept_name = format!("{}_daemon", file_name.replace('.', "_"));
                                        
                                        // Take a chunk up to 8000 bytes max just for the live context
                                        let safe_end = content.len().min(8000);
                                        let mut end = safe_end;
                                        while end > 0 && !content.is_char_boundary(end) {
                                            end -= 1;
                                        }
                                        
                                        let mut lock = store.lock().unwrap();
                                        if let Err(e) = lock.remember(&concept_name, &content[..end]) {
                                            error!("Daemon failed to auto-sync file {}: {}", path.display(), e);
                                        } else {
                                            debug!("Daemon: Auto-synced real-time modifications to {}", path.display());
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
