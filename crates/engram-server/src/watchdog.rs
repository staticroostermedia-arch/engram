//! Engram System Health Watchdog — Configuration Layer
//!
//! Reads `~/.engram/watchdog.toml` at daemon startup. If the file does not
//! exist the watchdog is a no-op — zero coupling to any consumer project.
//!
//! # Example `~/.engram/watchdog.toml`
//!
//! ```toml
//! # Where to write agency healing proposals (defaults to ~/.engram/proposals.json)
//! proposals_path = "~/.engram/proposals.json"
//!
//! [[watch]]
//! name        = "circadian"
//! restart_hint = "nohup ~/Documents/CodeLand/target/release/circadian > /tmp/circadian.log 2>&1 &"
//! severity    = "HIGH"
//! description = "Nightly NREM memory consolidation driver"
//!
//! [[watch]]
//! name        = "my_rag_server"
//! restart_hint = "systemctl start my-rag"
//! severity    = "MEDIUM"
//! description = "Custom embedding server for semantic recall"
//! ```

use serde::Deserialize;
use std::path::PathBuf;

// ── Config Types ──────────────────────────────────────────────────────────────

/// A single process entry in `watchdog.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct WatchedProcess {
    /// Process name as it appears in `/proc/<pid>/comm` (Linux) or `ps` output.
    pub name: String,

    /// Human-readable explanation written into the agency proposal.
    #[serde(default)]
    pub description: String,

    /// The exact shell command the operator can approve to restart this process.
    #[serde(default)]
    pub restart_hint: String,

    /// Proposal severity: "HIGH", "MEDIUM", or "LOW". Defaults to "MEDIUM".
    #[serde(default = "default_severity")]
    pub severity: String,
}

fn default_severity() -> String {
    "MEDIUM".to_string()
}

/// Top-level `watchdog.toml` structure.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WatchdogConfig {
    /// Path where agency healing proposals are written.
    /// Defaults to `~/.engram/proposals.json`.
    /// Override with `ENGRAM_PROPOSALS_PATH` environment variable.
    #[serde(default)]
    pub proposals_path: Option<String>,

    /// Processes to monitor. Each entry generates a proposal when the process
    /// is not found in the process table.
    #[serde(default)]
    pub watch: Vec<WatchedProcess>,
}

impl WatchdogConfig {
    /// Load from `~/.engram/watchdog.toml`.
    ///
    /// Returns an empty (no-op) config if the file does not exist.
    /// Logs a warning if the file exists but fails to parse.
    pub fn load() -> Self {
        // Allow env override for the entire config file path
        let config_path = std::env::var("ENGRAM_WATCHDOG_CONFIG")
            .map(|p| PathBuf::from(shellexpand::tilde(&p).into_owned()))
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join(".engram").join("watchdog.toml"))
                    .unwrap_or_else(|_| PathBuf::from("/tmp/engram_watchdog.toml"))
            });

        if !config_path.exists() {
            tracing::debug!(
                "[Watchdog] No config at {} — health watchdog disabled.",
                config_path.display()
            );
            return Self::default();
        }

        match std::fs::read_to_string(&config_path) {
            Ok(text) => match toml::from_str::<WatchdogConfig>(&text) {
                Ok(cfg) => {
                    tracing::info!(
                        "[Watchdog] Loaded config: {} process(es) to watch.",
                        cfg.watch.len()
                    );
                    cfg
                }
                Err(e) => {
                    tracing::warn!(
                        "[Watchdog] Failed to parse {}: {}. Health watchdog disabled.",
                        config_path.display(),
                        e
                    );
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!(
                    "[Watchdog] Cannot read {}: {}. Health watchdog disabled.",
                    config_path.display(),
                    e
                );
                Self::default()
            }
        }
    }

    /// Resolve the proposals file path.
    ///
    /// Priority:
    ///   1. `ENGRAM_PROPOSALS_PATH` environment variable
    ///   2. `proposals_path` field in `watchdog.toml`
    ///   3. `~/.engram/proposals.json` (Engram-local default)
    pub fn resolved_proposals_path(&self) -> PathBuf {
        // 1. Env var always wins
        if let Ok(p) = std::env::var("ENGRAM_PROPOSALS_PATH") {
            return PathBuf::from(shellexpand::tilde(&p).into_owned());
        }
        // 2. Config file field
        if let Some(ref p) = self.proposals_path {
            return PathBuf::from(shellexpand::tilde(p).into_owned());
        }
        // 3. Default: ~/.engram/proposals.json
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".engram").join("proposals.json"))
            .unwrap_or_else(|_| PathBuf::from("/tmp/engram_proposals.json"))
    }
}

// ── Runtime Health Check ──────────────────────────────────────────────────────

/// Check whether a process with the given name is currently running.
///
/// On Linux: scans `/proc/<pid>/comm`.
/// On non-Linux: always returns `true` (assume alive — watchdog is Linux-native).
pub fn is_process_alive(name: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir("/proc")
            .ok()
            .map(|entries| {
                entries.flatten().any(|e| {
                    // Only numeric entries are PIDs
                    let fname = e.file_name();
                    let is_pid = fname
                        .to_str()
                        .map(|s| s.chars().all(|c| c.is_ascii_digit()))
                        .unwrap_or(false);
                    if !is_pid {
                        return false;
                    }
                    std::fs::read_to_string(e.path().join("comm"))
                        .ok()
                        .map(|s| s.trim() == name)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(true) // if /proc unreadable, assume alive
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        true // macOS/Windows: watchdog is a no-op (use launchd/SCM instead)
    }
}

/// Append a healing proposal to the proposals JSON array on disk.
///
/// Deduplicates: does not write a new proposal if one for the same process
/// name is already in the file (prevents spamming every 5 minutes).
pub fn mint_proposal(process: &WatchedProcess, proposals_path: &std::path::Path) {
    let id = format!(
        "health_{}_{}",
        process.name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    let proposal = serde_json::json!({
        "id": id,
        "type": "SYSTEM_HEALTH",
        "severity": process.severity,
        "title": format!("'{}' process offline", process.name),
        "plain_english": if process.description.is_empty() {
            format!("The '{}' process is not running.", process.name)
        } else {
            process.description.clone()
        },
        "if_approved": if process.restart_hint.is_empty() {
            format!("Manually restart the '{}' process.", process.name)
        } else {
            process.restart_hint.clone()
        },
        "if_rejected": format!(
            "'{}' stays offline. No automatic action will be taken.",
            process.name
        ),
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    });

    // Load existing proposals (or start fresh)
    let mut proposals: Vec<serde_json::Value> = proposals_path
        .exists()
        .then(|| std::fs::read_to_string(proposals_path).ok())
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // Deduplication check: same process name already pending?
    let already_pending = proposals.iter().any(|p| {
        p.get("title")
            .and_then(|t| t.as_str())
            .map(|t| t.contains(&process.name))
            .unwrap_or(false)
    });

    if already_pending {
        tracing::debug!(
            "[Watchdog] Proposal for '{}' already pending — skipping.",
            process.name
        );
        return;
    }

    proposals.push(proposal);

    // Ensure parent directory exists
    if let Some(parent) = proposals_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    match serde_json::to_string_pretty(&proposals) {
        Ok(json) => {
            if let Err(e) = std::fs::write(proposals_path, json) {
                tracing::error!(
                    "[Watchdog] Failed to write proposals to {}: {}",
                    proposals_path.display(),
                    e
                );
            } else {
                tracing::info!(
                    "[Watchdog] Minted SYSTEM_HEALTH proposal for '{}' → {}",
                    process.name,
                    proposals_path.display()
                );
            }
        }
        Err(e) => tracing::error!("[Watchdog] Failed to serialize proposals: {}", e),
    }
}
