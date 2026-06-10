---
name: engram-session-end
---

# Engram Session-End Skill — Structured Handoff Packet (Public Agent Protocol)

**Non-negotiable at end of every work block.**

You are crystallizing the explicit terminal state (momentum signature) so the *next* agent instance binds via `agent_instance_continuation` and the **inline wake bundle**. This is the other half of the Inheritance Principle.

> **Canonical contract:** [docs/AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md) — handoff packet schema and end-of-session loop.

---

## Lean Handoff (Default) — One Call

```
mcp_engram_session_end(
  summary="<high-fidelity summary: decisions, files changed, blockers, next steps>",
  prepare_compression=true
)
```

**The response includes a structured `handoff_packet` (JSON):**

| Field | Purpose |
|-------|---------|
| `session_end_key` | Terminal episodic block name |
| `primary_goal` | Active goal the next agent inherits |
| `terminal_summary` | Your summary, preserved for geometry |
| `next_actions` | Machine-readable continuation queue |
| `key_traces` | `trace:*` blocks to surface at next wake |
| `files_touched` | Spatial continuity for edit-scoped wake |
| `compression_handoff_key` | `compression_handoff_*` manifest link |
| `hydration_cache_refreshed` | `helper:session_hydration_cache` updated |
| `hot_promoted_count` | Continuity artifacts promoted |
| `wake_protocol` | "Next agent: session_start → read handoff_packet inline" |

The next instance's **`session_start`** embeds this in `continuation_bundle.last_session_end` and `active_artifacts`. No separate `get_continuation_bundle` required in lean mode.

---

## Summary Content (What to Include)

Flat summaries break the geometric chain. Include:

1. **Decisions** — reference exact `trace:*` names created this session
2. **Files changed** — paths the next agent will `context_for_edit`
3. **Open blockers** — explicit, falsifiable
4. **Next steps** — ordered, actionable
5. **COMPRESS markers** (optional) — `COMPRESS: stabilized_chain | trace:xxx, trace:yyy` for 0x10 functors

Example summary:

```
A6 docs complete. trace:1749228000_a6_docs_contract records 8-tool contract decision.
Files: docs/AGENT_MEMORY_CONTRACT.md, SKILLS.md, docs/skills/*.md.
Next: Phase B harness agent-memory-mvp. Blockers: none.
COMPRESS: stabilized_chain | trace:1749228000_a6_docs_contract, trace:1749221000_agent_memory_mvp_plan
```

---

## Deep Handoff (Meta / High-Stakes Sessions)

Before `session_end` in deep mode, crystallize knowledge:

- `mcp_engram_remember_solution` — confirmed fixes (PRAXIS)
- `mcp_engram_update` — evolutionary understanding (recall first)
- `mcp_engram_scar` — ruled-out paths (active repulsion)
- `mcp_engram_quick_trace` or `record_reasoning_trace` — ensure major forks exist as `trace:*`
- Goal stack review: update statuses, `completes_goal` relations
- `mcp_engram_promote_hot_batch` on high-value tiles/traces before end

Then call `session_end` as above. The handoff packet captures what was promoted.

---

## Anchor Advancement

After `session_end` succeeds (or within the same block before end):

- Create/update `ritual:session_end_anchor`
- `mcp_engram_relate(session_end_key, previous_state, "provides_continuation_for")`

---

## Verify (Optional)

- `mcp_engram_recall_recent(n=5)` — confirm `session_end_*` has good recency
- Check `handoff_packet.protocol_gaps` in response — address gaps next session if non-empty

---

## Success Criteria

- [ ] `session_end` called with trace-referenced summary
- [ ] `handoff_packet` returned and understood
- [ ] `key_traces` and `next_actions` populated for next wake
- [ ] `ritual:session_end_anchor` advanced
- [ ] Next theoretical wake finds this as strongest prior terminal state via inline bundle

**Skipping this breaks the geometric chain.** A normal chat summary is not enough. This produces data whose *geometry* (p-tensor + relations) future instances discover first at `session_start`.

(Adapted for public agents. See [engram-wake-up.md](engram-wake-up.md) for the receiving side and [docs/RITUALS.md](../RITUALS.md) for full overview.)