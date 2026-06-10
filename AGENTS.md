# AGENTS.md — Engram Editing Guidelines for AI Agents

**Primary Objective**: `goal:engram_mvp_v1` (harness continuity; operationalize "Against Flat Knowledge" via geometric sheaf replacing flat weights/text/vec DBs).

This file + CLAUDE.md + docs/ + .grok/skills/ + processes/*.toml + GITHUB_MVP_PREP_PLAN.md define the contract for any agent editing the Engram substrate or its representation.

## Start Here — 8-Tool Agent Memory Contract (Lean Default)

**Read first:** [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md)

| Phase | Tools |
|-------|-------|
| **Wake** | `mcp_engram_session_start(intent)` — one call, inline continuation bundle + backend readiness |
| **Edit prep** | `mcp_engram_context_for_edit(absolute_path)` — file-scoped spatial + memory (replaces mandatory `watch_workspace` at wake) |
| **Read** | `mcp_engram_recall(query, scope="anchors")` — goals/traces/rituals before episodic noise |
| **Decide / write** | `mcp_engram_quick_trace` at forks; `mcp_engram_remember` for new concepts only (recall first; use `update` if match > 0.85) |
| **Handoff** | `mcp_engram_session_end(summary, prepare_compression=true)` — structured packet for next wake |
| **Probe / mode** | `mcp_engram_get_backend_readiness`; `mcp_engram_set_memory_mode("lean"|"deep")` |

**Lean by default.** Do **not** call `watch_workspace`, `rebuild_bvh`, `summarize`, or `query_with_momentum` at wake unless deep mode or a specific task requires it. See [docs/MCP_TOOLS_REFERENCE.md](docs/MCP_TOOLS_REFERENCE.md) for Essential / Power / Lean-avoid tiers.

**MCP config:** `ENGRAM_PROFILE=agent` via `scripts/engram-grok` (two env vars only). See [`integrations/README.md`](integrations/README.md) for Grok, Cursor, Claude, Antigravity, Codex.

Load public skills: [SKILLS.md](SKILLS.md) + `docs/skills/engram-wake-up.md`, `engram-working-memory.md`, `engram-session-end.md`.

---

## Deep Mode — Full Rituals & Working-Memory Discipline

Escalate with `set_memory_mode("deep")` when lean bundle is insufficient (meta arcs, lawfulness audits, relation-graph navigation).

**Dogfooding note:** "Dogfood" means using Engram's MCP tools and rituals to record and evolve the *current task* as living geometry.

- **Deep geometric entry** (cost-aware):
  1. `mcp_engram_query_with_momentum` (directional/trending only at entry or long-horizon).
  2. `mcp_engram_search_by_relation(seed, direction="both")` + `visualize`.
  3. For code: `mcp_engram_context_for_edit` or `mcp_engram_context_for_file` (absolute path); `mcp_engram_recall_in_file` (AABB). Add `watch_workspace` only if passive daemon ingest is needed.
  4. Anchor first: ritual:*, goal:*, trace:*, process:engram.* .
- **Recall before derive**. `mcp_engram_update` preferred (Lyapunov drift); never `forget` + `remember`.
- **Every significant decision/fork/edit**: `mcp_engram_record_reasoning_trace` (or `quick_trace`) with `decision_point`, `justification`, `alternatives_considered`, `falsifiability`, `spatial_context`, `goal_context`, `prev_trace` (chain), `related_entities`.
- **Code Edit Ritual v1** (mandatory for crates/, .grok/skills/, mcp.rs, store.rs, daemon, processes/, integrations/):
  - Pre: `context_for_edit` + recall_in_file + intent trace.
  - Edit (search_replace/write after read).
  - Post: re-context, delta trace (chained), relate edit to goal/arc, `remember_solution` or `scar`.
- **Scar immediately** on ruled-out approaches, friction, dead-ends, repetition, doom loops.
- **Goals**: Use `engram-goal` skill / mcp goal_* ; set primary; decompose; status updates with traces.
- **Spatial (Item 1.5)**: Lean path uses `context_for_edit` per file. Deep may add `watch_workspace` once per project. `force_spatial_ingest` for recovery only.
- **Sub-agent governance**: Narrow one-shot prompts only (~20 calls max). Kill on "doom loop detected". See `processes/monitor.subvisor.toml`.
- **Meta-work escalation**: For design:/progress: arcs, recall `helper:meta_work_escalation_v1` + `helper:current_meta_arc`; mint tiles at boundaries.

## Representing Engram (GitHub / Public)

When editing public surface (README, docs/, .github/, examples/, Cargo, CHANGELOG, SECURITY, AGENTS/CLAUDE):
- Lead with **8-tool contract** + Grok Build story ([docs/GROK_BUILD_MEMORY.md](docs/GROK_BUILD_MEMORY.md)).
- Highlight uniques: geometric/non-flat (q/p/CRS/Merkle/sheaf/H¹/VSA/spatial/continuation/lawfulness/rituals/subvisor/process tomls) vs flat vector/RAG.
- Emulate popular (badges, quickstarts, comparison tables, examples/cookbooks, .github templates with checklists, AGENTS/CLAUDE, CHANGELOG, CI matrix).
- Dogfood: every edit records trace/remember/relate/update to active goal; spatial where applicable.
- Current build: `cargo build && target/debug/engram --version` (use target/debug, not stale ~/.local/bin).
- Link: AGENT_MEMORY_CONTRACT.md, GEOMETRIC_MEMORY.md, RITUALS.md, MCP_TOOLS_REFERENCE.md, GROK_BUILD_MEMORY.md.

## Process Sheaf & Subvisor

Declarative first-class: processes/*.toml loaded at `session_start`. Subvisor uses OP_INVERT/H¹ for tool-graph oversight.

See processes/, mcp.rs, docs/RITUALS.md.

## Lawfulness & Verification (Deep Mode)

- `mcp_engram_verify_manifold_integrity` (min_crs=0.74, sample), `verify_block_lawfulness`, `genesis`, `spatial_status`.
- CRS >=0.74 grounded; pinned for axioms.

## Other

- Use todo_write for complex multi-step.
- For MCP in harnesses: ALWAYS `search_tool` first (exact schema), then `use_tool` (qualified name e.g. engram__mcp_engram_...).
- Hot promotion for high-value artifacts: `promote_hot`.
- **Do not delete MCP tools** — tier in documentation only; 62 tools remain for power users.

See also:
- **Public agent rituals**: [SKILLS.md](SKILLS.md) + `docs/skills/`
- **Grok Build pitch**: [docs/GROK_BUILD_MEMORY.md](docs/GROK_BUILD_MEMORY.md)
- **8-tool contract**: [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md)
- Full cycle: `docs/examples/full_ritual_cycle.md`, `examples/hello-engram-agent.py`
- Internal TUI: `.grok/skills/engram-*.md`
- docs/RITUALS.md, docs/GITHUB_MVP_PREP_PLAN.md, docs/MCP_TOOLS_REFERENCE.md, docs/GEOMETRIC_MEMORY.md
- MANIFESTO.md, CONTRIBUTING.md, .github/PULL_REQUEST_TEMPLATE.md

**Violations of this contract are scarred immediately.** The manifold will deflect future attempts.