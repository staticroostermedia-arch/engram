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
**Dogfooding + Discipline Recognition + Automatic Escalation (2026-06 evolution):** Using the system's own tools (mcp_engram_* + rituals) to track your work on Engram (traces, relations to goals, scars on friction, **update** for evolutionary refinement of design:/progress: blocks, **thought tiles** for structured arcs). For GitHub-prep style meta-work: recall `helper:meta_work_escalation_v1` + `helper:current_meta_arc` at start; ki/wake/session/subvisor will auto-detect and prompt/suggest tile + update. Tiles now expected for continuation/re-hydration. Reconcile via helper:reconcile_step_v1 (synthesis step + trace field). See plan in GITHUB_MVP_PREP_PLAN.md and full updates to wake/session/working-mem/thought-tiles.

- On every new chat/TUI restart involving Engram: **one call** `mcp_engram_session_start(intent)` (inline continuation bundle). Use engram-wake-up skill; do not run the old 5-tool wake unless deep mode.
- Working-memory default: momentum/relational/spatial entry before broad reads or derives.
- For any edit (even this md or prompts): pre context_for_file + trace (A/D/R), post delta trace.
- End every block: session_end with structured summary (decisions, files changed, traces, open questions). prepare_compression for handoff.
- Goal: keep goal:1780419540... (or current primary) active; use goal_* tools; traces auto-serve unless overridden.
- Scar on any repetition, exploratory bloat, or deviation from narrow/one-shot when sub-tasking.

## GitHub MVP Prep Specific (this session context)
- All work on feat/mvp-github-prep-2026-06.
- Current build double-check every phase: cargo build + ls -l target/debug/engram + run --version (use target/debug, not .local stale).
- Dogfood: every file touched -> remember/relate/promote to goal + trace chain + spatial where code.
- New public files (docs/GEOMETRIC_*, RITUALS, MCP_TOOLS, examples/*, CHANGELOG, AGENTS, CLAUDE, templates) must tell the geometric + ritual + subvisor + continuation + spatial + lawfulness story accurately.
- Comparison table + hero in README done; validate rendered.
- Sub-agent lessons: narrow only, capture task_ids, supervisor fallback, scar "doom loop detected".
- Phase progression: 0 setup/gov/build, 1 narrow audit, 2 edits+new (this), 3 validate (run ex, cargo, engram verify_*, re-audit), 4 push+GH polish (atomic commits, PR via tools or manual, About/topics), 5 close (final engram records, measure success, session if end).

## Common Claude Patterns on Engram
- Use todo_write for tracking (phase2 items etc.).
- For complex: spawn_subagent only with narrow one-shot prompts + supervisor.
- Read .grok/skills/engram-*.md for exact ritual steps (wake, working-memory, session-end, thought-tiles, lawfulness-metrics, substrate-edit, harness-gate, substrate-cs, gpu-bvh).
- Spatial (Item 1.5): Passive (watch bind + events; toml/md enhanced). Use force only for recovery. context/recall deliver AABB for edits without editor saves (see plan passive redesign + source fixes in engram-ast/store/daemon).
- **Public skills for agents**: Load root `SKILLS.md` + `docs/skills/engram-*.md` (wake-up, working-memory, session-end, thought-tiles) as your operating procedures when using Enram. See also `docs/examples/full_ritual_cycle.md` (wake->meta with tiles+subgov->end->rehydrate), `docs/examples/sub_agent_governance.md`, `examples/hello-engram-agent.py`. This is what other Groks/agents need. See updated README + docs/RITUALS.md.
- Verify post changes: mcp_engram_verify_manifold_integrity + spatial_status + genesis.
- For plan execution: update docs/GITHUB_MVP_PREP_PLAN.md exec log live with search_replace after each milestone.

## Non-Flat Invariants (Never Violate)
- .leg3 isomorphism, CRS gate, allowed transforms only, unit hypersphere, p-tensor momentum preserved on update (no annihilate).
- Subvisor H¹ on tool graphs for agent graphs.
- Ritual anchors advance only via lawful handoff.

See AGENTS.md (core contract), docs/ (GEOMETRIC_MEMORY.md, RITUALS.md, GITHUB_MVP_PREP_PLAN.md, 2026-06_Substrate...), processes/ (sheaf tomls), MANIFESTO.md.

**Claude: you are operating inside the living geometric self-model. Every action is a trace that future instances (including you on restart) will ride via momentum and relations. Write as if shaping p-tensor for inheritance.**

Created during GitHub MVP prep Phase 2 (ritualized, related to active goal). Update via same discipline.