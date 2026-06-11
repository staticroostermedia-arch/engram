# Engram Consolidation Plan — One Product, Best Version

**Status:** Draft for review (2026-06-06)  
**Author intent:** Replace fragmentation with one coherent tool Grok Build can ship as default memory.  
**Supersedes (partially):** scattered survival patches, conflicting docs, env-var soup.  
**Builds on:** `design/agent_memory_mvp_plan.md`, Codeland/ac3509a9 working baseline, GitHub MVP public surface.

---

## Executive summary

Engram’s **substrate is sound** (256KB `.leg` blocks, q/p/CRS, relations, spatial AABB, MCP). What broke the experience is **layered patches** for scale/OOM/agent-doc crises without removing old paths.

**Target:** One binary, **three profiles**, **one agent story**, CUDA as default on NVIDIA hardware, large-store behavior built into `CudaBackend` (not 15 env vars).

```
┌─────────────────────────────────────────────────────────────┐
│  engram (one binary)                                        │
│    profile: agent  ← default (Grok Build MCP)               │
│    profile: deep   ← power users, full manifold + BVH       │
│    profile: ui     ← leg-browser / serve --light            │
└─────────────────────────────────────────────────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
   8-tool contract      62 tools tiered      CPU-only fast serve
   181k-safe init       OptiX optional       no MCP ritual tax
   CUDA lean path       sheaf all stalks
```

**Review ask:** Approve profile model + ablation list before implementation sprint.

---

## 1. Diagnosis — why it feels like a hot mess

### 1.1 Three crises, three patch stacks (never merged)

| Era | Crisis | Patch | Left behind |
|-----|--------|-------|-------------|
| Codeland | — | ac3509a9: sheaf + CudaBackend + primary stalk | Reference binary diverged from main |
| Scale | 181k OOM, duplicate MCP | `mcp_lock`, fast placeholder, defer BVH/watch | Eager BVH + mandatory watch in old docs |
| Agents | 62 tools, ritual tax | 8-tool contract, lean/deep mode | 55-tool README, 5-tool wake workflows |
| GPU debug | segfault / low util | `ENGRAM_FORCE_BACKEND=wgpu`, sampled recall | CUDA selected but CPU query path |

### 1.2 Fragmentation inventory (ablate most of this)

**Env vars (today ~20 user-facing):**

| Var | Role today | Consolidation |
|-----|------------|---------------|
| `ENGRAM_PROFILE` | *(missing)* | **Replace bundle below** |
| `ENGRAM_STORE` | Store path | Keep (global) |
| `ENGRAM_MEMORY_MODE` | lean/deep | Fold into profile |
| `ENGRAM_CUDA_LEAN` | GPU kernels on | Fold into profile + default CudaBackend |
| `ENGRAM_DEFER_BVH` | Skip BVH at boot | Auto from store size in code |
| `ENGRAM_DEFER_WATCH_INGEST` | Skip recursive ingest | Default in agent profile |
| `ENGRAM_DISABLE_SHEAF` / `ENGRAM_SHEAF_LEAN` | Sheaf on/off | One rule: sheaf if toml exists |
| `ENGRAM_KI_DISABLE` / `ENGRAM_KI_LEAN` / `ENGRAM_KI_TICK_SECS` | KI hijacker | Profile defaults |
| `ENGRAM_OPTIX_ENABLED` / `ENGRAM_OPTIX_LEAN` | OptiX RT | deep profile only |
| `ENGRAM_FORCE_BACKEND` | wgpu escape | **Ablate** from normal use (dev only) |
| `ENGRAM_FORCE_CPU_BACKEND` | serve --light | ui profile only |
| `ENGRAM_LEAN_RECALL_POOL` | Sample size | Sensible constant; override dev-only |
| `ENGRAM_EMBED_URL` | Embeddings | Keep (optional enhancement) |
| `ENGRAM_LOG` | Logging | Keep |

**Init paths (today 2):**

1. MCP fast placeholder → background `upgrade_from` (split-brain risk, complexity)
2. `serve` full init

**Target:** Single `StoreHandle::open(profile, path)` with size-aware branches inside.

**Doc entry points (today 6+):**

- `AGENT_MEMORY_CONTRACT.md` ✓ (keep)
- `SKILLS.md` + 4 skills ✓ (keep, align only)
- `FIRST_RUN.md` (merge into contract)
- `integrations/workflows/wake_up.md` (merge into skill)
- `HOW_WE_ACTUALLY_USE_THIS_IN_2026.md` (deep appendix)
- `AGENT_INTEGRATION_GUIDE.md` (deep appendix)
- `GROK_INTEGRATION.md` (merge into GROK_BUILD_MEMORY)

**Binaries (today 3):**

- `target/debug/engram` (source of truth)
- `~/.local/bin/engram-grok` (launcher)
- `~/.engram-ac3509a9/bin/engram` (**ablate** fallback)

---

## 2. Vision — best version of the tool

### 2.1 One sentence

**Persistent geometric memory for AI agents: one-call wake, anchor-first recall, edit-scoped spatial context, structured handoff — local, MCP-native, scales to 200k blocks on consumer GPU.**

### 2.2 What makes Engram worth shipping (non-negotiable core)

Keep and polish — this is the product:

| Layer | Keep | Why |
|-------|------|-----|
| `.leg` / HolographicBlock | ✓ | Tamper-evident, CRS, Merkle — the moat |
| Relations + process sheaf | ✓ | Declarative rituals in `processes/*.toml` |
| 8-tool agent contract | ✓ | What Grok Build actually needs |
| `session_start` inline bundle | ✓ | One-call wake |
| `context_for_edit` | ✓ | Replaces watch-at-wake |
| Anchor-first `recall` | ✓ | Goals/traces before episodic noise |
| Structured `session_end` handoff | ✓ | Continuity across restarts |
| CUDA path on NVIDIA | ✓ | BVH filter + batch cosine (wired) |
| Streaming BVH build | ✓ | Large store without 10GB RAM spike |
| `mcp_lock` | ✓ | One MCP per store |
| Subvisor / scars (deep) | ✓ | Agent governance, not daily path |

### 2.3 What Grok Build ships as default

```json
{
  "mcpServers": {
    "engram": {
      "command": "engram-grok",
      "args": ["mcp"]
    }
  }
}
```

Agent instructions load **one file:** `docs/AGENT_MEMORY_CONTRACT.md`.

No env block in user config — profile baked in launcher.

---

## 3. Profiles — replace env soup

### 3.1 `ENGRAM_PROFILE=agent` (default)

**Audience:** Grok Build, Cursor, Claude daily use.  
**Store:** `~/.engram/stalks/` (or `--store`).  
**Manifold:** Auto-detect size; >10k blocks → defer BVH at boot, tiered recall until BVH built.

| Behavior | Setting |
|----------|---------|
| Backend | CudaBackend if NVIDIA + build; else CPU |
| GPU query | Always (not a flag) — BVH traverse + cosine batch |
| BVH at boot | Skip if >50k `.leg`; background build optional after `session_start` |
| Recall | `scope=anchors` default; tiered sample when BVH not ready |
| Spatial | `context_for_edit` only; no `watch_workspace` at wake |
| Sheaf | If `~/.engram/sheaf.toml` exists → active stalk GPU, others CPU lazy |
| KI hijacker | On, anchor-scoped bake, 300s tick |
| OptiX | Off |
| MCP init | Responsive <1s ack; full backend async (keep until unified init lands) |
| Tools surfaced | 8 essential + `get_backend_readiness` |

### 3.2 `ENGRAM_PROFILE=deep`

**Audience:** TUI ritual users, meta-work, lawfulness audits, Codeland parity.

| Behavior | Setting |
|----------|---------|
| Memory mode | `deep` (full recall, auto `rebuild_bvh` on large stores) |
| BVH | Eager or on-demand via `rebuild_bvh` |
| OptiX | Lazy on first spatial query (`ENGRAM_OPTIX_ENABLED=1` opt-in) |
| KI | Full bake (or lean if store >100k — auto downgrade) |
| Sheaf | All stalks CudaBackend |
| Tools | Full 62, tiered in docs |

### 3.3 `ENGRAM_PROFILE=ui`

**Audience:** `./scripts/leg`, leg-browser, HTTP review.

| Behavior | Setting |
|----------|---------|
| Command | `engram serve --light` or profile env |
| Backend | CPU only |
| MCP | Off or HTTP `/mcp` optional |
| KI / daemon | Minimal |

### 3.4 `ENGRAM_PROFILE=dev` (internal)

Escape hatches only for debugging:

- `ENGRAM_FORCE_BACKEND=wgpu|cpu`
- `ENGRAM_LOG=debug`
- Per-flag overrides still work but **undocumented in public docs**

---

## 4. Ablation plan — what to remove or merge

### 4.1 Ablate (delete or stop shipping)

| Item | Action | Rationale |
|------|--------|-----------|
| `~/.engram-ac3509a9/bin/engram` fallback in `engram-grok` | Remove | Hides stale behavior |
| `ENGRAM_FORCE_BACKEND=wgpu` in normal docs/build | Dev-only | Was segfault bandaid |
| Duplicate wake workflows | Merge → `engram-wake-up.md` | One ritual |
| README "55+ tools" hero remnants | Already fixed; grep purge | One message |
| Fast MCP placeholder **long-term** | Phase 2: merge init | Short-term keep until unified |
| `get_continuation_bundle` at wake in docs | Mark redundant | Inline in `session_start` |
| Mandatory `watch_workspace` in any public doc | Delete statements | Replaced by `context_for_edit` |
| `integrations/workflows/wake_up.md` as standalone | Redirect to skill | Duplication |
| Multiple MCP json templates with different env | One template | `integrations/grok-build/mcp.json` |
| glama.json / scout / puppeteer in antigravity template | Not Engram core | Separate integration repo or trim |

### 4.2 Merge (keep content, one canonical file)

| Sources | Target |
|---------|--------|
| `FIRST_RUN.md` quick path | `docs/AGENT_MEMORY_CONTRACT.md` § First Run |
| `GROK_INTEGRATION.md` | `docs/GROK_BUILD_MEMORY.md` |
| `integrations/workflows/wake_up.md` | `docs/skills/engram-wake-up.md` |
| `HOW_WE_ACTUALLY_USE_THIS_IN_2026.md` | `docs/RITUALS.md` § Deep path |
| `AGENT_INTEGRATION_GUIDE.md` first-person deep | `docs/RITUALS.md` appendix or keep as `docs/DEEP_AGENT_SELF_MODEL.md` |

### 4.3 Deprecate (keep in binary, hide from agents)

| MCP tools | Tier | Agent-visible |
|-----------|------|---------------|
| `watch_workspace` | Lean-avoid | deep only |
| `summarize` at wake | Lean-avoid | deep only |
| `get_continuation_bundle` | Redundant | TUI only |
| `query_pure` at wake | Lean-avoid | deep only |
| `rebuild_bvh` | Power | deep / on-demand |
| `promote_hot_batch` at wake | Lean-avoid | deep only |

**Do not delete MCP tools** — tier + hide from default tool descriptions (`[AGENT]` / `[DEEP]` tags in `mcp.rs`).

### 4.4 Code consolidation targets

| Area | Today | Target |
|------|-------|--------|
| `apply_mcp_safe_defaults()` | 10 `set_var` calls | `Profile::agent().apply()` |
| Backend selection | cfg × env × sheaf flags | `BackendSelector::for_profile()` |
| Recall | `recall` / `recall_scoped` / `query` / `recall_sampled*` | `RecallEngine::search(scope, k)` |
| BVH | defer flag + maybe_auto + rebuild async | `BvhPolicy::from_block_count(n)` |
| CUDA | `ENGRAM_CUDA_LEAN` + cpu fallback in bvh | Default in `CudaBackend::query` |

---

## 5. Architecture — target code map

```
crates/engram-server/src/
  profile.rs          NEW — Agent | Deep | Ui | Dev + apply()
  store.rs            Slim — delegates to profile + recall_engine
  mcp.rs              Tool descriptions tagged [AGENT]/[DEEP]
  main.rs             profile from env or --profile flag

crates/engram-gpu/src/
  backend.rs          CudaBackend — lean GPU path is default, not gated
  bvh.rs              Streaming build only (done); single query path
  cuda_dispatch.rs    Keep — internal to CUDA backend

scripts/
  engram-grok         ENGRAM_PROFILE=agent only (+ CUDA paths)

docs/  (public agent surface = 4 files)
  AGENT_MEMORY_CONTRACT.md   ← agents load this
  GROK_BUILD_MEMORY.md       ← xAI / integration pitch
  MCP_TOOLS_REFERENCE.md     ← tier reference
  GEOMETRIC_MEMORY.md        ← why non-flat (optional depth)
```

---

## 6. Execution phases

### Phase 0 — Review gate (you are here)

- [ ] Approve profile model (agent / deep / ui)
- [ ] Approve ablation list (§4)
- [ ] Approve doc merge map (§4.2)
- [ ] Decide: keep fast MCP placeholder through Phase 1? **Recommended: yes**

### Phase 1 — Profile layer (2–3 days, 1 PR stack)

**Goal:** User sets zero env vars; launcher sets one profile.

| PR | Work |
|----|------|
| 1.1 | `profile.rs` + `ENGRAM_PROFILE` + `--profile` CLI flag |
| 1.2 | Replace `apply_mcp_safe_defaults()` with profile.apply() |
| 1.3 | `engram-grok` → `ENGRAM_PROFILE=agent` only; remove ac3509a9 fallback |
| 1.4 | `get_backend_readiness` returns `profile` field |
| 1.5 | Doc: single MCP template, update AGENT_MEMORY_CONTRACT with profile table |

**Exit criteria:** Fresh install with only `engram-grok` + contract doc works on 181k store.

### Phase 2 — Init unification (3–5 days)

**Goal:** One store open path; no split-brain placeholder.

| PR | Work |
|----|------|
| 2.1 | `StoreHandle::open(profile, path)` — size-aware BVH policy inside |
| 2.2 | MCP stdio: still fast ack, but same `StoreHandle` struct (no upgrade_from) |
| 2.3 | Remove `ENGRAM_DEFER_*` from public surface (internal policy) |
| 2.4 | Harness: `agent-profile-smoke` — 10-iteration transport + wake/handoff |

**Exit criteria:** No `upgrade_from` in hot path; harness green.

### Phase 3 — Recall + CUDA unification (3–5 days)

**Goal:** One recall pipeline; GPU is default on CUDA builds.

| PR | Work |
|----|------|
| 3.1 | `RecallEngine` — anchors → tiered → BVH → sampled (single module) |
| 3.2 | Remove `ENGRAM_CUDA_LEAN` flag; always GPU when runtime probe succeeds |
| 3.3 | `rebuild_bvh` triggers from profile+size, not scattered call sites |
| 3.4 | Benchmark vs ac3509a9 traces (wake, recall, RSS, GPU util) — document in plan |

**Exit criteria:** `recall_mode=full_bvh_gpu` achievable in deep profile; agent profile stable <500MB RSS.

### Phase 4 — Doc + tool surface (2 days)

| PR | Work |
|----|------|
| 4.1 | Execute doc merges (§4.2); add deprecation banners on merged sources |
| 4.2 | `mcp.rs` tool descriptions: `[AGENT]` prefix on 8 tools |
| 4.3 | README one hero, one quickstart, one table |
| 4.4 | GitHub About + topics per GROK_BUILD_MEMORY |

### Phase 5 — GitHub ship (1 day)

- Atomic commits per phase
- PR checklist: profile smoke, build, no contradictory public doc grep
- Tag `v0.5.0-consolidated` with release notes: "one profile, one agent path"

---

## 7. Success metrics (vs Codeland baseline)

| Metric | Codeland (ac3509a9) | Agent profile target | Deep profile target |
|--------|---------------------|----------------------|---------------------|
| MCP wake latency | ~1–2s | <2s on 181k | <5s (may build BVH) |
| RSS at wake | unknown | <500MB | <2GB during BVH build |
| Recall quality (anchor query) | good | top-3 contains goal/trace | full manifold |
| GPU util on recall | 15–40% | >10% sustained on 10 queries | >25% on BVH path |
| Transport stability | variable | 10/10 harness iterations | same |
| Agent doc count | many | **1 required** (+ SKILLS optional) | RITUALS for deep |
| Env vars for user | several | **0** (profile in launcher) | 0 |

---

## 8. Open decisions (need your review)

### D1 — Fast MCP placeholder

**Options:**

- **A)** Keep through Phase 2, then remove (recommended)
- **B)** Remove now, accept slower first `tools/list` on 181k

### D2 — MCP tool deletion

**Options:**

- **A)** Tier only, never delete (recommended — power users, TUI)
- **B)** Feature-gate compile out lean-avoid tools in agent builds

### D3 — Sheaf default

**Options:**

- **A)** If `sheaf.toml` exists, always use sheaf; active stalk = `--store` or `active_stalk` (recommended)
- **B)** Always single backend on `--store`; sheaf opt-in

### D4 — WGPU backend

**Options:**

- **A)** Keep for non-NVIDIA; never on this machine's agent profile (recommended)
- **B)** Remove WGPU backend entirely (smaller codebase, lose AMD/Intel path)

### D5 — OptiX in deep profile

**Options:**

- **A)** Lazy, off by default, `ENGRAM_OPTIX_ENABLED=1` for deep only (recommended)
- **B)** Invest in SM 12 native OptiX PTX (more work, uncertain ROI)

### D6 — Old docs

**Options:**

- **A)** Merge + deprecation banner + redirect (recommended)
- **B)** Delete outright (risk: broken external links)

---

## 9. What NOT to do (scope guard)

- Do not rewrite geometric core (`.leg3`, CRS, VSA ops)
- Do not flatten to vector DB for "simplicity"
- Do not add new env vars — net count must **decrease**
- Do not start new ritual variants — consolidate existing
- Do not block GitHub ship on OptiX / cuFile / full device residency

---

## 10. Review checklist

Before implementation sprint, confirm:

- [ ] **Profiles** (`agent` / `deep` / `ui`) approved
- [ ] **Ablation list** (§4.1) approved — especially ac3509a9 fallback removal
- [ ] **Doc merge map** (§4.2) approved
- [ ] **Open decisions** D1–D6 answered
- [ ] **Phase order** acceptable (profile → init → recall → docs → ship)
- [ ] **Success metrics** are the right bar

---

## Appendix A — Grep commands for validation post-consolidation

```bash
# No mandatory watch at wake in public docs
rg -l 'mandatory.*watch_workspace|watch_workspace.*every session' docs/ README.md SKILLS.md

# No ac3509a9 fallback
rg 'ac3509a9' scripts/ integrations/

# Public env var count in templates (should be 0-1)
rg 'ENGRAM_' integrations/grok-build/mcp.json

# Single profile in launcher
cat scripts/engram-grok | rg ENGRAM_PROFILE
```

## Appendix B — Relationship to prior plans

| Plan | Status after consolidation |
|------|---------------------------|
| `agent_memory_mvp_plan.md` Phase A | **Done** — becomes agent profile core |
| `agent_memory_mvp_plan.md` Phase B | Fold into Phase 2–3 above |
| `GITHUB_MVP_PREP_PLAN.md` | Phase 4–5 completes public ship |
| Lean CUDA patches | Absorbed into default CudaBackend (Phase 3) |

---

*End of consolidation plan. Comment inline or in review; implementation starts after Phase 0 approval.*