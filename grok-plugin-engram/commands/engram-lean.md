---
name: engram-lean
description: Return to lean memory mode — protect wake latency after deep work
---

**Trigger:** Deep exploration done; ending meta session; restoring default throttle.

1. Call `mcp_engram_set_memory_mode` with `mode: "lean"`.
2. Call `mcp_engram_get_backend_readiness` — confirm `memory_mode: lean`.
3. Tell user: lean restored; use anchor recall + `context_for_edit` as default.

Pair with `/engram-deep`. Always return to lean before `/engram-session-end` unless user wants deep to persist.