# CLAUDE.md — Claude-Specific Guidance for Engram (Geometric Memory Substrate)

Claude (and similar): follow AGENTS.md exactly. This is a supplement for common Claude workflows + MCP usage.

## Start Here — 8-Tool Lean Contract

**Read first:** [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md)

Lean default: `session_start` → `context_for_edit` → `recall(scope=anchors)` → `quick_trace`/`remember` → `session_end`. Do **not** call `watch_workspace` or `rebuild_bvh` at wake. Use safe MCP env — see `integrations/grok-build/mcp.json`. Grok Build pitch: [docs/GROK_BUILD_MEMORY.md](docs/GROK_BUILD_MEMORY.md).

## MCP Tool Usage (Critical)
- **Always**: Call `search_tool` (query e.g. "engram record_reasoning_trace" or tool name) **first** to retrieve live input_schema.
- Then `use_tool` with exact `tool_name` (qualified e.g. "engram__mcp_engram_record_reasoning_trace") and `tool_input` matching schema precisely.
- Never guess parameter names or call without schema. This is enforced (prevents transport issues).
- Examples: context_for_file (path absolute), record_reasoning_trace (decision_point + justification required; goal_context for auto primary link, prev_trace for chain), remember/relate/scar/update, watch_workspace, verify_*, spatial_*, goal_*, session_start/end.

## Ritual Enforcement in Claude Sessions

When working on Engram itself, use the system's own MCP tools to record traces, relate work to active goals, scar friction, and **`update`** design/progress blocks. **Thought tiles** help structured arcs and session re-hydration. See `docs/HARNESS_INJECTION.md`, `docs/DEFORMATION_PLAYBOOKS.md`, and `docs/RITUALS.md`.

- On every new chat/TUI restart involving Engram: **one call** `mcp_engram_session_start(intent)` (inline continuation bundle). Use the wake-up skill; do not run the old 5-tool wake unless deep mode. Slim wake may include `rehydration_manifest` + soft `rehydrate_suggested` sentinel nudge (never blocking).
- Working-memory default: momentum/relational/spatial entry before broad reads or derives.
- For substrate edits: pre `context_for_file` + trace (A/D/R), post delta trace (Code Edit Ritual v1).
- End substantive work blocks: `session_end` with structured summary (decisions, files changed, open questions). Use `prepare_compression` for handoff when appropriate.
- Keep your active project goal current via `goal_*` tools; scar on repetition or exploratory bloat.

## Common Claude Patterns on Engram
- Use todo_write for tracking (phase2 items etc.).
- For complex: spawn_subagent only with narrow one-shot prompts + supervisor.
- **Canonical skills:** `docs/skills/engram-*.md` (wake-up, working-memory, session-end, thought-tiles, lawfulness-metrics, substrate-edit, harness-gate, substrate-cs, gpu-bvh). Optional Grok TUI overlay: `.grok/skills/engram-*.md`.
- Spatial (Item 1.5): Passive (watch bind + events; toml/md enhanced). Use force only for recovery. context/recall deliver AABB for edits without editor saves (see plan passive redesign + source fixes in engram-ast/store/daemon).
- **Public skills for agents**: Load root `SKILLS.md` + `docs/skills/engram-*.md` (wake-up, working-memory, session-end, thought-tiles) as your operating procedures when using Engram. See also `docs/examples/full_ritual_cycle.md` (wake->meta with tiles+subgov->end->rehydrate), `docs/examples/sub_agent_governance.md`, `examples/hello-engram-agent.py`. This is what other Groks/agents need. See updated README + docs/RITUALS.md.
- Verify post changes: mcp_engram_verify_manifold_integrity + spatial_status + genesis.
- For harness milestones: update docs/HARNESS_INJECTION.md; historical program record in docs/SUBSTRATE_WINS_PLAN.md.

## Non-Flat Invariants (Never Violate)
- .leg3 isomorphism, CRS gate, allowed transforms only, unit hypersphere, p-tensor momentum preserved on update (no annihilate).
- Subvisor H¹ on tool graphs for agent graphs.
- Ritual anchors advance only via lawful handoff.

See AGENTS.md (core contract), docs/ (GEOMETRIC_MEMORY.md, RITUALS.md, SUBSTRATE_WINS_PLAN.md, HARNESS_INJECTION.md), processes/ (sheaf tomls), MANIFESTO.md.

**Claude: you are operating inside the living geometric self-model. Every action is a trace that future instances (including you on restart) will ride via momentum and relations. Write as if shaping p-tensor for inheritance.**