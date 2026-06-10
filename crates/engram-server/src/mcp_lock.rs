//! Exclusive flock lock for MCP stdio servers — one process per store path.
//!
//! Prevents duplicate `engram mcp --store` instances from contending on the same
//! large manifold (BVH rebuilds, CUDA context, fd pressure). This was a root cause
//! of transport failures and 30GB+ RAM spikes when Grok + harness both launched MCP.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub struct McpStoreLock {
    _file: File,
}

impl McpStoreLock {
    /// Acquire an exclusive non-blocking lock for `store_path`.
    /// Fails fast if another engram MCP process already holds the lock.
    pub fn acquire(store_path: &str) -> anyhow::Result<Self> {
        let expanded = shellexpand::tilde(store_path).into_owned();
        let lock_dir: PathBuf = shellexpand::tilde("~/.engram/locks").into_owned().into();
        std::fs::create_dir_all(&lock_dir)?;

        let hash = blake3::hash(expanded.as_bytes());
        let lock_path = lock_dir.join(format!("mcp-{}.lock", &hash.to_hex()[..16]));

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if ret != 0 {
                let other_pid = std::fs::read_to_string(&lock_path).unwrap_or_default();
                anyhow::bail!(
                    "Another engram MCP server is already running on store '{expanded}'.\n\
                     Lock: {}\n\
                     Holder PID (from lock file): {}\n\
                     Fix: restart your IDE/TUI (one MCP instance per store), or stop the other process.",
                    lock_path.display(),
                    other_pid.trim()
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
}