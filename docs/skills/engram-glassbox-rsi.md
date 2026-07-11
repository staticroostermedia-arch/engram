---
name: engram-glassbox-rsi
---

# Engram Glass-Box RSI — Fire Lifecycle Skill

**For scheduled loop operators** (Dual RSI, Ship, PR Watch, MCP Stale, Aliveness) on the Engram substrate.

Every fire is a **child goal** under a durable parent, completed only after a **typed verify**. No silent success: stage flip, ship/PR claim, or “healthy” only with `verify_status=pass`.

**Spec:** [docs/superpowers/specs/2026-07-10-glassbox-rsi-design.md](../superpowers/specs/2026-07-10-glassbox-rsi-design.md)  
**Schemas:** [docs/schemas/fire_verify_packet_v1.json](../schemas/fire_verify_packet_v1.json), [docs/schemas/dual_loop_state_v1.json](../schemas/dual_loop_state_v1.json)  
**Control block:** `helper:rsi_dual_loop_state`  
**LEG mirror:** `./scripts/leg --live` → `http://127.0.0.1:8765/?view=glassbox` (read-only)

---

## When to use

Use this skill for **any scheduled fire** of:

| Fire | Typical loop id |
|------|-----------------|
| Dual RSI (S / G / M one-track) | `dual_rsi` |
| Ship gate (dirty-tree → test → PR) | `ship_gate` |
| PR watch (remote CI rollup) | `pr_watch` |
| MCP stale (binary vs process) | `mcp_stale` |
| Aliveness bench | `aliveness` |

Also use when manually replaying a fire dry-run or recovering a failed verify (mint new child; do not rewrite history of a completed fire).

**Do not use** for ordinary code-edit sessions that are not loop fires — use [engram-working-memory.md](engram-working-memory.md) + Code Edit Ritual instead.

---

## Parent goals (durable)

Parents **serve** `goal:engram_mvp_v1`. Mint once (if missing); do not re-mint per fire.

| Parent | Owns |
|--------|------|
| `goal:dual_rsi_program` | Tracks S/G/M, stage machine, corpus/PEFT |
| `goal:ship_substrate` | Dirty-tree → test → PR |
| `goal:glassbox_leg` | LEG split home + any glassbox API |

| Parent | Typical fires |
|--------|----------------|
| `goal:dual_rsi_program` | Dual RSI S/G/M, MCP stale (often), Aliveness |
| `goal:ship_substrate` | Ship gate, PR watch |
| `goal:glassbox_leg` | LEG glassbox UI / API work (not every loop tick) |

---

## Lifecycle (every fire)

```
session_start(intent="<loop> fire")
  → ack_wake_queue
  → ensure parent exists + related to goal:engram_mvp_v1
  → read_concept(helper:rsi_dual_loop_state)   # track_next, open_pr, last_verify, …
  → mint child goal (active, verify_status=pending)
  → act (ONE track / one ship / one PR check / one stale probe / one aliveness write)
  → run typed verify → fill verify packet
  → IF pass: goal_update_status(child, completed) + update dual_loop (last_fire_goal, last_verify, …)
  → IF fail: goal_update_status(child, blocked|abandoned) + scar if repeated + dual_loop blockers
  → session_end(summary MUST include child goal id + verify_status)
```

**HARD constraints**

- **No stage flip** in `helper:rsi_dual_loop_state` (or Gemma stage metrics) without `verify_status=pass`.
- **No ship/PR claim** (PR URL, “shipped”, ready-to-merge) without `verify_status=pass` on the matching gate (`ship_local` / `ship_skip` / `ci_status`).
- **No multi-track** Dual RSI in one fire — one of S, G, or M only.
- **No pack dumps** in chat (paths + short summaries only).
- LEG is **read-only**; it does not run loops or complete goals.
- No auto-merge. MCP auto-restart only if `ENGRAM_ALLOW_MCP_RESTART=1`.

---

## Child goal mint recipe

At fire start, mint a child under the correct parent:

```text
mcp_engram_goal_create(
  goal_id="fire_<loop>_<session_key_or_job>_<unix_ts>",
  parent="goal:dual_rsi_program",   # or goal:ship_substrate | goal:glassbox_leg
  statement="One-line fire intent (track / ship / PR / stale / aliveness)",
  priority="medium",
  affirm="What this fire advances if verify passes",
  deny="What is out of scope or rejected this fire (e.g. multi-track, auto-merge)",
  reconcile="How this fire compounds parent + engram_mvp_v1 continuity"
)
```

**Naming**

- Concept becomes `goal:fire_<loop>_<session_key_or_job>_<unix_ts>` when `goal_id` omits the `goal:` prefix (MCP mints `goal:`).
- Prefer stable, sortable ids: e.g. `fire_dual_rsi_sess1783716771_1720638000`.

**Optional:** `mcp_engram_goal_set_primary` to the child for the duration of the fire so traces auto-link; restore parent/primary at session_end if needed.

---

## Verify packet (required fields)

Embed in goal update note and/or `mcp_engram_remember` as `metric:verify_<fire_id>` (related to the child goal). Schema: `fire_verify_packet_v1`.

| Field | Meaning |
|-------|---------|
| `parent` | Durable parent goal id (e.g. `goal:dual_rsi_program`) |
| `loop` | `dual_rsi` \| `ship_gate` \| `pr_watch` \| `mcp_stale` \| `aliveness` \| … |
| `track` | `S` \| `G` \| `M` \| `null` (null when not Dual RSI) |
| `intent` | One-line fire intent |
| `verify_type` | Typed gate id (table below) |
| `verify_status` | `pending` \| `pass` \| `fail` |
| `verify_evidence` | Paths, test summary, CI URL, metric concept |
| `falsify` | What would reverse this fire |

### Paste template (YAML)

```yaml
parent: goal:dual_rsi_program
loop: dual_rsi
track: S                    # S | G | M | null
intent: "Dual RSI track S — one substrate win"
verify_type: substrate_local
verify_status: pending      # → pass | fail after gate
verify_evidence: ""         # fill on verify
falsify: "Artifact path missing or integrity sample fails"
```

### JSON equivalent

```json
{
  "parent": "goal:dual_rsi_program",
  "loop": "dual_rsi",
  "track": "S",
  "intent": "Dual RSI track S — one substrate win",
  "verify_type": "substrate_local",
  "verify_status": "pending",
  "verify_evidence": "",
  "falsify": "Artifact path missing or integrity sample fails"
}
```

Validate samples offline: `python3 scripts/validate_dual_loop_schema.py` / `python3 scripts/test_glassbox_schemas.py -v`.

---

## Typed gates

| Loop | `verify_type` | Pass means |
|------|---------------|------------|
| Dual RSI **S** | `substrate_local` | Disk artifact and/or targeted test + integrity sample; no pack dump in chat |
| Dual RSI **G** | `gemma_stage` | Stage advanced + metric atom status ok (`peft_metrics` / `eval_gate` / future `gguf_lora`) |
| Dual RSI **M** | `meta_policy` | dual_loop updated with rationale; optional scar |
| Ship | `ship_local` | Tests green + commit + PR URL |
| Ship (clean tree) | `ship_skip` | Explicit skip — complete child; **grey skip**, not green hero |
| PR watch | `ci_status` | Check rollup recorded; all **required** checks SUCCESS for ready-to-merge; else not ready |
| MCP stale | `binary_vs_proc` | FRESH / STALE / OFFLINE atom; restart only if allowed |
| Aliveness | `metrics_atom` | `metric:dual_rsi_aliveness_*` written and related |

### Failure handling (process)

| Failure | Process |
|---------|---------|
| Verify fail | Child → `blocked` (or `abandoned`); scar if repeated; **no** stage flip / ready claim |
| Flaky / partial CI | Not ready-to-merge until all required checks SUCCESS |
| dual_loop missing | Still mint child; scar thin handoff |
| MCP STALE | Set `mcp_restart_required=true`; no auto-kill unless allowed |
| Doom loop (same fail 2×) | Scar + stop fixing that fire |
| Ship skip (clean tree) | Child complete with `ship_skip` |

---

## dual_loop update after verify

On pass or fail, update `helper:rsi_dual_loop_state` (via `mcp_engram_update`) so LEG can mirror:

- `last_fire_goal` — child `goal:fire_*` id
- `last_verify` — `{ type, status, at }`
- Dual RSI: `track_last` / `track_next`, gemma stage fields when G passes
- Ship/PR: `open_pr` when applicable
- Stale: `mcp_restart_required`

Schema extensions: [docs/schemas/dual_loop_state_v1.json](../schemas/dual_loop_state_v1.json).

---

## Loop prompts (canonical bodies)

Paste-ready scheduler / operator prompts live under:

**[docs/skills/loop-prompts/](loop-prompts/)**

| Prompt (v2) | Parent | Default verify_type |
|-------------|--------|---------------------|
| `dual_rsi_v2.md` | `goal:dual_rsi_program` | `substrate_local` \| `gemma_stage` \| `meta_policy` by track |
| `ship_gate_v2.md` | `goal:ship_substrate` | `ship_local` or `ship_skip` |
| `pr_watch_v2.md` | `goal:ship_substrate` | `ci_status` |
| `mcp_stale_v2.md` | dual_rsi or ship parent | `binary_vs_proc` |
| `aliveness_bench_v2.md` | `goal:dual_rsi_program` | `metrics_atom` |

If a prompt file is not yet present, still follow **this skill’s lifecycle + verify packet**; do not run a fire without mint + typed verify. Task 3 lands the prompt bodies.

---

## session_end contract

`mcp_engram_session_end` summary **must** include:

1. Child goal id (`goal:fire_…`)
2. `verify_status` (`pass` \| `fail` \| still `pending` only if abandoned mid-fire with note)
3. `verify_type` + one-line evidence
4. Parent goal and loop id
5. Whether dual_loop was updated

Use `prepare_compression=true` so the next wake rehydrates fire lineage.

---

## Related

- Wake / work / handoff: [engram-wake-up.md](engram-wake-up.md), [engram-working-memory.md](engram-working-memory.md), [engram-session-end.md](engram-session-end.md)
- 8-tool contract: [docs/AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md)
- Design + plan: [2026-07-10-glassbox-rsi-design.md](../superpowers/specs/2026-07-10-glassbox-rsi-design.md), [2026-07-10-glassbox-rsi.md](../superpowers/plans/2026-07-10-glassbox-rsi.md)
- LEG Browser: [docs/LEG_BROWSER.md](../LEG_BROWSER.md)

---

*Glass-box rule: if LEG (or dual_loop) cannot show what the last fire claimed and proved, the fire is incomplete.*
