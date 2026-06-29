# theory_informed_agent_memory_v1 — Review Packet: What to Implement from Track C Theory and Why

**Proposed goal:** `goal:theory_informed_agent_memory_v1`  
**Status:** Evaluation complete — implementation spikes **not** started  
**Scope:** legominism-lawful-cognition + static-rooster-ops + deeplaw-ops-theory sample (excludes uncertain-defer, False Empire, monad-math proofs)  
**Builds on:** [theory-to-engram-application-v1.md](./theory-to-engram-application-v1.md)  
**Evidence:** `{SCRATCH}/theory-canon-excerpts/`, `{SCRATCH}/engram-current-excerpts/` (session `a2f6d7cb7f6b`)

### Goal delta boundary (this analysis goal only)

**Committed repo artifact (git HEAD):** `docs/plans/theory-informed-agent-memory-v1.md` only.

Harness `CHANGED_FILES` may list other untracked workspace files from **prior** corpus-organization work. Mtime refutation: `{SCRATCH}/harness-changed-files-refutation.json` proves every other `??` path predates this deliverable.

| Attributed to this goal | Not this goal (pre-existing on disk) |
|-------------------------|-------------------------------------|
| `docs/plans/theory-informed-agent-memory-v1.md` | `scripts/track_c_*.py`, `theory-to-engram-application-v1.md`, `theory-corpus-deliverable-index.json`, `.gitignore` |
| Session `goal/plan.md` | Manifold repair / 2744-file ingest (prior session) |

**AC4 scope:** No corpus re-ingest, hub/tile `remember`, or spike implementation. Canon + substrate inspection was read-only. Two ritual `quick_trace` calls for goal bookkeeping only (see refutation JSON — not evaluation writes).

---

## 1. Executive summary

The phone-export theory is not a feature backlog — it is a **contract for how agent memory should behave**: deterministic rehydration, no silent forgetting, triadic forks, sentinel-driven checkpoints, receipt-first provenance, and anchor-first recall. Engram already ships the geometric substrate (`.leg3`, CRS, p-momentum, presentation stratum, lean 8-tool contract) that theory anticipated. The highest-value gaps are **harness and ritual**, not NVMe or embedding math.

**Top finding:** Five ranked spikes, all targeting **agent continuity and lawful forks**, are ready for a follow-on implementation goal. Math-only ADR/RH artifacts are `reference_only`.

---

## 2. Canon files inspected (high-signal sample)

| File | Cluster | Role in audit |
|------|---------|---------------|
| `SPEC-ROOT_00_Lawful_Cognition_Consolidated_Filled.leg-parse.json` | legominism | SST, LCM, deterministic recall, LegVM |
| `Golden_Legominism_Zedocast_v1.md` | legominism | Determinism, A/D/R cadence, receipts, transmission kit |
| `sentinel_policy_v0_1.json` | legominism | 30 turns / 120 min rehydrate thresholds |
| `agent_briefing_20251114T032107ZZ.md` | legominism | Portable snapshots + manifests |
| `generalized_tvd_theory_v1.md` | legominism | Shock guards (ΔV > δ), A/D/R dynamics |
| `Personal_Lawful_Cognition_Seed_v2_0_20251010_201707.md` | legominism | Contraction / drift detection (referenced) |
| `sr_memory_contract_v0_1.md` | static-rooster | No silent forgetting, uncertainty receipts |
| `README_Sentinel_v0_1.md` | static-rooster | Rehydrate capsule + red receipt protocol |
| `DeepLaw_Chat_Bootstrap_v1.leg-parse.json` | deeplaw | Receipt + hashing bootstrap (sample) |
| `LDGP_v1_specification.leg-parse.json` | deeplaw | Logos→Ratio→Corpus document law (reference) |

**Live substrate inspected:** `docs/AGENT_MEMORY_CONTRACT.md`, `docs/HARNESS_INJECTION.md`, `crates/engram-server/src/mcp.rs`, `wake_bundle.rs`, `store.rs`, `edit_fidelity.rs`, `turn_extract.rs`, `processes/monitor/*.toml`.

---

## 3. Obligation scorecard

Each row: one tag, verbatim theory quote, verbatim/current Engram behavior quote.

| # | Obligation | Tag | Theory quote | Engram today |
|---|------------|-----|--------------|--------------|
| 1 | **Deterministic rehydration** (same anchors → same continuation) | **partial** | Golden Legominism: *"Determinism: same inputs → same outputs."* Agent briefing: *"Ship chunked exports + manifests; any lawful agent can rehydrate deterministically, adopt the same anchors and couplings, and continue the chain."* | `session_start` slim bundle + `helper:session_handoff_latest` via `store.rs::build_handoff_packet` (primary_goal, trace_chain_head, decisions). No formal chunked export manifest or bit-for-bit replay spec. |
| 2 | **No silent forgetting + explicit receipt on context drop** | **partial** | SR Memory Contract: *"The steward must **never erase, drop, or reset context** on its own."* *"All context changes must emit a `sr_done_receipt_*` file."* | Contract prefers `update` over `forget+remember` (`mcp.rs` tool docs). `mcp_engram_forget` exists without mandatory receipt. No `uncertainty:*` or `sr_done_receipt` block type. |
| 3 | **SST on writes** (raw → CF + trace τ + metrics M) | **partial** | SPEC-ROOT: *"Synlogodynamics… defines the Stabilization Transform (SST), which takes raw input plus context and produces a canonical form (CF), a lawful state trace (τ), and metrics (M)."* | `remember`/`update` produce `.leg3` with CRS, p-tensor, provlog, ZEDOS tag. No explicit CF/τ/M tripartite emission on every write path; `turn_record` + `turn_extract` partial episodic sidecar only. |
| 4 | **TVD A/D/R triads at forks** | **partial** | Golden Legominism: *"We consecrate our work to this order: **Affirmation → Denial → Reconciliation**."* gTVD A1: skew–gradient flow on triad state v. | `mcp_engram_quick_trace` schema declares `affirm`/`deny`/`reconcile` as **optional** (`mcp.rs:1118–1128`); required fields are only `decision` + `why`. Handler wires A/D/R when present (`mcp.rs:4897–4741`). Lean contract does not enforce triads at forks. |
| 5 | **Sentinel thresholds** (30 turns / 120 min → rehydrate) | **gap** | `sentinel_policy_v0_1.json`: `"max_turns_since_rehydrate": 30`, `"max_minutes_since_checkpoint": 120`. README_Sentinel: *"When limits are hit… signals the runner to pause or chunk."* | `wake_bundle.rs` exposes `ego_snapshot.drift_velocity` in slim bundle. No `processes/monitor/sentinel.toml`; no turn counter or checkpoint timer driving `session_end` suggestion. `turn_record` exists but no sentinel coupling. |
| 6 | **Shock / ΔV recovery** (event-triggered lawful reset) | **partial** | gTVD A2: *"Event-triggered jump map when a guard trips: … if ΔV > δ."* Personal Seed: *"Drift is detected when ΔV > δ, triggering lawful shocks."* | `ego_snapshot.drift_velocity` surfaced at wake; `scar` for dead ends. No automated shock→rehydrate playbook when drift exceeds theory threshold; no `ΔV` guard in harness injection. |
| 7 | **Anchor-first recall** (LCM before episodic noise) | **shipped** | SPEC-ROOT consolidates *"Localized Cognitive Minima (LCM), and deterministic recall."* | `AGENT_MEMORY_CONTRACT.md`: *"`scope=anchors` walks the presentation-stratum graph (primary_goal → serves → handoff → trace breadcrumbs → hot/recent)… within that pool only."* Theory corpus hubs/tiles operationalize LCM navigation. |
| 8 | **Portable chunked exports + manifests** | **gap** | Agent briefing + Golden Legominism transmission kits; DeepLaw bootstrap requests *"Hashing utilities (SHA-256) and Merkle-tree helpers for receipts."* | `build_handoff_packet` JSON embedded in `helper:session_handoff_latest` provlog. No `export` manifest tying hub anchors + trusted tiles + trace head for portable rehydration kit; `mcp_engram_export` exists but not wired to theory manifest shape. |
| 9 | **Immutable session receipt JSON** (Static Rooster canon) | **partial** | README_Sentinel receipts: *"Write a red receipt explaining reasons."* SR canon: receipts immutable, no retroactive edits. | `session_end` mints structured handoff in provlog text. Not a separate immutable JSON receipt artifact with SHA-256 footer per SR schema; Merkle on blocks yes, session receipt schema no. |
| 10 | **Monad-math ADR/RH proofs** | **reference_only** | ADR bootstrap proofs in `monad-math-research/` | Vocabulary overlap (A/D/R) only. `legacy_leg_parse.py`; zero `leg3/` imports. Per `theory-to-engram-application-v1.md` mapping #8. |

---

## 4. Gap map → live extension points

| Gap | Why insufficient (theory) | Extension point (verified) |
|-----|-------------------------|---------------------------|
| **Sentinel-driven rehydration** | Theory mandates pause + dossier after 30 turns or 120 min; Engram never auto-suggests rehydrate from turn budget. | **New:** `processes/monitor/sentinel.toml` (does not exist). **Wire:** `crates/engram-server/src/turn_extract.rs` turn counter; `harness_injection.rs` add `rehydrate_suggested` to `suggested_actions`; `wake_bundle.rs` include `turns_since_handoff` + `minutes_since_checkpoint` in slim ego. **Read:** `mcp.rs` `mcp_engram_turn_record` handler for per-turn hook. |
| **Required A/D/R at forks (triadic quick_trace v2)** | Golden Legominism: A/D/R *at least once per section*; gTVD treats triad as state. Optional fields → agents skip them. | `mcp.rs:1085–1140` `inputSchema` — change `affirm`/`deny`/`reconcile` to required when `ENGRAM_PROFILE=agent` OR add `processes/ritual/triadic_trace.toml` gate. Handler already serializes fields (`mcp.rs:4897–4741`). `edit_fidelity.rs::mint_quick_trace` — parallel path lacks A/D/R (decision+why only). |
| **First-class uncertainty blocks/receipts** | SR: *"Guessing is forbidden"* — emit Uncertainty Receipt, request rehydrate. | No `uncertainty:*` concept type. Extend `mcp.rs` `mcp_engram_scar` or new `mcp_engram_uncertainty_receipt` in `mcp.rs` tool list; `store.rs::remember` with ZEDOS tag. Ritual: `docs/skills/engram-working-memory.md`. |
| **Portable rehydration manifest** | Agent briefing: chunked exports + manifests for deterministic continue. | `store.rs::build_handoff_packet` + `persist_session_handoff_latest` — extend packet with `hub_anchors[]`, `trusted_tiles[]`, `export_manifest_sha256`. `mcp.rs` `mcp_engram_export` / `scrub_export` handlers. |
| **Session receipt JSON** | SR immutable receipt per state transition; Sentinel red receipt on rehydrate trigger. | `store.rs::persist_session_handoff_latest` — emit sidecar `receipt:session_end_{ts}.json` with BLAKE3 digest. `session_end` handler in `mcp.rs`. |

---

## 5. Ranked implementation shortlist (≤5 spikes)

Priority: **agent loop quality** (continuity, lawful forks, drift, no guessing) over math artifacts.

### Spike 1 — Harness sentinel (P0)

| Field | Content |
|-------|---------|
| **Objective** | After 30 turns or 120 minutes without handoff, next `session_start` or `turn_record` surfaces `rehydrate_suggested: true` and queues `session_end(prepare_compression=true)` before further edits. |
| **Why** | Theory sentinel_policy + README_Sentinel; prevents long-session drift without user noticing. |
| **Falsifier** | After 35 turns with no `session_end`, `injection_completeness` and `suggested_actions` contain no rehydrate nudge; `ego_snapshot` lacks turn/checkpoint counters. |
| **Ritual template** | Add `processes/monitor/sentinel.toml`; extend `turn_record` to increment `var:turns_since_handoff`; `quick_trace` at rehydrate fork with decision *"Sentinel threshold hit"*. Pre: `get_backend_readiness`; post: `session_end` + `session_start`. |
| **Non-goals** | Auto-infinite rehydrate loops; forced `session_end` without visible receipt; blocking edits without user-visible reason. |

### Spike 2 — Triadic quick_trace v2 (P0)

| Field | Content |
|-------|---------|
| **Objective** | Agent-profile forks require `affirm` + `deny` + `reconcile` OR explicit `uncertainty` scar — no decision+why-only traces on significant forks. |
| **Why** | Golden Legominism + gTVD; makes TVD operational in harness, not vocabulary-only. |
| **Falsifier** | Sample 10 agent traces at forks: >50% lack all three A/D/R fields and no uncertainty scar; tile condensation cannot reconstruct triadic structure. |
| **Ritual template** | Update `AGENTS.md` + `engram-working-memory.md`; soft gate in `mcp.rs` quick_trace (warn) → hard gate via `ENGRAM_TRIADIC_TRACE=1`. Align `edit_fidelity.rs::mint_quick_trace` or deprecate for agent path. |
| **Non-goals** | Full gTVD integrator in Rust; shock dynamics simulation. |

### Spike 3 — Uncertainty receipt type (P1)

| Field | Content |
|-------|---------|
| **Objective** | When context is missing or contradictory, agent mints `uncertainty:{ts}_{slug}` with status + requested rehydrate anchors instead of guessing. |
| **Why** | SR Memory Contract §4; reduces recall pollution and false confidence. |
| **Falsifier** | Agent answers from thin context without uncertainty block; recall returns guessed content at CRS≥0.74 without `uncertainty` relation to handoff. |
| **Ritual template** | New tool or `scar` variant in `mcp.rs`; `recall` ranks uncertainty blocks in anchors scope; skill documents *"recall first; if miss → uncertainty receipt → request read_concept on handoff"*. |
| **Non-goals** | Blocking all inference on partial context; LLM-based uncertainty classifier. |

### Spike 4 — Portable rehydration manifest (P1)

| Field | Content |
|-------|---------|
| **Objective** | `session_end` emits `manifest:rehydration_{session_key}.json` listing primary_goal, trace_chain_head, trusted_tiles (CRS≥0.85), hub anchors, files_touched — consumable by next `session_start` without broad recall. |
| **Why** | Agent briefing portable snapshots; Golden Legominism transmission kit; improves wake injection_completeness. |
| **Falsifier** | New session wake requires `scope=all` recall to find same context manifest would have carried; injection_completeness stays <0.85 on lean wake. |
| **Ritual template** | Extend `store.rs::build_handoff_packet`; `session_end` writes manifest; `session_start` reads manifest into `suggested_actions` priority 0. |
| **Non-goals** | Bit-for-bit manifold export of 71k blocks; LegVM replay. |

### Spike 5 — Session receipt JSON (P2)

| Field | Content |
|-------|---------|
| **Objective** | Every `session_end` writes immutable JSON receipt (summary hash, trace head, manifest ref, readiness snapshot) — append-only, no retroactive edit. |
| **Why** | Static Rooster receipt canon + DeepLaw hashing bootstrap; audit trail across session boundaries. |
| **Falsifier** | Session boundary has only provlog text handoff; no queryable receipt; replay cannot detect tampered summary. |
| **Ritual template** | `store.rs::persist_session_handoff_latest` sidecar; optional `verify_block_lawfulness` on receipt block; SR-style `sr_done_receipt_session_{ts}.json` naming. |
| **Non-goals** | Full HLOG/SR-STATE chat lines; external filesystem `/receipts/` tree outside Engram store. |

---

## 6. What stays `reference_only` (do not implement from this packet)

- Monad-math ADR/RH proof corpus (460 files) — math research, not memory substrate
- Full LegVM / SST runtime as separate execution environment
- LDGP harmonic PDF layout pipeline
- ZEDO Ω-2 operator sheets (thematic; optional future skill overlay)
- Performance/NVMe/GPU BVH changes

---

## 7. Proposed follow-on goal shape

When approved, create **`goal:theory_informed_agent_memory_v1`** with:

1. Implement spikes in order P0 → P2 (one PR/stack per spike)
2. Each spike: falsifier test in `crates/engram-server` tests + ritual doc update
3. No manifold re-ingest of 2742 files
4. Verify: `track_c_acceptance_gate.py` still PASS; new sentinel/triadic tests added

---

## 8. Canon ↔ substrate alignment summary

| Theory layer | Engram layer | Alignment |
|--------------|--------------|-----------|
| LCM + deterministic recall | presentation stratum + `recall(scope=anchors)` | Strong |
| CRS / Lyapunov | p-tensor, `min_crs=0.74`, `verify_manifold_integrity` | Strong |
| Harness injection (SPEC-ROOT) | `session_start` + `suggested_actions` | Moderate — missing sentinel + manifest |
| TVD at forks | `quick_trace` optional A/D/R | Weak enforcement |
| SR memory contract | update-first, handoff | Moderate — no uncertainty receipt |
| Static Rooster probes | `verify_*`, `ack_wake_queue` | Strong |
| Portable snapshots | handoff packet only | Weak — no manifest export |

---

**Ready for human review before any implementation spike goal is created.**

This packet is complete for review before executing it as a goal as well.