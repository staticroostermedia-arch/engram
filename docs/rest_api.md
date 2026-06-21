# Engram REST API Reference

The Engram REST server exposes a full HTTP JSON API as an alternative to MCP. This is the recommended integration path for Python agents, LangChain pipelines, AutoGen agents, or any tool that can make HTTP requests.

## Starting the Server

```bash
# Default port 3456
engram serve

# Custom port
engram serve --port 8888

# With a specific manifold store
engram serve --port 3456 --store ~/.engram/manifold
```

The server binds to `127.0.0.1` (localhost only) by default. CORS is enabled permissively, so browser-based agents can also connect.

## Authentication (Optional)

If you set the `ENGRAM_API_KEY` environment variable, all endpoints require a Bearer token:

```bash
export ENGRAM_API_KEY="your-secret-key"
engram serve
```

Include the token in all requests:
```
Authorization: Bearer your-secret-key
```

If `ENGRAM_API_KEY` is not set, the server runs open with no authentication (suitable for local development).

## PII Scrubbing

All text passed to `/api/remember` is automatically scrubbed of:
- **SSNs** (`123-45-6789` → `[REDACTED_SSN]`)
- **Credit card numbers** (`[REDACTED_CC]`)
- **Email addresses** (`[REDACTED_EMAIL]`)

This happens server-side before any data is written to disk.

---

## Endpoints

### `GET /health`

Returns server status and version. Use this to verify the server is running.

**curl:**
```bash
curl http://localhost:3456/health
```

**Response:**
```json
{
  "status": "ok",
  "version": "0.7.0-beta.3"
}
```

**Python:**
```python
import requests
r = requests.get("http://localhost:3456/health")
print(r.json())  # {'status': 'ok', 'version': '0.7.0-beta.3'}
```

---

### `POST /api/remember`

Encode text and store it as a persistent HolographicBlock under a concept name.

**Request body:**
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `concept` | string | ✅ | Unique snake_case identifier (e.g. `"auth_bug_fix"`) |
| `text` | string | ✅ | The content to encode and store |

**curl:**
```bash
curl -X POST http://localhost:3456/api/remember \
  -H "Content-Type: application/json" \
  -d '{"concept": "auth_bug_fix", "text": "Fixed JWT expiry by adding 5 minute clock skew tolerance in middleware."}'
```

**Response:**
```json
{"status": "success", "message": "Stored 'auth_bug_fix'"}
```

**Python:**
```python
import requests

r = requests.post("http://localhost:3456/api/remember", json={
    "concept": "auth_bug_fix",
    "text": "Fixed JWT expiry by adding 5 minute clock skew tolerance in middleware."
})
print(r.json())  # {'status': 'success', 'message': "Stored 'auth_bug_fix'"}
```

---

### `POST /api/recall`

Semantic search. Returns the top-k memories most similar to the query.

**Request body:**
| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `query` | string | ✅ | — | Natural language query |
| `k` | int | ❌ | `5` | Number of results (1–20) |
| `explain` | bool | ❌ | `false` | Include a human-readable explanation of the score |

**curl:**
```bash
curl -X POST http://localhost:3456/api/recall \
  -H "Content-Type: application/json" \
  -d '{"query": "authentication bugs", "k": 3}'
```

**Response:**
```json
[
  {
    "concept": "auth_bug_fix",
    "score": 0.91,
    "crs": 1.0,
    "text": "Fixed JWT expiry by adding 5 minute clock skew tolerance in middleware."
  }
]
```

**Response fields:**
| Field | Type | Description |
|-------|------|-------------|
| `concept` | string | The concept name |
| `score` | float | Semantic similarity (0–1). >0.80 = strong match |
| `crs` | float | Coherence-Reliability Score. ≥0.74 = grounded fact. 1.0 = pinned/immortal |
| `text` | string | The stored text content |
| `explain` | string? | Only present if `explain: true` was requested |

**Python:**
```python
import requests

results = requests.post("http://localhost:3456/api/recall", json={
    "query": "authentication bugs",
    "k": 5,
    "explain": True
}).json()

for r in results:
    print(f"[{r['score']:.2f} / CRS {r['crs']:.2f}] {r['concept']}: {r['text'][:80]}")
```

---

### `POST /api/forget`

Permanently delete a memory block from the manifold.

> **Warning:** This destroys the block's entire history. Use `remember` to overwrite instead if you want to update content.

**Request body:**
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `concept` | string | ✅ | The concept name to delete |

**curl:**
```bash
curl -X POST http://localhost:3456/api/forget \
  -H "Content-Type: application/json" \
  -d '{"concept": "auth_bug_fix"}'
```

**Response:**
```json
{"status": "success", "message": "Deleted 'auth_bug_fix'"}
```

**Python:**
```python
import requests

r = requests.post("http://localhost:3456/api/forget", json={"concept": "auth_bug_fix"})
print(r.json())
```

---

### `POST /api/relate`

Create a directional knowledge graph edge between two concepts.

**Request body:**
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `concept_a` | string | ✅ | Source concept |
| `concept_b` | string | ✅ | Target concept |
| `label` | string | ✅ | Relation type (e.g. `"depends_on"`, `"implements"`, `"contradicts"`) |

**curl:**
```bash
curl -X POST http://localhost:3456/api/relate \
  -H "Content-Type: application/json" \
  -d '{"concept_a": "auth_bug_fix", "concept_b": "jwt_middleware", "label": "fixes"}'
```

**Response:**
```json
{"status": "success", "message": "Related auth_bug_fix --[fixes]--> jwt_middleware"}
```

**Python:**
```python
import requests

r = requests.post("http://localhost:3456/api/relate", json={
    "concept_a": "auth_bug_fix",
    "concept_b": "jwt_middleware",
    "label": "fixes"
})
print(r.json())
```

---

### `POST /api/trace`

VSA geometry query. Computes the result of a vector operation (`ADD` or `BIND`) on two concept vectors and returns the nearest memories to the result.

- **`ADD`** (superposition): finds memories in the *union* of two concepts' semantic space
- **`BIND`** (association): finds memories that encode the *relationship* between two concepts

**Request body:**
| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `term_a` | string | ✅ | — | First concept name or raw text |
| `term_b` | string | ✅ | — | Second concept name or raw text |
| `op` | string | ❌ | `"ADD"` | `"ADD"` or `"BIND"` |
| `k` | int | ❌ | `5` | Number of results (1–20) |

**curl:**
```bash
# What memories live in the semantic space of "auth" AND "performance"?
curl -X POST http://localhost:3456/api/trace \
  -H "Content-Type: application/json" \
  -d '{"term_a": "authentication", "term_b": "performance", "op": "ADD", "k": 5}'
```

**Python:**
```python
import requests

results = requests.post("http://localhost:3456/api/trace", json={
    "term_a": "authentication",
    "term_b": "performance",
    "op": "ADD",
    "k": 5
}).json()

for r in results:
    print(f"[{r['score']:.2f}] {r['concept']}")
```

---

### `GET /api/list`

Returns all stored concept names. No pagination — returns the full list.

**curl:**
```bash
curl http://localhost:3456/api/list
```

**Response:**
```json
["auth_bug_fix", "jwt_middleware", "session_2026_04_22"]
```

**Python:**
```python
import requests
concepts = requests.get("http://localhost:3456/api/list").json()
print(f"{len(concepts)} concepts stored")
```

---

### `GET /api/recent?n=10`

Returns the N most recently accessed concept names and timestamps. Reads from an in-memory index — zero disk I/O.

**Query parameters:**
| Param | Type | Default | Max | Description |
|-------|------|---------|-----|-------------|
| `n` | int | `10` | `100` | Number of recent concepts to return |

**curl:**
```bash
curl "http://localhost:3456/api/recent?n=5"
```

**Response:**
```json
[
  {"concept": "auth_bug_fix", "last_accessed": 1745444716, "ago": "2m ago"},
  {"concept": "jwt_middleware", "last_accessed": 1745444600, "ago": "4m ago"}
]
```

**Python:**
```python
import requests
recent = requests.get("http://localhost:3456/api/recent", params={"n": 10}).json()
for entry in recent:
    print(f"{entry['concept']} — {entry['ago']}")
```

---

### `GET /api/code-atlas`

Returns code atlas v2.1 for a file window — same payload agents get from `mcp_engram_context_for_edit`, optionally with evolution bundle for LEG timeline.

**Query parameters:**
| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | *required* | Absolute or workspace-relative file path |
| `line_start` | int | — | Start line (1-based) for AABB filter |
| `line_end` | int | — | End line (1-based) for AABB filter |
| `evolution` | `1` | off | Include `evolution` object: loci, arc segments, trace chain |
| `preview_chars` | int | `200` | Max chars per arc segment preview |
| `trace_depth` | int | `6` | Max `prev_in_trace` hops from traces head |

**curl:**
```bash
curl "http://localhost:3456/api/code-atlas?path=crates/engram-server/src/store.rs&line_start=5100&line_end=5200&evolution=1"
```

**Response (abbreviated):**
```json
{
  "atlas_version": "v2.1",
  "spatial_items": [],
  "traces_at_locus": [],
  "traces_at_locus_tiers": { "exact": [], "file": [], "stem": [] },
  "edit_arc_debt": { "pending": 0 },
  "evolution": {
    "loci": [],
    "arcs": [],
    "trace_chain": []
  }
}
```

---

### `GET /api/context-window`

Wake harness bundle for LEG cockpit — `suggested_actions`, `ego_snapshot`, `continuity_playbook`, `presentation_stratum`. Used by `./scripts/leg --live`.

```bash
curl http://localhost:3456/api/context-window
```

---

### `GET /api/consciousness-surface`

Distilled geosphere / presentation nodes with lineage edges for LEG geosphere view.

```bash
curl http://localhost:3456/api/consciousness-surface
```

---

## Error Responses

All endpoints return a consistent error shape on failure:

```json
{"status": "error", "message": "concept and text are required"}
```

| HTTP Status | Meaning |
|-------------|---------|
| `200 OK` | Success |
| `400 Bad Request` | Missing or invalid fields |
| `401 Unauthorized` | Invalid or missing `ENGRAM_API_KEY` bearer token |
| `500 Internal Server Error` | Store write/read failure |

---

## CRS Score Guide

The `crs` field returned by `/api/recall` tells you how reliable a memory is:

| CRS | Meaning |
|-----|---------|
| `1.0` | Pinned / immortal — foundational knowledge, never evicted |
| `≥ 0.74` | Grounded fact — safe to act on |
| `0.50–0.74` | Working hypothesis — use with caution |
| `< 0.50` | Uncertain — verify before acting |

Low-CRS memories are automatically evicted by the Autophagy GC (run `engram forget-old` or call `/api/boot_agent` with the appropriate command).
