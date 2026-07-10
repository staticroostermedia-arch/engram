# Glass-Box RSI v1 — Design Spec

**Date:** 2026-07-10  
**Status:** Approved in dialogue (architecture, goals/verify, LEG home, errors/CI/testing)  
**Primary goal:** `goal:engram_mvp_v1`  
**Related parents (to mint):** `goal:dual_rsi_program`, `goal:ship_substrate`, `goal:glassbox_leg`

## Problem

Dual RSI and related scheduled loops compound agent memory (`.leg3`, PEFT, ship/PR) under token budget, but:

1. Loop fires often claim success without a structured **verify packet**.
2. Human visibility lags — **LEG Browser** was last evolved as a beta glass-box and is not wired to dual RSI / Gemma stage / PR / MCP restart state.
3. CI can be partial (e.g. one matrix job fail, one pass) while agents still need an honest “ready-to-merge” signal.
4. Goal stack is not systematically used as a **fire-level verification loop**.

## Goals (product)

- Every scheduled fire is a **child goal** under a durable parent, completed only after a **typed verify**.
- **LEG Browser** is the human mirror of process + verify state (split home).
- Prefer existing MCP + REST; add aggregate API only if hydrate is too chatty.
- No silent success: stage flip, ship claim, or “healthy” only after verify.

## Non-goals (v1)

- Auto-merge PRs.
- LEG editing goals or running loops from the browser.
- React rewrite of LEG.
- Auto-restart MCP without `ENGRAM_ALLOW_MCP_RESTART=1`.
- Full GGUF LoRA serve for Gemma4 (blocked by llama.cpp tensor map; tracked separately).

---

## §1 Architecture

```
LEG Browser :8765  (?view=glassbox)
        │ REST read-only
        ▼
engram serve :3456  →  .leg3 (~/.engram)
        ▲
        │ MCP write (verify before complete)
Loop fires (Dual RSI, Ship, PR Watch, MCP Stale, Aliveness, …)
```

**Rules**

1. No silent success without typed verify.
2. LEG is read-only; does not execute loops.
3. Phase A (process contract) before Phase B (LEG home).
4. Prefer `/api/block`, presentation, activity SSE; optional later `GET /api/glassbox`.

---

## §2 Goal + verify contract

### Parent goals (durable)

| Parent | Owns |
|--------|------|
| `goal:dual_rsi_program` | Tracks S/G/M, stage machine, corpus/PEFT |
| `goal:ship_substrate` | Dirty-tree → test → PR |
| `goal:glassbox_leg` | LEG split home + any glassbox API |

Parents **serve** `goal:engram_mvp_v1`.

### Child fire goals

Mint at fire start:

`goal:fire_<loop>_<job_or_session>_<ts>`

Required fields (goal text and/or related `metric:verify_*`):

| Field | Meaning |
|-------|---------|
| `parent` | Durable parent goal id |
| `loop` | `dual_rsi` \| `ship_gate` \| `pr_watch` \| `mcp_stale` \| `aliveness` \| … |
| `track` | `S` \| `G` \| `M` \| null |
| `intent` | One line |
| `verify_type` | Typed gate id |
| `verify_status` | `pending` \| `pass` \| `fail` |
| `verify_evidence` | Paths, test summary, CI URL, metric concept |
| `falsify` | What would reverse this fire |

### Typed gates

| Loop | `verify_type` | Pass means |
|------|---------------|------------|
| Dual RSI **S** | `substrate_local` | Disk artifact and/or targeted test + integrity sample; no pack dump in chat |
| Dual RSI **G** | `gemma_stage` | Stage advanced + metric atom status ok (`peft_metrics` / `eval_gate` / future `gguf_lora`) |
| Dual RSI **M** | `meta_policy` | dual_loop updated with rationale; optional scar |
| Ship | `ship_local` | Tests green + commit + PR URL, or explicit `ship_skip` if no code dirty |
| PR watch | `ci_status` | Check rollup recorded; all **required** checks SUCCESS for ready-to-merge; else not ready |
| MCP stale | `binary_vs_proc` | FRESH / STALE / OFFLINE atom; restart only if allowed |
| Aliveness | `metrics_atom` | `metric:dual_rsi_aliveness_*` written and related |

### Lifecycle

```
session_start
  → ensure parent related to engram_mvp_v1
  → mint child goal (active, verify=pending)
  → act (one track / one ship / one pr check)
  → run typed verify
  → IF pass: complete child + dual_loop update
  → IF fail: block/abandon child + scar if repeated + dual_loop blockers
  → session_end (must include child goal id + verify_status)
```

### `helper:rsi_dual_loop_state` schema extensions

```json
{
  "open_pr": "url|null",
  "mcp_restart_required": false,
  "last_fire_goal": "goal:fire_...",
  "last_verify": { "type": "...", "status": "pass|fail", "at": "ISO-8601" },
  "parents": ["goal:dual_rsi_program", "goal:ship_substrate", "goal:glassbox_leg"],
  "track_next": "S|G|M",
  "gemma": { "stage": "...", "adapter_path": "...", "sft_rows": 0 }
}
```

---

## §3 LEG split home

**Entry:** `./scripts/leg --live` → `http://127.0.0.1:8765/?view=glassbox`

### Layout

| Region | Content |
|--------|---------|
| **Top — health strip** | fidelity, mean hub CRS, hermies cos, gemma stage, track_next, open_pr, CI pill, mcp_restart_required, last_verify |
| **Center — goals + last fire** | Parent cards (`dual_rsi_program`, `ship_substrate`, `glassbox_leg`) with last child fire, verify pass/fail, evidence one-liner |
| **Right — activity** | Existing SSE / activity feed; click → block inspector |

### Data sources (Phase B1 — no new API required)

| UI | Source |
|----|--------|
| Health | `helper:rsi_dual_loop_state`, latest aliveness metric, `/health` |
| Parents / fires | `/api/block/goal:*` + relations |
| open_pr | dual_loop field; external link only |
| Activity | existing feed |

Optional Phase B2: `GET /api/glassbox` one-shot aggregate if multi-fetch is too slow.

### Interaction

- Parent → list child fires (newest first).
- Fire → verify packet + falsify.
- Amber banner if `mcp_restart_required=true`.
- Read-only; no “run loop” in v1.

### Out of scope for LEG v1

React rewrite; goal editing; auto-merge; full CI log streaming.

---

## §4 Errors, CI, testing

### Failure table

| Failure | Process | LEG |
|---------|---------|-----|
| Verify fail | Child → blocked; scar if repeated; no stage flip | Red fire card |
| Flaky / partial CI | Not ready-to-merge until all required checks SUCCESS | Yellow CI pill |
| dual_loop missing | Still mint child; scar thin handoff | Amber unknown |
| MCP STALE | `mcp_restart_required=true`; no auto-kill unless allowed | Restart banner |
| Doom loop (same fail 2×) | Scar + stop fixing that fire | Scarred fire |
| Ship skip (clean tree) | Child complete with `ship_skip` | Grey skip, not green hero |

### CI policy

- Ship verify = **local** tests only.
- PR watch verify = **remote** rollup; ready only if every required check is SUCCESS.
- No auto-merge in v1.
- One narrow CI fix max per PR-watch fire; second same failure → scar.

### Testing the program

| Layer | What |
|-------|------|
| Schema | dual_loop field parse; verify packet presence |
| Loop dry-run | Throwaway namespace: mint child → pass/fail → status |
| LEG | Static glassbox fixture + live checklist |
| Regression | Ship cannot claim PR without URL; PR watch cannot mark ready on red CI |

---

## Phased delivery

| Phase | Deliverable |
|-------|-------------|
| **A1** | Mint parent goals; dual_loop schema fields; rewrite loop prompts with fire goal + typed verify |
| **A2** | Land/fix PR #58 CI; document MCP restart after server binary merge |
| **B1** | LEG `?view=glassbox` split home on existing APIs |
| **B2** | Optional `/api/glassbox` aggregate |
| **C** | Deep links (CI refresh, stage diagram) |

---

## Live context (2026-07-10)

- Dual RSI: eval_gate **pass**; SFT ~51 rows; PEFT adapter on disk; GGUF convert **blocked** (Gemma4 tensor map).
- PR: https://github.com/staticroostermedia-arch/engram/pull/58 — open; mixed build-and-test historically.
- Control: `helper:rsi_dual_loop_state`.
- LEG: `tools/leg-browser/index.html`, `docs/LEG_BROWSER.md`, `./scripts/leg --live`.

---

## Success criteria

1. A Dual RSI fire that skips verify cannot flip stage in dual_loop without a failing/pending child goal visible in LEG (or dual_loop last_verify fail).
2. Ship fire either opens PR with local green tests or records `ship_skip`.
3. PR watch never reports ready-to-merge with a required check FAILURE.
4. LEG glassbox view shows health strip + three parent cards + last fire within one live load.
5. Human can answer “what did the last fire claim and prove?” from LEG alone without chat scrollback.

---

## Open questions (post-v1)

- When llama.cpp maps Gemma4 LoRA tensors, re-open G track `gguf_lora` with `gemma_stage` verify.
- Whether consciousness L7 should get parent `goal:consciousness_loop` or stay meta-only.
- Token budget: reduce L7 cadence while L1 runs if wallet contention continues.
