---
name: engram-wake-up
description: >
  First call every Grok session. One-call geometric wake via session_start (slim
  bundle by default); execute suggested_actions before any edit or broad read.
metadata:
  short-description: "Wake — session_start + mandatory queue"
---

# Engram Wake-Up

**Canonical source:** [docs/skills/engram-wake-up.md](../../../docs/skills/engram-wake-up.md) — keep plugin copy aligned when editing.

**Trigger:** Start of every session, chat restart, or new task arc.

## Call (mandatory)

```
mcp_engram_session_start(intent="<your objective>")
```

Or: `/engram-wake`

## Execute queue BEFORE edits (non-negotiable)

Run `continuation.suggested_actions` (slim wake) in **priority order** before:
- `context_for_edit`
- broad `Read` / `Grep` / codebase search
- any file edit in `crates/`, `processes/`, `docs/`, `grok-plugin-engram/`

For full harness (playbook, trusted tiles, lineage): `mcp_engram_get_continuation_bundle`.

Skipping the queue thins the next wake — the substrate feeds back poor injection when agents skip `session_end`.

## Read the response

1. `bundle_tier` — expect `"slim"` by default.
2. `continuation.primary_goal` — inherit and name it in your reply.
3. `continuation.suggested_actions` — **execute mechanically** (top 5 in slim mode).
4. `continuation.ego_snapshot` — NREM step, drift, stability.
5. `continuation.trace_chain_head` — chain next `quick_trace` with `prev`.
6. `continuation.presentation_stratum.previews` — distilled nodes; expand via `get_continuation_bundle` if needed.
7. `readiness.fully_initialized` — must be `true` before heavy recall.

## Then

Activate `engram-working-memory` discipline. Do **not** call `watch_workspace`, `rebuild_bvh`, or `list_concepts` at wake (lean default).

Process spec: `processes/meta/agent_evolution.toml`  
Full protocol: `docs/skills/engram-wake-up.md` · `docs/HARNESS_INJECTION.md`