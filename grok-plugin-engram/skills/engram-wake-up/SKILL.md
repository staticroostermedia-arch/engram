---
name: engram-wake-up
description: >
  First call every Grok session. One-call geometric wake via session_start;
  read continuation_bundle and state primary goal inheritance.
metadata:
  short-description: "Wake — session_start + continuation"
---

# Engram Wake-Up

**Trigger:** Start of every session, chat restart, or new task arc.

## Call (mandatory)

```
mcp_engram_session_start(intent="<your objective>")
```

Or: `/engram-wake`

## Read the response

1. `continuation.primary_goal` — inherit and name it in your reply.
2. `continuation.structured_handoff` — read `helper:session_handoff_latest` if present.
3. `readiness.fully_initialized` — must be `true` before heavy recall.

## Then

Activate `engram-working-memory` discipline. Do **not** call `watch_workspace`, `rebuild_bvh`, or `list_concepts` at wake (lean default).

Full protocol: `docs/skills/engram-wake-up.md`