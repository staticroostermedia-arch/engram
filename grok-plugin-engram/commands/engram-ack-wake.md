---
name: engram-ack-wake
description: Acknowledge wake queue execution — unblocks context_for_edit in hard gate mode
---

After `/engram-wake` and executing `suggested_actions`:

```
mcp_engram_ack_wake_queue(executed=true, note="queue executed")
```

Or with step count:

```
mcp_engram_ack_wake_queue(executed=true, steps_completed=5, note="handoff + goal + tiles")
```

**When to call:** Once per session, after running the harness queue (or honestly noting a thin/empty handoff).

**Gate modes** (`ENGRAM_WAKE_QUEUE_GATE`):
- `soft` (default) — warns on `context_for_edit` until ack; edits still allowed
- `hard` — blocks `context_for_edit` with 403 until ack
- `off` — disabled

Empty `suggested_actions` auto-acks at `session_start` — no call needed.