---
name: engram-ack-edit-arc
description: Clear edit-arc debt on read-only repeat context_for_edit — prefer update on __arc after real edits
---

When `edit_arc_debt` is pending and you need **read-only** recon on the same file (no substantive edits):

```
mcp_engram_ack_edit_arc(skip=true, note="read-only recon — no source changes")
```

Or clear specific concepts:

```
mcp_engram_ack_edit_arc(concepts=["store__fn__context_for_edit"], skip=true, note="read-only pass")
```

**When to call:** Repeat `context_for_edit` on a path you already edited this session, but you are only reading context — not writing new arc narrative.

**Preferred after real edits:** `mcp_engram_update` on `{concept}__arc` using args from `post_edit_palette` — do not skip the arc update when you changed source.

**Gate modes** (`ENGRAM_EDIT_ARC_GATE`):
- `soft` (**default** with `ENGRAM_PROFILE=agent`) — warns until arc cleared or acked
- `hard` — blocks repeat `context_for_edit` on same locus until `update(__arc)` or ack
- `off` — disabled