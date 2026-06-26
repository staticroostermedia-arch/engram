#![recursion_limit = "512"]
#![allow(unused_mut)]
// many guard bindings from short-lock refactor are read-only after acquire; harmless, silences the bulk of the "does not need to be mutable" warnings
#![allow(clippy::type_complexity)]
// complex tuple types in KI bake (pinned + weighted) are inherent to the geometric payload; documented
#![allow(clippy::ptr_arg)] // &PathBuf in bake_ki is fine for the ki_dir usage; changing would cascade to callers

//! Engram server — MCP + REST memory backend for AI agents.
//!
//! # Modes
//!
//! **MCP mode (default)** — reads JSON-RPC 2.0 from stdin, writes to stdout.
//! Used by Claude Desktop, Cursor, and any MCP-compatible client.
//!
//! ```sh
//! engram mcp [--store ~/.engram/manifold]
//! ```
//!
//! **REST mode** — HTTP server on localhost. Used by custom integrations and the dynamic leg-browser GUI.
//!
//! ```sh
//! engram serve [--port 3456] [--store ~/.engram/manifold] [--light] [--no-scout]
//! ```
//!
//! Flags for reliable leg-browser / GUI use (see scripts/launch-leg-browser-review.sh):
//!   --light     : Force CPU backend (ENGRAM_FORCE_CPU_BACKEND), skips CUDA/Metal/BVH heavy init for fast non-GPU startup during UI testing.
//!   --no-scout  : Skip scout_daemon supervisor (avoids port 8088 contention/spam when only using /api/* for dynamic views).

mod cockpit_cache;
mod coherence;
mod context_var;
pub mod daemon;
mod edit_arc_gate;
mod edit_fidelity;
mod evolution_at_locus;
mod fidelity_few_shots;
mod goal_hygiene;
mod harness_injection;
mod injection_priority;
pub mod ki_hijacker;
mod leg_corpus;
mod linguistic_reference_frame;
mod local_stratum;
mod mcp;
mod mcp_lock;
mod mirror;
mod presentation_stratum;
mod process_metrics;
mod profile;
pub mod scout;
pub mod scout_supervisor;
mod scrub_export;
mod serve;
mod session_lifecycle;
mod solid_state_tensor;
mod store;
mod tensor_tile_bridge;
mod tile_draft;
mod turn_extract;
mod wake_bundle;
mod wake_queue_gate;
pub mod watchdog;

use clap::{Parser, Subcommand};
// open_store is now called inside the command arms (fast path for MCP, full for Serve)
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser)]
#[command(
    name    = "engram",
    version = env!("CARGO_PKG_VERSION"),
    about   = "Persistent geometric memory for AI agents",
    long_about = None,
)]
struct Cli {
    /// Directory to store .leg memory blocks
    #[arg(
        long,
        global = true,
        default_value = "~/.engram/manifold",
        env = "ENGRAM_STORE"
    )]
    store: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run as an MCP server over stdin/stdout (for Claude Desktop, Cursor, etc.)
    Mcp {
        /// Skip seeding alignment genesis blocks on first boot
        #[arg(long, default_value_t = false)]
        no_genesis: bool,
    },

    /// Block until the geometric store is loaded and ready (for engram-grok / TUI MCP launcher).
    /// Performs full StoreHandle init (not the MCP fast-placeholder). Warms large manifolds
    /// before the stdio MCP subprocess starts so native use_tool sees a healthy backend sooner.
    WaitReady {
        /// Max seconds to wait for full store initialization (default 180)
        #[arg(long, default_value_t = 180)]
        timeout: u64,
        /// Emit readiness JSON to stdout on success (for launch/resume evidence scripts)
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Run as a REST HTTP server
    Serve {
        /// Port to listen on
        #[arg(long, default_value_t = 3456)]
        port: u16,

        /// Skip seeding alignment genesis blocks on first boot
        #[arg(long, default_value_t = false)]
        no_genesis: bool,

        /// Light / UI-test mode for leg-browser dynamic GUI: force CPU-only backend (no CUDA/Metal/GPU BVH heavy init). Fast startup, sufficient for /api/block /api/hydrate /api/recent etc. (see parent goal:1780106168)
        #[arg(long, default_value_t = false)]
        light: bool,

        /// LEG glass-box cockpit profile (GPU-hot presentation cache, lazy galaxy). Preferred over --light for live LEG.
        #[arg(long, default_value_t = false)]
        cockpit: bool,

        /// Disable the scout_daemon.py supervisor (port 8088 web-search companion). Recommended with --light when only using serve for live leg-browser views (no /api/scout needed).
        #[arg(long, default_value_t = false)]
        no_scout: bool,

        /// Enable the Streamable HTTP MCP transport at POST /mcp.
        /// Allows multiple agents (Grok, Antigravity) to share ONE engram process
        /// instead of each spawning their own private stdio subprocess.
        /// Config: set `url = "http://127.0.0.1:<port>/mcp"` in your MCP client config.
        #[arg(long, default_value_t = false)]
        mcp_http: bool,
    },
}

fn main() -> anyhow::Result<()> {
    // Log to stderr so stdout stays clean for MCP protocol
    fmt()
        .with_env_filter(
            EnvFilter::try_from_env("ENGRAM_LOG").unwrap_or_else(|_| EnvFilter::new("engram=info")),
        )
        .with_writer(std::io::stderr)
        .without_time()
        .init();

    // Raise fd limit early for large stores (181k+ .leg files). Default soft ulimit 1024 causes EMFILE
    // during bvh build (scan_dir opens many .leg), spatial force_ingest, hot cache, etc.
    // This is required for the CUDA/BVH/NVMe-GPU path to work on real data without "too many open files".
    // Hard limit is usually high (1M+); we raise soft to 64k.
    // Ties to "NVMe to GPU" design + large manifold support post our bvh guard lift.
    raise_fd_limit();

    let cli = Cli::parse();

    // Boot scout daemon in background — only for HTTP serve mode (unless --no-scout for minimal leg-browser UI use).
    // In MCP mode the scout is not needed and its port-8088 startup
    // noise on stderr would corrupt the JSON-RPC protocol stream.
    // Linked to sub-goal:1780106172 (diagnose/stabilize serve for seamless dynamic leg-browser under parent goal:1780106168).
    if let Commands::Serve { no_scout, .. } = &cli.command {
        if !no_scout {
            scout_supervisor::boot();
        } else {
            tracing::info!("[SERVE] --no-scout: skipping scout_daemon supervisor (cleaner for leg-browser dynamic GUI testing).");
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    match cli.command {
        Commands::WaitReady { timeout, json } => {
            use std::sync::mpsc;
            use std::time::Duration;

            profile::EngramProfile::from_env().apply();
            maybe_defer_bvh_for_large_store(&cli.store);

            tracing::info!(
                "[wait-ready] Loading store at '{}' (timeout {}s) before MCP spawn…",
                cli.store,
                timeout
            );

            let path = cli.store.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let store = store::StoreHandle::new(&path);
                store.mark_fully_initialized();
                let readiness = store.backend_readiness();
                let hot = store.hot_concepts().len();
                let _ = tx.send((readiness, hot));
            });

            match rx.recv_timeout(Duration::from_secs(timeout)) {
                Ok((readiness, hot)) => {
                    tracing::info!(
                        "[wait-ready] Store ready (hot_concepts={}, readiness={})",
                        hot,
                        readiness
                    );
                    if json {
                        let payload = serde_json::json!({
                            "status": "ready",
                            "hot_concepts": hot,
                            "readiness": readiness,
                        });
                        println!(
                            "{}",
                            serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into())
                        );
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    tracing::error!(
                        "[wait-ready] Timed out after {}s — MCP may start on fast-placeholder; \
                         poll mcp_engram_get_backend_readiness after wake.",
                        timeout
                    );
                    std::process::exit(1);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::error!("[wait-ready] Init thread exited before signaling readiness");
                    std::process::exit(1);
                }
            }
        }
        Commands::Mcp { no_genesis } => {
            profile::EngramProfile::from_env().apply();
            let _mcp_lock = mcp_lock::McpStoreLock::acquire(&cli.store)?;

            // === Early MCP Ready Path ===
            // Create an ultra-light placeholder instantly so we can answer
            // initialize / tools/list before the heavy manifold + GPU + BVH work.
            // The real work happens in the background (or on first real tool use).
            let store = store::open_store_placeholder_for_mcp(&cli.store);

            if !no_genesis {
                // Genesis seeding is cheap enough to do even on the placeholder path.
                match store.lock().unwrap().seed_genesis() {
                    Ok(msg) => tracing::info!("{msg}"),
                    Err(e) => tracing::warn!("Genesis seed failed: {e}"),
                }
            }

            let _guard = rt.enter();

            // Kick off the REAL heavy initialization in the background.
            // This loads the full Sheaf/Cuda backends, BVH indexes, embed matrix, etc.
            // We create a dedicated Tokio runtime inside this thread so that
            // boot_daemon / ki_hijacker (which use tokio::spawn) do not panic.
            let store_for_upgrade = store.clone();
            let real_path = cli.store.clone();
            std::thread::spawn(move || {
                // Brief pause so stdio MCP can finish initialize/handshake before heavy
                // backend work (StoreHandle::new, daemon, BVH). Prevents CI harness races
                // where the subprocess aborts before the client sees any stderr/logs.
                std::thread::sleep(std::time::Duration::from_millis(1200));
                tracing::info!("[MCP-FAST] Starting full manifold initialization in background...");
                maybe_defer_bvh_for_large_store(&real_path);

                // Create a minimal multi-thread runtime just for this init thread.
                let rt = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("[MCP-FAST] Failed to create background runtime: {e}");
                        return;
                    }
                };

                let _rt_guard = rt.enter();

                let mut full = store::StoreHandle::new(&real_path);

                if !no_genesis {
                    match full.seed_genesis() {
                        Ok(msg) => tracing::info!("[MCP-FAST] {msg}"),
                        Err(e) => tracing::warn!("[MCP-FAST] Genesis seed failed: {e}"),
                    }
                }

                // Hot-swap into the SAME Arc the MCP stdio loop uses (fixes split-brain).
                {
                    let mut guard = store_for_upgrade.lock().unwrap();
                    guard.upgrade_from(full);
                    guard.mark_fully_initialized();
                }

                store::StoreHandle::boot_daemon(store_for_upgrade.clone());
                let _hijacker = ki_hijacker::spawn(store_for_upgrade.clone());

                tracing::info!(
                    "[MCP-FAST] Full initialization complete — MCP tools now use real backend."
                );

                // Keep the runtime alive for the background work.
                std::mem::forget(rt);
            });

            // Daemon + ki_hijacker start only after upgrade (same Arc as MCP loop).
            tracing::info!("[MCP-FAST] Fast MCP path active — replying to protocol immediately.");

            mcp::run(store)?;
        }
        Commands::Serve {
            port,
            no_genesis,
            light,
            cockpit,
            no_scout: _,
            mcp_http,
        } => {
            // Serve mode (HTTP) — stabilized for leg-browser dynamic GUI (parent goal:1780106168_make-the-leg-browser-a-seamless--truly-dynamic-g ; sub0:1780106172).
            if cockpit {
                if std::env::var("ENGRAM_PROFILE").is_err() {
                    std::env::set_var("ENGRAM_PROFILE", "cockpit");
                }
                profile::EngramProfile::Cockpit.apply();
                tracing::info!(
                    "[SERVE] --cockpit: glass-box LEG profile (presentation cache + lazy galaxy)."
                );
            } else if light {
                if std::env::var("ENGRAM_PROFILE").is_err() {
                    std::env::set_var("ENGRAM_PROFILE", "ui");
                }
                profile::EngramProfile::Ui.apply();
                tracing::info!("[SERVE] --light: ui profile (CPU-only, fast leg-browser).");
            } else if std::env::var("ENGRAM_PROFILE").is_ok() {
                profile::EngramProfile::from_env().apply();
            }

            // Serve mode (HTTP) can afford the full heavy initialization (unless light).
            let store = store::open_store(&cli.store);

            if !no_genesis {
                match store.lock().unwrap().seed_genesis() {
                    Ok(msg) => tracing::info!("{msg}"),
                    Err(e) => tracing::warn!("Genesis seed failed: {e}"),
                }
            }

            let _guard = rt.enter();

            // ── Boot file-watcher daemon ──────────────────────────────────────
            store::StoreHandle::boot_daemon(store.clone());

            // ── Boot KI Hijacker — — — — — — — — — — — — — — — — — — — — — — —
            let _hijacker = ki_hijacker::spawn(store.clone());
            tracing::info!("[KI_HIJACKER] Logophysical Antigravity Bridge spawned (REST mode).");

            // Concrete improvement: serve now supports clean shutdown signals (see serve.rs); "Keyboard interrupt received" will be logged on intentional Ctrl-C.
            rt.block_on(serve::run(store, port, mcp_http))?;
        }
    }

    Ok(())
}

/// Defer the memory-heavy BVH full scan on very large stores.
/// Queries fall back to CPU linear scan until BVH is built on demand.
fn maybe_defer_bvh_for_large_store(path: &str) {
    if std::env::var("ENGRAM_DEFER_BVH").is_ok() {
        return;
    }
    let expanded = shellexpand::tilde(path).into_owned();
    let count = std::fs::read_dir(&expanded)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| engram_core::storage::is_leg_block_path(&e.path()))
                .count()
        })
        .unwrap_or(0);

    if count > 50_000 {
        std::env::set_var("ENGRAM_DEFER_BVH", "1");
        tracing::info!(
            "[MCP-FAST] Large manifold (~{count} .leg files) — ENGRAM_DEFER_BVH=1. \
             MCP stays responsive; recall uses CPU scan until BVH built on demand."
        );
    }
}

/// Raise soft RLIMIT_NOFILE early (for large stores with 100k+ .leg files).
/// Prevents EMFILE during bvh::scan_dir / build (opens many .leg), spatial force_ingest
/// (on source + manifold), hot cache residency, etc.
/// Required for reliable CudaBackend + LBVH + device_residency (NVMe-GPU) on real data.
/// We set soft to 64k (or hard if lower); hard is typically 1M+ from OS.
fn raise_fd_limit() {
    unsafe {
        let mut rlim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut rlim) == 0 {
            let target: libc::rlim_t = 65536;
            if rlim.rlim_cur < target {
                let new_cur = if rlim.rlim_max > 0 {
                    target.min(rlim.rlim_max)
                } else {
                    target
                };
                rlim.rlim_cur = new_cur;
                if libc::setrlimit(libc::RLIMIT_NOFILE, &rlim) == 0 {
                    tracing::info!(
                        "[FD] Raised RLIMIT_NOFILE soft limit to {} (was {}; hard {})",
                        new_cur,
                        rlim.rlim_cur,
                        rlim.rlim_max
                    );
                } else {
                    tracing::warn!("[FD] Failed to raise RLIMIT_NOFILE (errno {}) — large store bvh/spatial may hit EMFILE", std::io::Error::last_os_error().raw_os_error().unwrap_or(0));
                }
            } else {
                tracing::info!(
                    "[FD] RLIMIT_NOFILE already >= {} (cur {})",
                    target,
                    rlim.rlim_cur
                );
            }
        }
    }
}
