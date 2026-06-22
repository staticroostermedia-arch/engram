---
name: engram-goal
description: Goal stack — set primary, list status, orient intentional self-model
---

**Trigger:** New task arc, shift focus, check what goal traces should link to.

1. If user names a goal → `mcp_engram_goal_set_primary` with `goal:...` id.
2. Else call `mcp_engram_goal_list` or `mcp_engram_goal_status` for orientation.
3. Call `mcp_engram_recall` with `scope: anchors` and query `goal:`.
4. Report primary goal; suggest linking next `/engram-trace` with `goal_context`.

For decomposing work → `mcp_engram_goal_decompose`. For search → `mcp_engram_goal_search`.

When a goal is **completed** or **demoted**:
1. `mcp_engram_goal_update_status(goal=..., status="completed", note=...)` — auto-removes `primary_goal --serves-->` edge
2. `mcp_engram_demote_from_context(concept=...)` — full archival trace + `completes_goal` + cascade condensation cleanup
3. `mcp_engram_session_end(summary=..., prepare_compression=true)` — handoff for next wake
4. **Terminal:** push branch + PR notes (fixes, ACs passed, traces) — e.g. after manage-resume verification on `feat/perfect-context-injection-nvme-bypass`

**TUI `/goal` vs Engram `goal:*`:** Ephemeral TUI session goals clear with `update_goal(completed=true)`. Persistent substrate goals use the MCP steps above. After clear, `session_start` should not surface the completed goal in `primary_goal` or top `suggested_actions`.

All traces and tiles should reference active primary goal when known. Keep serving stack ≤6 — demote stale goals via LEG hygiene or MCP.

**Note on generic TUI /goal:** The core TUI has a built-in `/goal` (simple autonomous session goals via the `update_goal` tool). It may not autocomplete when Engram is the primary MCP/plugin because the session toolset uses Engram's richer geometric goal system instead. This is by design for persistence in the manifold.

- Use **/engram-goal** (this) + Engram MCP goal tools for anything that should live in the geometric substrate — tiles, traces, continuation, subvisors, and multi-session goals.
- Generic TUI `/goal` is fine for ephemeral, non-persistent session notes.

See [docs/PERSONAL_KNOWLEDGE_WIKI.md](../../docs/PERSONAL_KNOWLEDGE_WIKI.md) for multi-chat coexistence and personal knowledge wiki setup.