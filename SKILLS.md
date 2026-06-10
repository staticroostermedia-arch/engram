# Enram Agent Skills & Rituals

**For AI agents (Grok, Claude, custom, etc.) using the Enram MCP server:**

This is the canonical entry point for the published ritual protocols.

## Start Here — 8-Tool Agent Memory Contract

**Read this first:** [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md)

The minimal agent path on large stores (181k+ blocks):

| Wake | Work | Handoff |
|------|------|---------|
| `session_start` (1 call, inline bundle) | `context_for_edit` → `recall` → `quick_trace` → `remember` | `session_end` (handoff packet) |

Plus `get_backend_readiness` and `set_memory_mode` for lean ↔ deep. **Lean by default** — do not call `watch_workspace` or `rebuild_bvh` unless needed.

Load the ritual skills in `docs/skills/` for full protocol detail (all aligned to the contract):

- [docs/skills/README.md](docs/skills/README.md) — Overview, when-to-use, quickstart loop for external agents.
- [docs/skills/engram-wake-up.md](docs/skills/engram-wake-up.md) — 1-call wake (`session_start` inline bundle, lean vs deep).
- [docs/skills/engram-working-memory.md](docs/skills/engram-working-memory.md) — Runtime discipline (`context_for_edit`, anchor-first recall, `quick_trace`, update-preferred).
- [docs/skills/engram-session-end.md](docs/skills/engram-session-end.md) — Structured handoff packet (`session_end` JSON, COMPRESS, anchors).
- [docs/skills/engram-thought-tiles.md](docs/skills/engram-thought-tiles.md) — Structured offload (mandatory for meta, promote_hot for re-hydration).

## Declarative Process Sheaf

The rituals are also defined as first-class processes in `processes/*.toml` (agent:engram.* names, gluing, H¹ handlers for subvisor etc.). Dynamically loaded at `session_start`.

See:
- `processes/ritual/wake-up.toml`, `session-end.toml`, etc.
- `processes/monitor/subvisor.toml` (governance).
- `docs/RITUALS.md` for full overview + Code Edit + sub-agent patterns.

## Examples

- `examples/hello-engram-agent.py` (tiny self-contained demo that loads skills + walks one full loop).
- `examples/mcp_client.py` (MCP usage with rituals).
- `examples/ritual_verify.md` (Code Edit + working-memory).
- `examples/spatial_geosphere_demo.py` (spatial passive + geosphere).
- `docs/examples/sub_agent_governance.md` (subvisor H¹, narrow prompts, escalation, doom-loop prevention).
- `docs/examples/item1_goal_trace_examples.md`

## Full Cycle Demo

See `docs/examples/full_ritual_cycle.md` (or the python equiv) for a complete runnable narrative: wake → heavy meta-work (tiles for plan, traces, spatial pre/post edits, sub-agent call with governance) → session-end (handoff) → simulated next wake (rehydrate from bundle, continue with momentum).

## Usage for Your Agent

1. Connect to engram MCP (see README + [AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md) + MCP_TOOLS_REFERENCE.md).
2. At session start: **one call** `mcp_engram_session_start` (inline continuation bundle) + follow `docs/skills/engram-wake-up.md`.
3. For work: follow `engram-working-memory.md` (`context_for_edit` for edits, `recall(scope=anchors)`, `quick_trace`, `remember`/`update`, tiles for deep meta).
4. At end: `mcp_engram_session_end` (structured handoff packet) per `engram-session-end.md`.

This gives you the same geometric continuation, self-model, and lawfulness we use internally.

**Dogfooding**: The best way to use Enram is to use these rituals *on your own development/meta-work*. The manifold will compound your agent's capability.

See also `docs/AGENT_MEMORY_CONTRACT.md` (8-tool contract), `docs/GEOMETRIC_MEMORY.md`, `docs/RITUALS.md`, `docs/MCP_TOOLS_REFERENCE.md`, `design/agent_memory_mvp_plan.md`, `docs/GITHUB_MVP_PREP_PLAN.md` (execution history + sub-agent governance lessons).

---

*This top-level index makes discovery trivial for agents and humans. All content is in the public repo surface (no .grok/ dependency).*