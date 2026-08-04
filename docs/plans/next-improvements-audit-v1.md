# Next improvements audit v1 — Engram after honesty-closure (#215–#218)

**Date:** 2026-08-03  
**Master tip analyzed:** `4e26b6be` (post #218 Autophagy surface removal)  
**Workspace version at analysis:** `0.7.0-beta.11`  
**Status update 2026-08-04:** Closed via **#219** (`v0.7.0-beta.12` tag on `48c5bb4b`) + residuals PR (lawfulness extract, BVH quality path force, protocol-invoke demotion). See disposition table below.  
**Kind:** analysis + post-hoc disposition (original body is historical snapshot)

---

## 1. Executive summary

Engram’s **agent memory loop is real and recently more honest** (seal-aware verify, `wake_digest`, intent demotion of scars, no Autophagy product myth). The largest remaining risks are not “missing cosmology” but **release hygiene**, **stale public messaging**, and **agent-path friction** that still burns tokens and mis-prioritizes work.

| Rank | Theme | Disposition (2026-08-04) |
|------|--------|----------|
| 1 | **Release / version alignment** | **Closed** — #219 + tag `v0.7.0-beta.12` |
| 2 | **CLAIMS_LEDGER hygiene** | **Closed** — refreshed on #219 + residuals |
| 3 | **Tool-count claim drift** | **Closed** — 87 enforced by unit test + docs |
| 4 | **Wake still heavy** | **Closed (opt-in)** — `ENGRAM_WAKE_DIGEST_ONLY=1` (not default) |
| 5 | **Soft contracts by default** | **Closed** — agent `ENGRAM_PRAXIS_CONTRACT=hard` |
| 6 | **Merkle depth honesty** | **Deferred honest** — seal done; 6-deep walk still partial (CLAIMS) |
| 7 | **Hybrid wire + ZK API names** | **Closed (demote)** — experimental stub / attestation wording |
| 8 | **BVH / NVMe-as-context** | **Closed (policy)** — GPU eager; QUALITY_MODE forces BVH; hint on readiness |
| 9 | **God objects** | **Partial** — `wake_digest.rs` + `lawfulness.rs` extracts; no full rewrite |
| 10 | **Agent process** | **Closed (docs)** — dual-memory + env table in AGENT_MEMORY_CONTRACT |

---

## 2. Version / release discrepancies

### Observed facts (spot-checked)

| Surface | Value |
|---------|--------|
| Workspace `Cargo.toml` | `0.7.0-beta.11` |
| Local `engram --version` | `engram 0.7.0-beta.11` |
| Git tags `v0.7*` | `v0.7.0-beta.1` … **`v0.7.0-beta.5` only** (no beta.6–beta.11 tags) |
| README quickstart | `# 0.7.0-beta.5` comment on `--version` |
| README section | `## What's new (v0.7.0-beta.5+)` |
| CHANGELOG | **4×** `## Unreleased` / `## [Unreleased]` headers stacked at top (autophagy, PRAXIS/wake/lawfulness, sig_5 seal, NREM/REST/trust residual) then `## [0.7.0-beta.6]` dated 2026-06-30, then beta.5… |

### Implications

1. **Public “release” and crate version have diverged by six beta increments** with no tags for beta.6–beta.11. Downstream (GitHub Releases, Glama, installers, “what shipped”) cannot trust tags.
2. **README actively teaches the wrong version string** in the first copy-paste block.
3. **CHANGELOG is not Keep-a-Changelog–clean**: multiple Unreleased sections + unreleased work that *is* on master (honesty-closure, sig_5, NREM fix, trust residual) should be **rolled into a single `[0.7.0-beta.12]` or a deliberate “changelog catch-up” release note**, then tagged.
4. CHANGELOG still documents features under beta.6 that README never elevates to “What’s new” (tensor–tile unification, etc.), while README “What’s new” freezes the story at beta.5-era continuity spikes.

### Recommended disposition

| Action | Type |
|--------|------|
| Collapse all Unreleased into one section; cut **`0.7.0-beta.12`** (or renumber honestly) with bullet list of #209–#218 | **Release hygiene** |
| Tag `v0.7.0-beta.12` matching Cargo after CHANGELOG edit | **Release hygiene** |
| README: `--version` example → `0.7.0-beta.11`+; “What’s new” → current beta + 5 bullets max | **Docs** |
| Optional: GitHub Release notes from CHANGELOG only (no code) | **Process** |

---

## 3. Claims vs code (post #215–#218)

### What is solid on master now

| Area | Status |
|------|--------|
| Whole-block `sig_5` seal + stack-safe digest | **Implemented** |
| Seal-aware lawfulness + manifold seal sample | **Implemented** (#215) |
| `wake_digest` + intent demotion of scars | **Implemented** (#216) |
| Trust residual on wake | **Implemented** (#210) |
| NREM dedicated stack / REST `scope=all` | **Implemented** (#209) |
| Proof harness CI | **Implemented** (#213) |
| Autophagy as product GC | **Removed** (#218); `forget_old` = explicit only |
| PII `docs/property/` | **Gitignored** (#214) |

### Residual partial / risk claims (still true)

| Claim / area | Reality on master | Disposition |
|--------------|-------------------|-------------|
| **6-deep Merkle temporal crystal** | Updates mostly `sig_0`/`sig_1`; seal is separate; no full history walk | **Demote docs** + ledger row already partial — refresh wording that seal is done but chain depth is not |
| **CLAIMS_LEDGER row “seal hardening separately”** | Stale — seal shipped | **Update ledger** (docs-only) |
| **Trust residual “merges when PR greens”** | Stale — merged | **Update ledger** |
| **Lean wake “intent mismatch is friction”** | Partially fixed by wake_digest; **primary_goal marker still sticky** | **Implement** optional intent→primary_goal rebind *or* demote primary_goal when mismatch |
| **87 MCP tools** | ~**83** `mcp_engram_*` name strings in `mcp.rs` (+ linguistic tools separate) | **Count + fix docs** |
| **NVMe as crypto context extension** | Needs warm BVH/`full_bvh_gpu`; agent often `sampled_bounded` | **Process**: readiness gate; **docs**: capability-gated |
| **cuFile/GDS** | Labels improved (`device_residency_mode`); not full GDS product | **Keep honesty**; don’t re-hype |
| **Hybrid wire** | Decode does not restore q/p | **Demote or delete** from public CHANGELOG hero lists |
| **`generate_zk_proof` name** | Documented as attestation; symbol still says ZK | **Rename alias already exists** (`generate_transform_attestation`); **deprecate ZK name in public API docs** |
| **Linguistic “synthetic calculus”** | Ops exist; marketing > daily use | **Demote README hero** or add cookbook only |
| **Subvisor H¹ / OP_INVERT** | Partial process layer | **Docs: aspirational** unless scoped MVP |
| **Lawfulness deep history** | Sampled seals; not full registry-free lineage reconstruction | **OK as partial**; don’t oversell |
| **Soft PRAXIS/contracts default** | Hard is env opt-in | **Process decision**: when to default hard for agent profile |
| **Protocol invoke stub** | `stub_dispatch` vertical slice in store | **Implement or hide MCP** |
| **MCP fast placeholder** | Real; race if agents write before upgrade | **Document + soft block writes** until upgraded |

### CLAIMS_LEDGER maintenance debt

Ledger is valuable but **behind master** in several “Notes” cells. Next docs PR should:

1. Mark seal row notes as current (not “hardening separately”).  
2. Mark trust residual as merged.  
3. Add rows for `wake_digest_v1`, intent-shaped queue, proof harness CI.  
4. Fix tool count when re-counted.  
5. Set last truth pass date after that PR.

---

## 4. Codebase quality gaps (not every TODO)

### High blast-radius structure

| File | LOC (approx) | Risk |
|------|--------------|------|
| `store.rs` | ~16.7k | Every honesty/wake fix risks collateral |
| `mcp.rs` | ~12k | Tool surface + dispatch god-object |

**Next (process + small extracts):** continue extracting **only** hot modules already started (`block_integrity` in core; lawfulness/wake_digest candidates). Do **not** big-bang split without harness coverage.

### Stubs / incomplete (prioritized)

| Item | Path | Priority |
|------|------|----------|
| Hybrid wire decode stub | `encode.rs` `from_hybrid_wire` | Medium (honesty) |
| Protocol invoke stub | `store.rs` stub_dispatch | Medium if MCP-exposed |
| Device residency / GDS | `engram-gpu` | Low unless hardware roadmap |
| ki_hijacker spatial/geo stubs | `ki_hijacker.rs` | Medium for LEG/Cursor dogfood |
| OPERATIONAL glue stubs on relate | `mcp.rs` | Low–medium (pollution of manifold) |

### Soft gates that still shape agent behavior

- Default contract enforcement: **soft proceed** (except agent update-transform soft reject, PRAXIS hard if env set).  
- Edit-arc gate often soft.  
- Sentinel rehydrate: soft only (intentional).  
- Tool tier: soft warnings for non-lean tools.

### CPU vs GPU (remaining)

| Path | Notes |
|------|--------|
| Encode / seal / NREM | Correctly CPU-heavy; do not force GPU for seal |
| Recall | BVH GPU when ready; else Rayon linear / sampled — **main agent quality cliff** |
| Agent profile defer BVH | Protects RAM; hurts recall quality — product tension |

---

## 5. Use-case and agent-process improvements

### Use cases Engram should win (and current gaps)

| Use case | Strength | Gap to address next |
|----------|----------|---------------------|
| **Long multi-session agent on one codebase** | Traces, goals, code atlas, handoff | Primary goal sticky across context switches; wake still noisy |
| **Honest research / residual tracking** | Scars, idea-gate, lawfulness | Scar injection can still appear in full open_scars list even when digest demotes |
| **Human review (LEG Browser)** | Live mirror | Spatial/ki “not yet created” stubs; galaxy scale |
| **Local-first memory vs SaaS RAG** | MCP local | Install story still heavy; version confusion hurts first-run trust |
| **Land-trust / personal ops** | Engram works as notebook | Must stay out of git (done); dual-memory (Grok MEMORY) still holds PII-capable notes |

### Agent process (how *we* and other agents work)

| Friction | Impact | Next |
|----------|--------|------|
| **Grok `search_tool` before every Engram MCP call** | High token/latency tax | Process: cache schemas for 8-tool highway in session; Engram skill slash commands |
| **Dual memory** (Grok MEMORY.md + Engram stalk) | Split brain | Document single source of truth: Engram for project continuity; Grok memory for cross-project prefs only |
| **Wake without reading `wake_digest` first** | Miss intent alignment | Skills/AGENTS: mandatory “read digest then handoff” |
| **Stacking PRs on each other** | CI chaos (#216 clippy) | Process: prefer merge 215 then rebase 216; clippy `-D warnings` in pre-push |
| **Local master accidental rebase** | Dangerous | Process: never `git rebase` on master branch; worktrees only |
| **Goal/plan files vs product** | Continuity | Keep `docs/plans/*` for meta; don’t claim them as runtime |

---

## 6. Ranked “do next” list (top 10)

Each item: **problem → why it matters → disposition**.

1. **Release catch-up (version + CHANGELOG + tag)**  
   - **Problem:** beta.11 code, beta.5 tags/README, 4 Unreleased sections.  
   - **Why:** Public trust and install docs are wrong on day one.  
   - **Disposition:** **Release hygiene** PR (docs only) then tag.

2. **README version + What’s new rewrite**  
   - **Problem:** Still teaches beta.5.  
   - **Why:** First-run mismatch.  
   - **Disposition:** **Docs**.

3. **CLAIMS_LEDGER refresh (post #215–#218)**  
   - **Problem:** Stale notes; missing wake_digest/proof harness rows.  
   - **Why:** Ledger is the honesty scoreboard.  
   - **Disposition:** **Docs**.

4. **Recount and fix tool counts (87 vs 83)**  
   - **Problem:** Public count drift.  
   - **Why:** Easy falsifiable claim.  
   - **Disposition:** **Docs** (+ optional one-line test counting tools).

5. **Primary-goal rebind or demotion on intent mismatch**  
   - **Problem:** Digest warns but `primary_goal` marker still RH-class sticky.  
   - **Why:** Agents still serve wrong goal graph.  
   - **Disposition:** **Implement** (small, agent-profile only) *or* demote marker to handoff-selected goal.

6. **Further shrink default wake payload**  
   - **Problem:** Digest is additive; firehose remains.  
   - **Why:** Token cost dominates agent sessions.  
   - **Disposition:** **Implement** `ENGRAM_WAKE_DIGEST_ONLY=1` or move readiness under lazy fetch.

7. **Default agent-profile PRAXIS/contract policy**  
   - **Problem:** Soft-by-default allows soft Praxis pollution.  
   - **Why:** Load-bearing ops integrity.  
   - **Disposition:** **Process decision** → maybe `ENGRAM_PROFILE=agent` implies hard Praxis.

8. **Hybrid wire: remove from public surface or finish**  
   - **Problem:** Stub decode blessed by tests.  
   - **Why:** Release notes still mention hybrid wire.  
   - **Disposition:** **Demote docs** (preferred) or implement decode.

9. **BVH warm path for agent memory quality**  
   - **Problem:** sampled/linear under agent defaults.  
   - **Why:** Recall quality is the product.  
   - **Disposition:** **Implement** background BVH always-on with RSS caps *or* document “quality mode”.

10. **Extract lawfulness + wake_digest modules from store/mcp**  
    - **Problem:** 28k LOC god-files.  
    - **Why:** Next honesty/CI fixes will keep colliding.  
    - **Disposition:** **Implement** narrow extract only.

---

## 7. Explicit non-priorities (do not start next)

- Full store.rs/mcp.rs rewrite in one PR  
- Production cuFile/GPUDirect productization  
- GPU encode / GPU NREM  
- zk-SNARKs or true 6-deep Merkle history product  
- Completing Autophagy GC (removed; leave dead)  
- RH invent / personal property tracks  

---

## 8. Suggested next goal shape

**Goal A (docs/release, 1–2 days):** items 1–4 — version catch-up, README, CLAIMS, tool count.  
**Goal B (agent continuity, 2–4 days):** items 5–6 — primary_goal rebind + wake slim mode.  
**Goal C (integrity policy, 1–2 days):** items 7–8 — agent hard Praxis default + hybrid demotion.  
**Goal D (quality/scale, longer):** items 9–10 — BVH policy + module extract.

---

## 9. Sources consulted

- `Cargo.toml`, `git tag -l 'v0.7*'`, `engram --version`  
- `README.md`, `CHANGELOG.md`, `CLAIMS_LEDGER.md`, `MANIFESTO.md`  
- `docs/plans/honesty-closure-v1.md`, master log #209–#218  
- Code greps: stubs, ZK, hybrid, soft enforce, wake_digest, LOC, tool names  

Evidence artifacts for this analysis live under the goal SCRATCH directory (version-claim-spotcheck, optional-code-scan, next-list-extract, delivery-summary).
