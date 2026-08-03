# Honesty Closure Plan v1 — Fix stubs, soft gates, and claim/code gaps

**Status:** Active execution plan  
**Date:** 2026-08-03  
**Origin:** Full honesty review post-SOL (block integrity, claims ledger, dogfood PRs)  
**Goal kind:** multi-wave code + docs + CI (no invented features)

---

## North star

**Agents and humans must be able to trust Engram’s tools and docs.**  
Every public claim either (A) has a real implementation + tests, or (B) is explicitly demoted to partial/aspirational with accurate wording. Prefer (A) for agent-facing verify/wake/readiness; prefer (B) for marketing names that cannot be made true in one program (full GDS, SNARK ZK, store.rs split).

---

## Principles (how we choose “best”)

1. **Honesty over theater** — never leave a tool that claims to verify and only pretty-prints.
2. **Agent-path first** — wake, verify, recall readiness, contracts that agents hit every session.
3. **Small reviewable PRs** — one concern per PR; green CI before merge.
4. **CPU-only CI must stay green** — GPU paths feature-gated; readiness must not greenwash missing GDS.
5. **No 256KB stack copies** — seal/digest patterns already fixed; do not reintroduce.
6. **PII stays out** — never touch `docs/property/`; respect gitignore.
7. **CLAIMS_LEDGER is the scoreboard** — update rows when status changes.

---

## Inventory → disposition

| # | Item | Disposition | Wave |
|---|------|-------------|------|
| H1 | `verify_block_lawfulness` is a pretty-printer | **Implement** real integrity + contract summary | A |
| H2 | Manifold verify ignores whole-block seal / relations default off | **Implement** seal sample + optional relation lineage | A |
| H3 | CLAIMS_LEDGER stale post-#212 | **Docs** refresh rows | A |
| H4 | 6-deep Merkle “temporal crystal” overclaim | **Docs + light code**: report chain *depth present*; no fake 6-deep walk without history log | A |
| H5 | “ZK” = BLAKE3 cookie | **Rename/docs**: attestation API + CHANGELOG/README | A |
| H6 | Hybrid wire stub round-trip | **Demote**: mark experimental; fix test name; docs | A |
| H7 | Intent ignored at wake (sticky goal/scars) | **Implement** intent-ranked suggested_actions + scar filter | B |
| H8 | Wake slim packet still firehose | **Implement** `wake_digest` top-level compact view | B |
| H9 | cuFile/GDS residency stub greenwashing | **Honesty**: readiness fields explicit `stub`/`unavailable`; never imply DMA | B |
| H10 | BVH not ready → silent sampled/linear | **Honesty + nudge**: readiness + wake digest show `recall_mode` + agent hint | B |
| H11 | Soft contracts on PRAXIS | **Implement** env `ENGRAM_PRAXIS_CONTRACT=hard` default soft for compat | C |
| H12 | verify_manifold `include_relation_integrity` hard-coded false on MCP | **Implement** wire arg + sample checks | C |
| H13 | NREM always CPU | **Document** (correct for 256KB blocks); optional later GPU batch | C docs |
| H14 | Encode always CPU | **Document** (intentional); optional bulk GPU later | C docs |
| H15 | store.rs / mcp.rs god objects | **Partial extract** only high-churn verify + wake_digest modules (not full split) | C |
| H16 | Dual memory (Grok MEMORY vs Engram) | **Doc + optional** note in wake_digest; no full sync product | C docs |
| H17 | Autophagy incomplete vs manifesto | **Docs** soften MANIFESTO; keep forget_old as-is | C docs |
| H18 | External pointer lazy fetch | **Docs** demote to descriptor-only | C docs |
| H19 | Proof harness extend | **Add** lawfulness integrity cases once H1 done | B |
| H20 | Agent memory contract / AGENTS.md | **Docs** point to honest verify + wake_digest | B |

---

## Wave A — Truth tools & language (must ship first)

### A1. Real block lawfulness (`feat/honest-verify-lawfulness`)

**Code**
- Extend `get_block_lawfulness_summary` (or new `audit_block_lawfulness`) to return JSON-capable struct:
  - `integrity: BlockIntegrityStatus` via `engram_core::verify_block_integrity`
  - `chain_slots_nonzero: {sig_0..sig_5}` counts / booleans
  - `contract_ok` / `contract_mode` (soft vs hard later)
  - `lawful: bool` = structure ok && (Valid \| LegacyUnsealed) && no hard contract fail
- MCP `mcp_engram_verify_block_lawfulness` returns structured text **and** machine fields; **remove** “coming in follow-up” lie.
- For relation-tagged blocks: if `merkle_sub_root` nonzero, note that endpoint re-check needs `from`/`to` (or store-side relate reverse lookup if cheap).

**Tests**
- Unit: sealed block → Valid; flip byte on disk → Mismatch reported via store path; legacy zero sig_5 → LegacyUnsealed still `lawful=true` with status.
- MCP or store-level test without full GPU.

### A2. Manifold integrity samples seals (`feat/honest-verify-lawfulness` same PR if small)

**Code**
- In `verify_manifold_integrity`, for each sampled block run `verify_block_integrity`; count valid / legacy / mismatch / structural.
- Report in `ManifoldVerificationReport` + MCP message.
- Wire MCP `include_relation_integrity` from args (default false for cost).

**Tests**
- Inject one mismatched seal in temp store; sample catches issue when sample includes it (force single-block store).

### A3. Honesty docs pass (`docs/honesty-language-wave-a`)

**Docs**
- CLAIMS_LEDGER: sig_5 → implemented; verify_block_lawfulness → implemented (with sample limits); ZK → attestation; hybrid → experimental stub; 6-deep chain → partial depth-present.
- README / MANIFESTO / CHANGELOG: ZK → “transform attestation (BLAKE3)”; hybrid wire demoted; Merkle wording already softened — align with ledger.
- encode.rs comments: rename public docs for `generate_zk_proof` → keep fn names for API stability but rustdoc says attestation; optional aliases `generate_transform_attestation`.

**Hybrid wire**
- Rename test `p2_hybrid_wire_roundtrip_stub` → keep but assert stub behavior explicitly (`from_hybrid_wire` does not restore q).
- CHANGELOG: experimental / not production path.

---

## Wave B — Agent continuity (wake + readiness)

### B1. Intent-shaped wake queue (`feat/intent-shaped-wake`)

**Code**
- `build_suggested_actions*` / ultra_lean: score actions by token overlap / keyword match of `session_intent` against goal/scar/handoff text.
- Demote scars/goals with zero intent overlap below intent-matching handoff/next_vector.
- When handoff `next_vector` present, pin as priority-0 action (already sometimes); **force** include handoff read before unrelated scars when intent mismatches primary_goal.
- Soft field: `continuation.intent_match: { primary_goal_aligned: bool, note }`.

**Tests**
- With primary_goal RH + intent “land trust questionnaire”, suggested_actions must not lead with RH scar alone; handoff/next_vector ranks high.

### B2. Wake digest (`feat/intent-shaped-wake` or follow-on)

**Code**
- Top-level `wake_digest` on session_start packet:
  ```json
  {
    "version": "wake_digest_v1",
    "primary_goal": "...",
    "next_vector": "...",
    "recall_mode": "full_bvh_gpu|sampled_bounded|...",
    "integrity_hint": "call verify_* if dual_gate soft",
    "top_actions": [ /* max 3 */ ],
    "top_scars": [ /* max 2, intent-filtered */ ],
    "trust_ok": true,
    "warnings": []
  }
  ```
- Full readiness dump remains under `readiness` / `continuation` for power users.
- AGENT_MEMORY_CONTRACT: “Read `wake_digest` first.”

**Tests**
- Wake bundle / session_start unit: digest keys present; ≤3 actions.

### B3. Readiness honesty (cuFile + BVH) (`feat/readiness-honesty`)

**Code**
- `cufile_transfer_path` / residency: explicit values `unavailable` | `stub` | `active` — never imply active when stub.
- When `device_residency` stub registers: log + readiness `device_residency: "stub"`.
- Wake digest `warnings` push if `recall_mode` is `sampled_bounded` on large_manifold.

**Tests**
- Unit on readiness JSON shape; no false `active` when stub.

### B4. Proof harness extension

- After A1: harness section “lawfulness reports mismatch on corrupted sealed block” via core APIs (already partially there) + optional store audit if cheap.

---

## Wave C — Contracts, relations, debt containment

### C1. PRAXIS hard contract (opt-in)

- `ENGRAM_PRAXIS_CONTRACT=soft|hard` (default soft).
- Hard: update/store of PRAXIS without `evidence_update` in DSL → Err (not just warning).
- Test both modes.

### C2. Relation integrity sample

- When `include_relation_integrity=true`, sample relation blocks; if endpoints resolvable, `verify_relation_lineage`.
- Document cost.

### C3. Module extract (limited)

- Move lawfulness audit helpers to `crates/engram-server/src/lawfulness_audit.rs`.
- Move `wake_digest` builder next to `wake_bundle.rs`.
- **Do not** attempt full store.rs split in this goal.

### C4. Docs-only demotions

- NREM/encode CPU intentional.
- Autophagy / external pointer / dual-memory caveats.
- MANIFESTO autophagy continuous-daemon claim → “supported via forget_old + thresholds when enabled”.

---

## Out of scope (explicit non-goals)

- Full GPU encode / GPU NREM rewrite  
- Real production cuFile GDS pipeline  
- zk-SNARK / true ZK  
- Full store.rs / mcp.rs decomposition  
- Intent NLP model (keyword/heuristic only)  
- Merging RH research or personal property docs  

---

## PR sequence

| Order | Branch | Contents |
|------:|--------|----------|
| 1 | `feat/honest-verify-lawfulness` | A1+A2+tests |
| 2 | `docs/honesty-language-wave-a` | A3 + CLAIMS_LEDGER (or same PR as 1 if tiny) |
| 3 | `feat/intent-shaped-wake` | B1+B2+tests+contract docs |
| 4 | `feat/readiness-honesty` | B3+B4 |
| 5 | `feat/praxis-contract-hard` | C1+C2 |
| 6 | `refactor/lawfulness-wake-modules` | C3+C4 |

Each PR: `cargo build -p engram-server --features cpu-only`, targeted tests, `cargo fmt`, proof-harness if touch seal/recall.

---

## Acceptance criteria (goal complete)

1. `mcp_engram_verify_block_lawfulness` reports `BlockIntegrityStatus` and never claims “coming later” for seal checks.  
2. Manifold verify samples seal status; mismatch surfaces as issue.  
3. CLAIMS_LEDGER + README/CHANGELOG honest on ZK, hybrid, Merkle depth, cuFile.  
4. Wake packet includes `wake_digest`; intent re-ranks scars/goals.  
5. Readiness does not claim active GDS when stub.  
6. Optional hard PRAXIS contract env works with test.  
7. All PRs green on CI (build-and-test + agent-memory where required).  
8. No PII paths committed.

---

## Verification plan

1. Unit tests for seal status in lawfulness audit.  
2. Temp store: corrupt one block → manifold issues_found ≥ 1.  
3. Intent wake unit: RH primary + land-trust intent → handoff-ranked actions.  
4. `grep -i 'coming in follow-up' crates/engram-server` empty for lawfulness.  
5. `grep -i 'zero-knowledge' README CHANGELOG` only if reworded to attestation.  
6. Proof harness PASS after wave A/B.  
7. `CLAIMS_LEDGER.md` rows for seal/verify/ZK match code.

---

## Risks

- Wake ranking changes may surprise agents relying on scar-first queue → keep scars but demote; document.  
- Hard PRAXIS default would break soft writers → opt-in only.  
- Manifold seal sampling adds BLAKE3 of 256KB × N — keep sample small; seal path already stack-safe.  
- Master branch protection → all via PRs.

---

## Execution order for the agent goal

Execute waves A → B → C. Do not start C until A merged or stacked cleanly on A. Prefer mergeable independent docs PR if code PR blocked.
