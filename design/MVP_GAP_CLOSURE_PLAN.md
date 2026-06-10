# MVP Gap Closure Plan — Local Agent Memory

**Status:** Executing (2026-06-06)  
**Primary goal:** `goal:mvp_gap_closure_v1`  
**Branch:** `feat/mvp-github-prep-2026-06`  
**North star:** One geometric primitive + 8-tool agent contract = Grok Build default memory.

---

## Thesis (unchanged)

Flat weights / flat files / chunk-vector RAG lack the right **unit of memory**. The HolographicBlock primitive (256KB, q/p, CRS, Merkle, VSA) is correct. MVP work closes the **agent contract** and **productization** gaps — not the physics paper.

---

## Tiers & acceptance

### Tier 0 — One product (`ENGRAM_PROFILE`)

| ID | Deliverable | Acceptance |
|----|-------------|------------|
| T0.1 | `crates/engram-server/src/profile.rs` | `agent\|deep\|ui\|dev` + `apply()` |
| T0.2 | `main.rs` uses `Profile::apply()` not env soup | MCP boot logs `[PROFILE] agent` |
| T0.3 | `scripts/engram-grok` sets `ENGRAM_PROFILE=agent` | No ac3509a9 fallback |
| T0.4 | `integrations/grok-build/mcp.json` minimal | Profile only, no 10-var block |
| T0.5 | `backend_readiness.profile` field | JSON includes `"profile":"agent"` |

**Done when:** Zero env block in MCP config; readiness shows profile.

### Tier 1 — Continuity loop (8-tool contract)

| ID | Deliverable | Acceptance |
|----|-------------|------------|
| T1.1 | `session_end` → `helper:session_handoff_latest` | Structured JSON packet |
| T1.2 | `session_start` inline `continuation_bundle` | goal + handoff + artifacts |
| T1.3 | `recall(scope=anchors)` tier | goals/traces before episodic |
| T1.4 | Harness `agent-memory` suite | 7/7 steps, wake <5s, transport alive |
| T1.5 | Handoff packet includes `profile` + `memory_mode` | Machine-readable rehydration |

**Done when:** `engram-harness.sh --suite agent-memory` passes on `target/debug/engram`.

### Tier 2 — Edit-scoped memory (next sprint)

| ID | Deliverable | Acceptance |
|----|-------------|------------|
| T2.1 | `context_for_edit` on real repo paths | AABB + anchor hits |
| T2.2 | Post-edit `quick_trace` with spatial_context | Documented in skill |

### Tier 3 — Scale retrieval (in progress)

| ID | Deliverable | Acceptance |
|----|-------------|------------|
| T3.1 | `scope=anchors` no overview backfill | Pure anchor pool only |
| T3.2 | `ENGRAM_LEAN_ANCHOR_POOL` (default 800) | Agent profile sets it |
| T3.3 | `warm_wake_anchors()` at `session_start` | Hot RAM before bundle |
| T3.4 | `fetch_block_high_priority` in recall scoring | mmap before O_DIRECT |
| T3.5 | `~/.grok/config.toml` → `engram-grok` | Two env vars only |

Target: `recall(anchors)` p50 &lt;500ms on 181k (was ~2.3s).

### Tier 4 — Trust (deferred)

WAL sidecar protocol, full H¹ audit — post-MVP.

### Explicitly parked

432Hz integrator, Golden Goose fine-tune, OptiX default in agent profile, full geometric time register.

---

## Execution log

| Time | Milestone |
|------|-----------|
| 2026-06-06 | Plan created; `goal:mvp_gap_closure_v1` minted |
| 2026-06-06 | Sub-agent: `agent-memory` harness suite added + passed |
| 2026-06-06 | Sub-agent: handoff/readiness `profile` field |
| 2026-06-06 | `profile.rs` + launcher consolidation (orchestrator) |
| 2026-06-06 | MCP param aliases (`goal_id`, `from`/`to`/`relation`); context_for_edit Tier 2 fields |
| 2026-06-06 | Harness: profile assertion + context_for_edit step |

---

## Run validation

```bash
cargo build -p engram-server
target/debug/engram --version

STABLE_BIN=/path/to/Engram/target/debug/engram \
  tools/test-harness/bin/engram-harness.sh --suite agent-memory
```

---

## Agent contract (reference)

```
WAKE  → session_start(intent)
WORK  → context_for_edit(path) | recall(scope=anchors) | quick_trace | remember
END   → session_end(summary)
PROBE → get_backend_readiness
MODE  → set_memory_mode("deep")  # only when needed
```

See `docs/AGENT_MEMORY_CONTRACT.md`.