//! Exclusive flock lock for MCP stdio servers — one process per store path.
//!
//! Prevents duplicate `engram mcp --store` instances from contending on the same
//! large manifold (BVH rebuilds, CUDA context, fd pressure). This was a root cause
//! of transport failures and 30GB+ RAM spikes when Grok + harness both launched MCP.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct McpStoreLock {
    _file: File,
}

/// Format the double-spawn failure message (pure; unit-tested).
pub fn format_lock_held_message(store_path: &str, lock_path: &Path, holder_pid: &str) -> String {
    format!(
        "Another engram MCP server is already running on store '{store_path}'.\n\
         Lock: {}\n\
         Holder PID (from lock file): {}\n\
         Fix: restart your IDE/TUI (one MCP instance per store), or stop the other process.\n\
         Dev only: ENGRAM_MCP_FORCE_STEAL=1 steals a lock when the holder PID is dead or force-steal is set.",
        lock_path.display(),
        if holder_pid.trim().is_empty() {
            "unknown (lock file empty — live holder still owns flock; check: pgrep -af 'engram.*mcp')"
        } else {
            holder_pid.trim()
        }
    )
}

/// True when `ENGRAM_MCP_FORCE_STEAL=1|true|on` (dev/CI recovery only).
pub fn force_steal_enabled() -> bool {
    matches!(
        std::env::var("ENGRAM_MCP_FORCE_STEAL")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "on" | "yes"
    )
}

/// True if the PID in the lock file is not a live process (orphan).
pub fn holder_pid_is_dead(holder_pid: &str) -> bool {
    let pid: i32 = match holder_pid.trim().parse() {
        Ok(p) if p > 0 => p,
        _ => return true, // empty/garbage → treat as recoverable stale file
    };
    #[cfg(unix)]
    {
        // signal 0 = existence check
        let ret = unsafe { libc::kill(pid, 0) };
        ret != 0 // ESRCH or not permitted → treat as dead/unusable for our purposes when force-steal
            || force_steal_enabled()
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        force_steal_enabled()
    }
}

fn lock_dir() -> PathBuf {
    shellexpand::tilde("~/.engram/locks").into_owned().into()
}

fn lock_path_for_store(store_path: &str) -> PathBuf {
    let expanded = shellexpand::tilde(store_path).into_owned();
    let hash = blake3::hash(expanded.as_bytes());
    lock_dir().join(format!("mcp-{}.lock", &hash.to_hex()[..16]))
}

/// Read holder PID text from lock file (best-effort).
pub fn read_holder_pid(lock_path: &Path) -> String {
    let mut s = String::new();
    if let Ok(mut f) = File::open(lock_path) {
        let _ = f.read_to_string(&mut s);
    }
    s.lines().next().unwrap_or("").trim().to_string()
}

/// Remove orphaned lock files where the recorded PID is not alive.
/// Returns how many files were removed.
pub fn recover_orphan_locks() -> usize {
    let dir = lock_dir();
    if !dir.is_dir() {
        return 0;
    }
    let mut removed = 0usize;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for ent in rd.flatten() {
            let path = ent.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with("mcp-") || !name.ends_with(".lock") {
                continue;
            }
            let pid = read_holder_pid(&path);
            // Only remove if PID is clearly dead (not force_steal for live holders)
            let dead = match pid.trim().parse::<i32>() {
                Ok(p) if p > 0 => {
                    #[cfg(unix)]
                    {
                        unsafe { libc::kill(p, 0) != 0 }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = p;
                        false
                    }
                }
                // empty/garbage lock file — only remove under force-steal
                _ => force_steal_enabled(),
            };
            if dead && std::fs::remove_file(&path).is_ok() {
                tracing::warn!(
                    "[MCP-LOCK] Recovered orphaned lock {} (stale pid {})",
                    path.display(),
                    pid
                );
                removed += 1;
            }
        }
    }
    removed
}

impl McpStoreLock {
    /// Acquire an exclusive non-blocking lock for `store_path`.
    /// Fails fast if another engram MCP process already holds the lock.
    pub fn acquire(store_path: &str) -> anyhow::Result<Self> {
        // Opportunistic orphan recovery before flock (dead PID lock files).
        let _ = recover_orphan_locks();

        let expanded = shellexpand::tilde(store_path).into_owned();
        let lock_dir = lock_dir();
        std::fs::create_dir_all(&lock_dir)?;

        let lock_path = lock_path_for_store(&expanded);

        // Do NOT truncate before flock — a failed competitor would wipe the holder's PID.
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if ret != 0 {
                let other_pid = read_holder_pid(&lock_path);
                // Force-steal: only when env set AND holder dead (or force regardless if env set and kill fails)
                if force_steal_enabled() && holder_pid_is_dead(&other_pid) {
                    tracing::warn!(
                        "[MCP-LOCK] ENGRAM_MCP_FORCE_STEAL: removing lock {} (pid={})",
                        lock_path.display(),
                        other_pid
                    );
                    drop(file);
                    let _ = std::fs::remove_file(&lock_path);
                    // Retry once
                    return Self::acquire_raw(&expanded, &lock_path);
                }
                anyhow::bail!(
                    "{}",
                    format_lock_held_message(&expanded, &lock_path, &other_pid)
                );
            }
        }

        #[cfg(not(unix))]
        {
            let _ = &lock_path;
        }

        file.set_len(0)?;
        writeln!(file, "{}", std::process::id())?;

        tracing::info!(
            "[MCP-LOCK] Acquired exclusive lock for store '{}' (pid={})",
            expanded,
            std::process::id()
        );

        Ok(Self { _file: file })
    }

    fn acquire_raw(expanded: &str, lock_path: &Path) -> anyhow::Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if ret != 0 {
                let other_pid = read_holder_pid(lock_path);
                anyhow::bail!(
                    "{}",
                    format_lock_held_message(expanded, lock_path, &other_pid)
                );
            }
        }
        file.set_len(0)?;
        writeln!(file, "{}", std::process::id())?;
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static L: OnceLock<Mutex<()>> = OnceLock::new();
        L.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn format_lock_held_includes_pid_and_store() {
        let msg = format_lock_held_message(
            "/home/a/.engram/stalks/",
            Path::new("/home/a/.engram/locks/mcp-deadbeef.lock"),
            "391632",
        );
        assert!(msg.contains("391632"), "{msg}");
        assert!(msg.contains("/home/a/.engram/stalks/"), "{msg}");
        assert!(msg.contains("mcp-deadbeef.lock"), "{msg}");
        assert!(msg.contains("Another engram MCP server"), "{msg}");
    }

    #[test]
    fn format_lock_held_empty_pid_hint() {
        let msg = format_lock_held_message("/tmp/s", Path::new("/tmp/l.lock"), "  ");
        assert!(msg.contains("unknown") || msg.contains("empty"), "{msg}");
    }

    #[test]
    fn force_steal_env_parsed() {
        let _g = env_lock();
        std::env::remove_var("ENGRAM_MCP_FORCE_STEAL");
        assert!(!force_steal_enabled());
        std::env::set_var("ENGRAM_MCP_FORCE_STEAL", "1");
        assert!(force_steal_enabled());
        std::env::remove_var("ENGRAM_MCP_FORCE_STEAL");
    }

    #[test]
    fn dead_pid_is_dead() {
        // PID 1 usually exists; use impossible PID
        assert!(holder_pid_is_dead("999999999"));
        assert!(holder_pid_is_dead(""));
        assert!(holder_pid_is_dead("not-a-pid"));
    }
}
