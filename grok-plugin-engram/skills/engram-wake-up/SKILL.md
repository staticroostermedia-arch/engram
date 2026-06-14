---
name: engram-wake-up
description: >
  First call every Grok session. One-call geometric wake via session_start;
  execute harness queue before any edit or broad read.
metadata:
  short-description: "Wake — session_start + mandatory queue"
---

# Engram Wake-Up

**Trigger:** Start of every session, chat restart, or new task arc.

## Call (mandatory)

```
mcp_engram_session_start(intent="<your objective>")
```

Or: `/engram-wake`

## Execute queue BEFORE edits (non-negotiable)

Run `continuation.harness_injection.suggested_actions` in **priority order** before:
- `context_for_edit`
- broad `Read` / `Grep` / codebase search
- any file edit in `crates/`, `processes/`, `docs/`, `grok-plugin-engram/`

Skipping the queue thins the next wake — the substrate feeds back poor injection when agents skip `session_end`.

## Read the response

1. `continuation.primary_goal` — inherit and name it in your reply.
2. `harness_injection.suggested_actions` — **execute mechanically**.
3. `harness_injection.ego_snapshot` — collective evolution (NREM drift, goal-serving stack).
4. `harness_injection.continuity_playbook.steps` — 12-step breadcrumb path if queue is empty.
5. `harness_injection.trace_chain.head` — chain next `quick_trace` with `prev`.
6. `harness_injection.trusted_tiles` — JIT playbooks to read if relevant.
7. `harness_injection.condensation_hints` — offer `/engram-tile` if present.
8. `readiness.fully_initialized` — must be `true` before heavy recall.

## Then

Activate `engram-working-memory` discipline. Do **not** call `watch_workspace`, `rebuild_bvh`, or `list_concepts` at wake (lean default).

Process spec: `processes/meta/agent_evolution.toml`  
Full protocol: `docs/skills/engram-wake-up.md` · `docs/HARNESS_INJECTION.md`