//! Local large-payload transport (Wave A2).
//!
//! On-box geometric payloads (atlas chunks, hot `.leg3` views) must not require
//! multi-MB JSON copies over MCP stdio. Prefer:
//! 1. **mmap** via [`engram_core::mmap::LegView`] — zero-copy process-local bytes
//! 2. **UDS path token** — Unix socket serves a small JSON descriptor
//!    `{path, offset, len, transport}` so peers open mmap themselves
//!
//! Docs: `docs/plans/local-primary-large-payload-ipc-v1.md`

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Schema version for path-token descriptors.
pub const LOCAL_IPC_V1: &str = "local_ipc_v1";

/// Build a small path-token JSON for a file region (no body bytes).
pub fn path_token(path: impl AsRef<Path>, offset: u64, len: u64) -> Value {
    json!({
        "version": LOCAL_IPC_V1,
        "transport": "mmap_path_token",
        "path": path.as_ref().display().to_string(),
        "offset": offset,
        "len": len,
        "note": "open path with mmap/LegView; do not JSON-encode block body",
    })
}

/// Open a 256KB `.leg` via mmap and return (token, first_n_bytes_preview).
/// Preview is capped so callers never serialize full blocks as JSON.
pub fn mmap_leg_preview(
    path: impl AsRef<Path>,
    preview_len: usize,
) -> std::io::Result<(Value, Vec<u8>)> {
    let path = path.as_ref();
    let view = engram_core::mmap::LegView::open(path)?;
    let bytes = view.as_bytes();
    let n = preview_len.min(bytes.len()).min(64);
    let preview = bytes[..n].to_vec();
    let token = path_token(path, 0, bytes.len() as u64);
    Ok((token, preview))
}

/// Serve one path-token response over a Unix domain socket, then exit.
/// Client connects, reads one JSON line, disconnects.
pub fn serve_path_token_once(
    sock_path: impl AsRef<Path>,
    token: &Value,
    accept_timeout: Duration,
) -> std::io::Result<()> {
    let sock_path = sock_path.as_ref();
    let _ = std::fs::remove_file(sock_path);
    if let Some(parent) = sock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let listener = UnixListener::bind(sock_path)?;
    listener.set_nonblocking(false)?;
    // Best-effort accept timeout via SO_RCVTIMEO on the listener fd is
    // platform-specific; tests use short client/server join.
    let _ = accept_timeout;
    let (mut stream, _) = listener.accept()?;
    let line = format!("{}\n", token);
    stream.write_all(line.as_bytes())?;
    stream.flush()?;
    let _ = std::fs::remove_file(sock_path);
    Ok(())
}

/// Client: connect to UDS, read one JSON line path token.
pub fn fetch_path_token(sock_path: impl AsRef<Path>) -> std::io::Result<Value> {
    let mut stream = UnixStream::connect(sock_path.as_ref())?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf)?;
    let line = buf.lines().next().unwrap_or("").trim();
    serde_json::from_str(line).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid path token JSON: {e}"),
        )
    })
}

/// Readiness / docs snippet: large-payload transport is available.
pub fn readiness_fields() -> Value {
    json!({
        "local_ipc_v1": true,
        "local_ipc_transports": ["mmap_leg_view", "uds_path_token"],
        "local_ipc_note": "prefer path tokens + LegView mmap over multi-MB JSON for on-box geometric payloads",
    })
}

/// Temp sock path helper for tests / one-shot servers.
pub fn temp_sock_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "engram_local_ipc_{label}_{}_{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_core::types::BLOCK_SIZE;
    use std::sync::mpsc;
    use std::thread;

    fn write_fake_leg(path: &Path) {
        let mut buf = vec![0u8; BLOCK_SIZE];
        // Distinct non-zero prefix so preview is non-empty.
        buf[0] = 0xE1;
        buf[1] = 0xE2;
        buf[2] = 0xE3;
        buf[3] = 0xE4;
        std::fs::write(path, &buf).expect("write fake leg");
    }

    #[test]
    fn mmap_leg_preview_returns_token_not_full_body() {
        let dir = std::env::temp_dir().join(format!("engram_mmap_preview_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("block.leg");
        write_fake_leg(&path);
        let (token, preview) = mmap_leg_preview(&path, 32).expect("mmap preview");
        assert_eq!(token["version"], LOCAL_IPC_V1);
        assert_eq!(token["transport"], "mmap_path_token");
        assert_eq!(token["len"], BLOCK_SIZE as u64);
        assert_eq!(preview.len(), 32.min(BLOCK_SIZE));
        assert_eq!(preview[0], 0xE1);
        // Critical: token serializes small — not 256KB JSON body.
        let token_bytes = serde_json::to_vec(&token).unwrap();
        assert!(
            token_bytes.len() < 4096,
            "path token must stay small, got {}",
            token_bytes.len()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uds_path_token_roundtrip() {
        let dir = std::env::temp_dir().join(format!("engram_uds_token_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let leg = dir.join("hot.leg");
        write_fake_leg(&leg);
        let token = path_token(&leg, 0, BLOCK_SIZE as u64);
        let sock = temp_sock_path("roundtrip");

        let sock_s = sock.clone();
        let token_s = token.clone();
        let (tx, rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let r = serve_path_token_once(&sock_s, &token_s, Duration::from_secs(5));
            let _ = tx.send(r);
        });
        // Brief settle so listener binds.
        thread::sleep(Duration::from_millis(50));
        let got = fetch_path_token(&sock).expect("client fetch");
        let server_r = rx.recv().expect("server result");
        server_r.expect("server ok");
        server.join().unwrap();
        assert_eq!(got["version"], LOCAL_IPC_V1);
        assert_eq!(got["path"], leg.display().to_string());
        assert_eq!(got["len"], BLOCK_SIZE as u64);
        let _ = std::fs::remove_file(&sock);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn readiness_fields_declare_transports() {
        let r = readiness_fields();
        assert_eq!(r["local_ipc_v1"], true);
        let t = r["local_ipc_transports"].as_array().unwrap();
        assert!(t.iter().any(|x| x.as_str() == Some("mmap_leg_view")));
        assert!(t.iter().any(|x| x.as_str() == Some("uds_path_token")));
    }
}
