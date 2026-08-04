# Next improvements closure v1 — plan + execution contract

**Date:** 2026-08-03  
**Origin:** `docs/plans/next-improvements-audit-v1.md` (master `4e26b6be`)  
**Goal id:** `goal:next_improvements_closure_v1`  
**Kind:** multi-wave code + docs + release hygiene (close all top-10 audit items)

---

## North star

Ship a **trustworthy public surface** (version/CHANGELOG/README/CLAIMS) and **agent-path integrity** (primary_goal follows intent, wake is cheap, PRAXIS hard under agent profile, hybrid not oversold, BVH quality path honest, god-files stop growing for wake/lawfulness helpers).

## Principles

1. **Honesty over theater** — fix claims/docs first when code is already correct.
2. **Agent-path first** — every session hits wake / primary_goal / contracts / recall mode.
3. **Small reviewable commits** — A → B → C → D; no big-bang store split.
4. **CPU-only CI green** — no GPU productization in this goal.
5. **No invented SHAs / no PII** — never touch `docs/property/`.
6. **SCRATCH only under goal implementer dirs** when using harness.
7. **Non-goals stay non-goals** — no Autophagy GC, no SNARKs, no full store rewrite, no RH invent.

---

## Inventory → disposition (audit top-10 + residuals)

| # | Item | Best disposition | Wave |
|---|------|------------------|------|
| 1 | Release catch-up (4× Unreleased, tag lag) | Collapse Unreleased → **`[0.7.0-beta.12]`**; bump workspace Cargo; tag after merge | **A** |
| 2 | README version + What’s new | `--version` → beta.12; What’s new rewrite (≤7 bullets, honesty stack) | **A** |
| 3 | CLAIMS_LEDGER stale rows | Refresh seal/trust/wake_digest/proof harness; fix tool count note; last truth pass | **A** |
| 4 | Tool count 87 vs 83 | **Truth:** `tool_list()` = **87** unique (`83` `mcp_engram_*` + **4** linguistic). Keep public claim; add **unit test** that freezes the count | **A** |
| 5 | Sticky `primary_goal` on intent mismatch | `ENGRAM_PRIMARY_GOAL_REBIND=off\|suggest\|auto`; agent default **`auto`**: rebind marker to handoff primary when intent aligns better; else priority-0 `goal_set_primary` suggest + digest field | **B** |
| 6 | Wake still heavy | `ENGRAM_WAKE_DIGEST_ONLY=1` → session_start returns digest-first minimal packet (status, session_key, wake_digest, trust_ok, readiness_summary); full bundle still via `get_continuation_bundle` | **B** |
| 7 | Soft PRAXIS default | Agent profile: `ENGRAM_PRAXIS_CONTRACT=hard` via `set_default` (override still allowed) | **C** |
| 8 | Hybrid wire stub | Demote public hero language; encode comments already stub; ledger `partial` + “do not ship as product” | **C** |
| 9 | BVH / sampled agent quality | Document **quality path** (`ENGRAM_DEFER_BVH=0` when GPU already agent default); add `ENGRAM_QUALITY_MODE=1` alias forcing eager BVH + readiness note; no always-on RAM bomb on CPU | **D** |
| 10 | God objects | Extract `wake_digest` (+ intent overlap helpers used only there) into `wake_digest.rs`; re-export from harness_injection. **No** full store/mcp split | **D** |

### Residuals closed as docs/process (same PR wave)

| Residual | Disposition |
|----------|-------------|
| Merkle 6-deep story | CLAIMS already partial; refresh wording “seal done, chain depth shallow” |
| ZK API name | Prefer attestation in public docs; keep symbol + alias |
| Subvisor H¹ | Aspirational in CLAIMS (no code change) |
| Dual memory (Grok vs Engram) | One paragraph in AGENT_MEMORY_CONTRACT / plan § process |
| search_tool tax | Skill note: cache 8-tool highway schemas in-session (docs only) |
| Protocol invoke stub / ki stubs | Out of scope unless MCP-exposed as product (hide/demote only if public) |

### Explicit non-goals (do not implement)

- Full `store.rs` / `mcp.rs` rewrite  
- Production cuFile/GDS productization  
- GPU encode / GPU NREM  
- zk-SNARKs / full 6-deep Merkle product  
- Autophagy GC restoration  
- RH invent / personal property tracks  

---

## Success criteria (goal complete when)

1. **Version surface aligned:** Cargo = `0.7.0-beta.12`, CHANGELOG has single Unreleased empty or next-only + released beta.12 section covering #209–#218 + this closure; README version + What’s new match.  
2. **Tag:** `v0.7.0-beta.12` on the release commit (after CI green / merge).  
3. **CLAIMS_LEDGER** last truth pass = this day; seal/trust/wake_digest/proof rows current; tool count accurate.  
4. **Tool count test** fails if `tool_list()` count drifts without intentional update.  
5. **Primary rebind:** unit tests for mismatch + auto rebind from handoff; agent profile default documented.  
6. **Digest-only wake:** env flag + test that packet omits fat continuation when set.  
7. **PRAXIS hard** under agent profile when unset; test in `profile.rs`.  
8. **Hybrid** not listed as product-ready in README What’s new / hero.  
9. **Quality mode** documented + profile/readiness note; no CI flake from forcing GPU.  
10. **`wake_digest.rs` module** exists; `cargo test -p engram-server` relevant suites pass.  

---

## Wave breakdown

### Wave A — Docs/release (fast, low risk)

**Files:** `CHANGELOG.md`, `Cargo.toml` (workspace version), `README.md`, `CLAIMS_LEDGER.md`, optional `docs/AGENT_MEMORY_CONTRACT.md` tool-count line, `crates/engram-server/src/mcp.rs` test only.

**Steps:**
1. Merge four Unreleased blocks into `## [0.7.0-beta.12] - 2026-08-03` with Added/Fixed/Removed/Changed.  
2. Bump workspace version `0.7.0-beta.11` → `0.7.0-beta.12`.  
3. README quickstart comment + What’s new rewrite.  
4. CLAIMS refresh.  
5. Add `tool_list_count_is_stable` test (assert 87 names).  

**PR title:** `release: 0.7.0-beta.12 hygiene + claims/tool-count honesty`

### Wave B — Agent continuity

**Files:** `harness_injection.rs` / new `wake_digest.rs` (if D lands same branch), `mcp.rs` session_start, `store.rs` primary marker helpers, `profile.rs`, tests, skills/AGENTS one-liner.

**Behavior:**
```
ENGRAM_PRIMARY_GOAL_REBIND=
  off     — warn only (legacy digest)
  suggest — priority-0 goal_set_primary action when !aligned
  auto    — if handoff.primary_goal aligns with intent better than sticky, rewrite primary_goal marker + receipt; else suggest
```
Agent profile default: `auto`.

```
ENGRAM_WAKE_DIGEST_ONLY=1
  → response: { status, session_key, wake_digest, trust_residual?, readiness_summary, get_continuation_hint }
  → full path still get_continuation_bundle
```

**PR title:** `feat(agent): primary_goal rebind + wake digest-only mode`

### Wave C — Integrity policy

**Files:** `profile.rs`, `store.rs` comment, CLAIMS/CHANGELOG, encode hybrid comments if needed.

- Agent: `set_default("ENGRAM_PRAXIS_CONTRACT", "hard")`  
- Docs: hybrid demoted; attestation naming  

**PR title:** `feat(agent): hard PRAXIS default + hybrid surface demotion`

### Wave D — Quality + extract

**Files:** new `wake_digest.rs`, `lib`/`main` mod, `profile.rs` quality mode, readiness string, short doc in `docs/AGENT_MEMORY_CONTRACT.md` or ENV note.

- `ENGRAM_QUALITY_MODE=1` → `ENGRAM_DEFER_BVH=0` + readiness `quality_mode: true`  
- Extract only wake digest builders (low blast radius)

**PR title:** `refactor: wake_digest module + agent quality-mode BVH policy`

---

## Execution order (this session)

1. Write this plan (done).  
2. `session_start` + `goal_create` + decompose A–D + `goal_set_primary`.  
3. Branch `chore/next-improvements-closure-v1` from clean master.  
4. Implement A → B → C → D on one branch if CI-friendly; split PRs if conflicts.  
5. `cargo fmt` + targeted tests + `cargo test -p engram-server --lib` where feasible.  
6. Commit, push, open PR; tag **after** merge (do not force-push master).  

---

## Process notes (agent operating system)

| Friction | Closure |
|----------|---------|
| Dual memory | Engram = project continuity; Grok MEMORY = cross-project prefs only (document) |
| search_tool tax | Cache 8-tool highway schemas after first `search_tool` in session |
| Stacking PR CI chaos | Prefer merge base green then rebase; clippy `-D warnings` locally before push |
| Never rebase master | Feature branches + worktrees only |

---

## Rollback

- Env flags default-preserving except agent PRAXIS hard and primary rebind auto (both overridable).  
- Digest-only off by default.  
- Quality mode off by default.  
- Docs-only rollback = revert release commit.

---

## Evidence

Implementer SCRATCH (if used): under goal worktree /tmp only — not product docs/property.
