# Local large-payload IPC (Wave A2)

**Goal:** `goal:engram_local_primary_critical_path_v1`  
**Module:** `crates/engram-server/src/local_ipc.rs`

## Problem

Multi-MB geometric views (atlas AABB dumps, full `.leg3` bodies, hot residency snapshots) must not cross MCP stdio as fat JSON.

## Transports

| Transport | Mechanism | Use |
|-----------|-----------|-----|
| **mmap_leg_view** | `engram_core::mmap::LegView` | Same process: zero-copy 256KB block bytes |
| **uds_path_token** | Unix socket, one JSON line | Peer process: receive `{path,offset,len}` and open mmap locally |

## Path-token schema (`local_ipc_v1`)

```json
{
  "version": "local_ipc_v1",
  "transport": "mmap_path_token",
  "path": "/home/a/.engram/stalks/….leg",
  "offset": 0,
  "len": 262144,
  "note": "open path with mmap/LegView; do not JSON-encode block body"
}
```

Token size is O(path length), never O(block size).

## Readiness

`get_backend_readiness` includes `local_ipc_v1`, `local_ipc_transports`.

## Tests

- `mmap_leg_preview_returns_token_not_full_body`
- `uds_path_token_roundtrip`
- `readiness_fields_declare_transports`
