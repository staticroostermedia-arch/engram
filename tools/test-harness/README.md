# Engram MCP Test Harness

**Ground truth reference**: `target/debug/engram` (or `scripts/engram-grok` with `ENGRAM_PROFILE=agent`). Run suites with `STABLE_BIN=target/debug/engram`.

**Note:** `logs/` and `results/` are gitignored — harness writes locally only.

**Location**: `tools/test-harness/`

**Date created**: 2026-05-31 (immediately after stable adoption + diagnosis of the transport regression).

## Why This Harness Exists

On 2026-05-31 the MCP transport in agent/CLI contexts (separate from the user's Grok Build TUI client connection) would succeed on the lightest entry points (`watch_workspace`, `session_start`) then immediately return "Transport closed" for every subsequent geometric call required by the wake-up ritual:

- `verify_manifold_integrity`
- `summarize`
- `query_with_momentum`
- `search_by_relation`
- `recall*`
- `record_reasoning_trace`
- etc.

The stable server process itself (running under the TUI) was healthy and serving heavy OptiX + full manifold features. The regression was in transport *lifetime* under repeated / heavy load from long-lived stdio MCP clients (exactly the subagent-like pattern).

Contributing factors diagnosed in the living config:
- Duplicate `engram mcp --store` processes fighting over the same large stalk (resource contention on indexes + BVH + CUDA context).
- Fast-MCP placeholder path + heavy background init (OptiX GAS/pipeline for 150k+ primitives, ki_hijacker, full BVH) starving the stdio event loop.
- Wrapper/launcher logic that did not reliably prevent duplicates or select the known-good binary.

This harness makes the exact failure mode impossible to regress silently.

## Directory Layout

```
tools/test-harness/
├── README.md                 # This file
├── bin/
│   ├── engram-harness.sh     # Main orchestrator (all suites, side-by-side, repro, observers, recording)
│   └── live-observer.sh      # Standalone tail + ps/lsof monitor (usable on any engram)
├── python/
│   └── mcp_test_client.py    # Self-contained stdio JSON-RPC MCP client (handshake + tool calls + timing + death detection)
├── logs/                     # Per-run stderr captures + monitor output
├── results/                  # Machine-readable JSON per run (timings, alive/failure counts, full step traces)
└── tests/                    # (future) declarative suite definitions
```

## Core Scripts

### 1. `bin/engram-harness.sh` (primary entry point)

All the requirements in one script:

- Always launches in **isolated temp stores** (`/tmp/engram-harness-*-store`) — eliminates the duplicate-instance class of failure.
- Supports `--binary`, defaults to the stable ac3509a9 reference.
- `--suite`:
  - `health` — watch + session_start + summarize + verify + stats (the exact sequence that died on May 31).
  - `full-wakeup` — complete ritual including momentum/relation/continuation + spatial + wake-up lawfulness verification metric recording + assert (metric: blocks bound to codeland 1780091465) + compression measurement.
  - `transport-lifetime` — the killer: repeated heavy calls in a single long-lived client (subagent simulation).
  - `heavy-light` — buckets timings for light/medium/heavy tools.
  - `optix-stress` — env-controlled (ENGRAM_OPTIX_ENABLED) + larger verify samples.
  - `lawfulness-metric` — exercises + asserts Wake-up Lawfulness Verification Tracking (metric:wake_up_verification_* + trend update via update-preferred; genesis/spatial/ki freshness; lawful bool + score; auto-relates to handoff:codeland_integration_2026_plan + 1780091465 + May 31 investigation artifacts).
  - `compression-measurement` — Context Compression Tracking System v1 (dual-lens before/after proxies, COMPRESS: compression_tracking_v1 marker minting high-CRS compression_event_* artifacts with full schema (before snapshots, promoted hot tiles/traces/anchors/hydration cache, after, continuity metrics, scars), explicit binds to codeland 1780091465 + MCP transport regression harness results + trace:1779992449 pilot). Low-friction trigger simulation. Permanent regression gate for 65-70% windows.
  - `all`
  - Unified "Continuity & Coherence Metrics" surface (integrating both lawfulness + compression systems) exercised via `lawfulness-metric` + `compression-measurement` + `full-wakeup` suites (see living config "Continuity & Coherence Metrics" unified section + new helper:continuity_coherence_metrics_dashboard_v1 for synthesized schemas, queries, dashboard visibility/trending; all artifacts bind to codeland 1780091465 + recent investigation).
- `--side-by-side` — spawns stable + dev in parallel isolated stores, identical workload, automatic diff of alive/failure counts + timings.
- `--repro-pre-fix` — simulates the exact old wrapper logic (cargo-first preference order, **no** aggressive duplicate killing, no forced `.leg-http.pid` cleanup, different KI env handling). Use this to re-create the pre-fix state on demand.
- `--observe` — spawns background log tails (grep for MCP-FAST, Pipeline, LBVH, Transport, starvation, etc.) + ps/lsof/fd monitor on the store.
- `--record-results` — emits ready-to-paste `mcp_engram_remember` / `relate` commands (with proper goal_context) plus a ready-to-append snippet for the living `Engram_Build_Launch_Configuration.md`.
- `--pre-swap-validate` — **The one-command implementation of the Mandatory Pre-Binary-Swap Validation Protocol** (see the dedicated section in `Engram_Build_Launch_Configuration.md`). Forces strict side-by-side + high-iteration heavy suites (including momentum + lifetime + continuity metrics) + observe + record-results. This is the gate you must pass before any `cargo install` that could become your daily driver. Hard-fails on transport deaths or major regressions.
- Duplicate launch contention test is always exercised.
- Clean trap + PID management modeled on the proven `scripts/leg` launcher.

### 2. `python/mcp_test_client.py`

Pure-Python, no external deps beyond stdlib. Acts as a real MCP client:

- Spawns the target binary as `... mcp --store /tmp/iso...`
- Full initialize + notifications/initialized handshake.
- `tools/call` for any `mcp_engram_*` tool.
- Per-call timing, classification (light/medium/heavy), aggregate stats.
- Hard transport death detection: process poll + broken pipe + timeout = "Transport closed" equivalent. Exactly matches the symptom.
- Dedicated stderr capture thread (server logs never pollute protocol stdout).
- Suites implemented directly in the client so they are identical across stable/dev runs.

Run it standalone for ad-hoc debugging:
```bash
python3 tools/test-harness/python/mcp_test_client.py \
  --binary /path/to/.engram-ac3509a9/bin/engram \
  --store /tmp/harness-test-$$ \
  --suite transport-lifetime --iterations 15 --verbose
```

## Quick Start (Tested on the Stable Reference)

From repo root, inside a TUI session connected to the stable server (so you also have live MCP tools for recording results):

```bash
cd /path/to/Documents/Engram

# Minimal health gate (fast)
tools/test-harness/bin/engram-harness.sh --suite health

# The regression-critical one (lifetime under load)
tools/test-harness/bin/engram-harness.sh --suite transport-lifetime --iterations 25 --observe

# Compression tracking fidelity (65-70% windows + artifact production + codeland/MCP bind)
tools/test-harness/bin/engram-harness.sh --suite compression-measurement --iterations 3 --record-results

# Full ritual + recording (recommended after any dev change)
tools/test-harness/bin/engram-harness.sh --suite full-wakeup --iterations 8 --record-results

# Side-by-side after you rebuild a dev binary
cargo build -p engram-server
tools/test-harness/bin/engram-harness.sh --side-by-side --suite transport-lifetime --dev-binary target/debug/engram

# Repro the exact pre-fix wrapper conditions
tools/test-harness/bin/engram-harness.sh --repro-pre-fix --suite all --observe
```

All runs are completely isolated from `~/.engram/stalks/`. The stable binary is never mutated.

## How This Prevents Recurrence of the May 31 MCP Transport Regression

1. **Exact symptom reproduction** — The `transport-lifetime` and `full-wakeup` suites perform the light entry calls then immediately hammer with the heavy geometric tools (`verify_manifold_integrity` with non-trivial samples, `query_with_momentum`, relation searches, repeated summarize/stats under load) inside one persistent client connection. Any early death or "timeout / transport closed" is reported with full step trace and timing.

2. **Duplicate / contention class eliminated by design** — Every launch uses a unique temp store directory. The harness still *exercises* the duplicate scenario explicitly so the old failure mode stays visible in logs.

3. **Background init starvation path exercised** — OptiX/BVH-heavy launches (with `ENGRAM_OPTIX_ENABLED=1`) + large verify samples + repeated calls during/after "Pipeline ready" / "MCP-FAST full initialization" directly stress the fast-path placeholder + stdio loop that was the root architectural contributor.

4. **Repro on demand** — `--repro-pre-fix` gives you a one-command recreation of the old launcher selection + missing safety logic. You can prove a fix actually closes the window that the May 31 stable binary closed.

5. **Side-by-side + objective metrics** — Alive count + failure count + per-bucket latency diffs between stable (must be 0 failures) and any candidate dev binary. No "it feels better" subjectivity.

6. **Live observability** — `--observe` gives you the same diagnostic signals the subagent transport observer and live diagnostics used (tail of MCP-FAST messages, LBVH, ki_hijacker activity, fd pressure on the store).

7. **Closed feedback loop into the living system**:
   - `--record-results` produces `mcp_engram_*` calls that become first-class episodic + relational artifacts (with goal_context linking to codeland handoff + MCP work).
   - Emits a ready-to-append section for `docs/Engram_Build_Launch_Configuration.md`.
   - Results JSONs are themselves candidates for `mcp_engram_force_spatial_ingest` + `context_for_file`.

8. **Integration with engram-tui** — The harness never touches the production stable store. After any harness run that flags a regression you simply run `engram-tui` (which self-heals duplicates + relaunches the known-good binary) and continue.

Run this harness as a gate before swapping any dev binary into daily use, before merging changes that touch `mcp.rs`, `main.rs` (the fast path), `store.rs` (verify paths), `bvh.rs`, or the launcher scripts. Any future change that re-introduces transport death under realistic ritual load will fail the harness in the exact same way the May 31 incident manifested.

## Extending / Custom Suites

- Add new sequences in `python/mcp_test_client.py` (the `WAKEUP_SEQUENCE`, `FULL_RITUAL_SEQUENCE`, and the `run_*` methods). The `record_and_assert_wake_up_verification_metric` + lawfulness-metric suite now exercises the first-class `metric:wake_up_verification_*` + `metric:wake_up_lawfulness_trend` (update-preferred) per engram-wake-up Phase 1.5. Compression + unified Continuity & Coherence Metrics (both systems + helper:continuity_coherence_metrics_dashboard_v1) exercised in compression-measurement + full-wakeup (binds to codeland 1780091465 + living config unified section).
- Add new `--suite` branches in `bin/engram-harness.sh`.
- The client already classifies tools and records every step with timings — new suites automatically benefit from all the death detection and observer plumbing.

## Ritual Compliance for Future Work on the Harness

Any edit to these scripts or the client must follow the established Code Edit Ritual:
- Pre-edit: `mcp_engram_context_for_file` (or `recall_in_file`) on the exact paths being changed + the living config doc.
- Structured `record_reasoning_trace` with `goal_context` (codeland or MCP integration arc).
- Post-edit spatial impact + `mcp_engram_force_spatial_ingest` on the harness dir.
- Update this README + the living config only via the harness's own `--record-results` path where possible.

This harness itself was created under that discipline (multiple pre-edit recon calls on the target files, the Engram_Build_Launch_Configuration.md, mcp.rs, main.rs, the stable launcher, and the subagent diagnostic notes in the manifold).

## References (Living Sources)

- `docs/Engram_Build_Launch_Configuration.md` (the single source of truth for binary state, the ac3509a9 adoption, the transport incident, and the engram-tui wrapper).
- `~/.engram-ac3509a9/bin/engram` + `~/bin/engram-tui` (the fixed production path).
- `crates/engram-server/src/mcp.rs` and `main.rs` (the fast-path + tool implementations exercised by every suite).
- `scripts/leg` (proven patterns for robust bg launch, PID files, traps, binary resolution — heavily reused here).
- Prior subagent transport observer / live diagnostic work (recorded in the manifold as traces/tiles under the relevant conv:arc and codeland goals).

---

**Success criterion**: Running the full transport-lifetime suite against the stable reference always reports `still_alive: true`, `transport_failures: 0`, and clean heavy-call timings. Any deviation on a dev binary is a hard stop.

This is how we keep the geometric memory substrate reliable for every future agent instance.