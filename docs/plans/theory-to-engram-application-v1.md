# Theory-to-Engram Application Plan v1

**Scope:** Track C research theory only (`Engram/data/theory-corpus/organized/`).  
**Non-goals:** Implementing theory into code in this phase; ingesting False Empire or Laws of Coherence into Engram.

**Evidence base:** Pass 1 (5201 paths journaled), pass 2 organize + refinement (2744 organized files post round-3 cleanup), acceptance gate with live MCP raw captures 2026-06-29 (`scripts/track_c_acceptance_gate.py`).

---

## 1. Inventory — Track C clusters (post pass-2)

| Cluster | Files | Engram obligation |
|---------|------:|-------------------|
| `legominism-lawful-cognition` | 676 | **Primary** — harness, CRS, rituals ancestors |
| `monad-math-research` | 468 | Reference only — separate math domain |
| `static-rooster-ops` | 333 | Ops canon → AGENTS.md ritual alignment |
| `deeplaw-ops-theory` | 47 | Provlog / lawful filesystem precursors |
| `gurdjieff-fourth-way` | 6 | Optional thematic only |
| `reference-papers` | 4 | External literature |
| `_quarantine/*` | 848 | Junk, False Empire, Track B duplicates — no ingest |
| `uncertain-defer` | 369 | Needs human/agent refinement pass (13 False Empire moved to BookForge deferred) |

**Quarantined (not Engram):** False Empire → `BookForge/corpus/false-empire/` (115 files). Laws of Coherence → `BookForge/corpus/laws-of-coherence/` (68 files).

---

## 2. Element → Engram mappings (≥5)

| # | Theory element | Source cluster | Engram target | Evaluation criterion |
|---|----------------|----------------|---------------|----------------------|
| 1 | SPEC-ROOT / SPIRAL-LIGHT lawful stack | legominism | `session_start` continuation bundle, `suggested_actions` injection | Wake replay surfaces harness actions without blind tool replay |
| 2 | LEG v2 / triadic container (HEADER/BODY/FOOTER) | legominism | `.leg3` block lawfulness, `verify_block_lawfulness` | Block schema matches container spec; CRS ≥0.74 on pinned axioms |
| 3 | proof_phi_lyapunov / CRS control proofs | legominism | `p-tensor` momentum on `update`, `min_crs=0.74` gate | Drift tests show no annihilation on lawful update path |
| 4 | TVD / triadic vector dynamics | legominism | `quick_trace` ADR triads, harness fork routing | Trace chains carry A/D/R fields at decision forks |
| 5 | Fundamental Theorem of Deterministic Recall | legominism | BVH recall tiers, `recall(scope=anchors)` | Anchor recall returns goals/traces before episodic noise |
| 6 | DeepLaw CNCP / LegVM receipts | deeplaw | `provlog`, `session_end` compression, spatial ingest | Receipt chain tamper-evident across session boundary |
| 7 | Static Rooster probe/receipt contracts | static-rooster-ops | `ack_wake_queue`, `verify_manifold_integrity` cadence | Hard gate blocks `context_for_edit` until queue acked |
| 8 | Legacy `.leg` RH/ADR proofs | monad-math | **None (format cousin)** — `legacy_leg_parse.py` only | Zero files under `leg3/`; all sidecars `is_engram_leg3: false` |

---

## 3. Phased evaluation plan (not implementation)

### Phase 1 — Canon alignment audit (read-only)
- Sample 20 files per primary cluster; score vocabulary overlap with `AGENT_MEMORY_CONTRACT.md`, `GEOMETRIC_MEMORY.md`, `RITUALS.md`.
- Output: `scratch/theory-canon-alignment-scorecard.jsonl`.

### Phase 2 — Harness injection mapping
- Map SPEC-ROOT / SPIRAL-LIGHT injection fields to live `session_start` payload keys.
- Falsifier: if >30% of spec fields have no MCP tool equivalent, mark as `design_debt` tile.

### Phase 3 — Lawfulness gate replay
- Re-run `proof_phi_lyapunov` and CRS proof artifacts against `verify_manifold_integrity` + `verify_block_lawfulness` on sample blocks.
- Accept if sampled high-CRS blocks ≥0.74 and no p-momentum annihilation in update path tests.

### Phase 4 — Recall ergonomics
- Define 10 canonical queries (`legominism`, `ADR bootstrap`, `lawful cognition`, etc.).
- Require `read_concept` or `recall` CRS ≥0.74 on hub concepts after manifold ingest.

### Phase 5 — DeepLaw / provlog bridge
- Compare CNCP receipt schema in DeepLaw docs to Engram provlog block headers.
- Deliverable: diff report only — no schema merge without explicit user approval.

### Phase 6 — Uncertain-defer triage
- 382 remaining `uncertain-defer` files: second human pass or paired-file context.
- Target: <100 uncertain after triage; rest → quarantine or cluster promotion.

### Phase 7 — Application shortlist
- From phases 1–6, produce ranked list of ≤5 theory elements recommended for **future** implementation spikes.
- Each spike needs: falsifier, non-goals, and ritual trace template.

---

## 4. Manifold state (2026-06-29)

Verification: `python3 scripts/track_c_acceptance_gate.py` → exit 0; raw MCP in scratch `*.mcp-raw.txt`.

Search tiles (anchor recall, CRS≥0.74):
- `tile:formal_spec_theory-corpus-search---legominism-cluster`
- `tile:formal_spec_theory-corpus-search---lawful-cognition-cluster`
- `tile:formal_spec_theory-corpus-search---adr-bootstrap-cluster`

Hubs (676 + 460 files; stabilized via `track_c_manifold_repair.py` forget+remember, no update/pin during gate):
- `hub:theory_corpus_legominism_lawful_cognition`
- `hub:theory_corpus_monad_math_research`
- `hub:theory_corpus_static_rooster_ops`
- `hub:theory_corpus_deeplaw_ops`

Tile: `tile:knowledge_graph_theory-corpus-track-c---organized-clusters--pass`

Relations: legominism → precursor_to → deeplaw; legominism → implements_via → static-rooster-ops.

---

## 5. Open questions

1. Should `uncertain-defer` (369) block Phase 2 harness mapping, or proceed on legominism-only subset?
2. Are any False Empire attention-theology essays desired as *optional thematic* ingest (currently quarantined)?
3. Phase B async sheaf — does theory corpus inform `note()` primitive design?

---

## 6. Verification references

- Pass 1 full corpus: `scratch/track-c-full-corpus-verification.log`
- Pass 2 organize: `scratch/track-c-pass2-final-verification.log`
- Pass 2 refine: `scratch/track-c-pass2-refine-verification.log`
- Legacy `.leg` sample: `scratch/legacy-leg-sidecar-verification.log` (10/10 PASS)
- Manifold: `scratch/manifold-verify.log`
- Acceptance gate: `scratch/track-c-acceptance-gate.json` (2744 organized files)
- Deliverable index: `docs/theory-corpus-deliverable-index.json`