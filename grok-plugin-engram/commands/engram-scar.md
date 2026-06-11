---
name: engram-scar
description: Scar a dead-end approach — geometric repulsion so future agents do not repeat it
---

**Trigger:** Fix failed, approach ruled out, doom loop detected, user says "don't do that again."

1. Name the failure clearly (e.g. `scar:watch_workspace_at_wake`, `scar:forget_remember_on_design_v1`).
2. Call `mcp_engram_scar` with:
   - `concept` — scar concept name
   - `magnitude` — default 0.15; raise to 0.3–0.5 for load-bearing mistakes
3. Call `mcp_engram_quick_trace` with `decision` = what was ruled out and `why` = what failed.
4. Report scar id to user.

Pair with `/engram-trace` when the fork was already traced. Never scar without recording *why*.