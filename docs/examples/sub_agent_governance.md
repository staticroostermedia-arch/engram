# Sub-Agent Governance Patterns with Enram (Subvisor + H¹)

**For agents that launch sub-agents (narrow one-shot tasks, recon, coding, research):**

Enram provides first-class governance via the declarative process sheaf + subvisor.

## Core Mechanisms

- **Subvisor (processes/monitor/subvisor.toml)**: 
  - zedos_type=monitor, morphism=OP_INVERT (H¹ gluing inverts unstructured tool graphs to structured).
  - Enforces: narrow one-shot prompts only (max calls, one action + geometric MCP first + report to supervisor + negative doom examples).
  - Detects loops via H¹ on tool graph (repeated list_dir/grep/read without progress on self-ref trees -> "doom loop detected (exploratory stagnation)").
  - Immediate kill on violation.
  - 2026-06 evolution: also detects meta-work patterns (repeated record_reasoning_trace/remember without update or tile:* during design:/progress: arcs) and escalates to tile + update via helper:meta_work_escalation_v1 + helper:current_meta_arc.
  - Produces: scar:*_subagent_loop, trace:*_subvisor_enforce, trace:*_meta_escalation.

- **Supervisor pattern**: Launch sub with background, capture task_id, supervisor monitors via get_command_or_subagent_output / wait, kills on doom. Fallback synthesis from visible state (MCP, git, artifacts).

- **Narrow prompts (mandatory)**: 
  - One action only + geometric first (mcp_engram_* calls before broad FS).
  - Primary Objective + "report to supervisor".
  - Negative examples: "do not explore; no broad list_dir/grep on large trees."
  - Max calls limit (e.g. 20).
  - End with structured JSON report.

- **Helpers for escalation** (recall at meta start):
  - helper:meta_work_escalation_v1
  - helper:current_meta_arc (living anchor updated at boundaries, points to tile/design/traces/goal)
  - helper:reconcile_step_v1 (for synthesis in traces)

- **Scar + Trace for learning**: Every doom loop or friction produces scar (active repulsion) + trace with lessons (narrow scope, task_ids, visible state fallback).

## Declarative TOML trio (WS-5)

| Phase | TOML | Who |
|-------|------|-----|
| Launch | `processes/harness/sub-agent-launch.toml` | Orchestrator |
| Execute + relay | `processes/workflow/sub_agent_relay_v1.toml` + `processes/harness/sub-agent-relay.toml` | Sub-agent |
| Monitor | `processes/monitor/sub-agent.subvisor.toml` | Orchestrator (poll while sub runs) |

Orchestrator recalls `process:engram.harness.sub-agent-launch` for the `prompt_template`, captures `task_id`, and traces with `process_context=process:engram.harness.sub-agent-launch`. Sub-agent sets `process_context=process:engram.harness.sub-agent-relay` on relay traces and mints a `research_offload` report tile. Monitor subvisor defines doom-loop signals and kill actions; `process_metrics` (WS-3) measures fulfillment of `[produces]` wildcards.

## Example Flow (from prep history + toml)

1. Main agent: recall helpers + `process:engram.harness.sub-agent-launch`, decide sub-task (e.g. recon of GH popular patterns).
2. `quick_trace` launch fork with `task_id`; spawn narrow sub-agent (background, prompt from `[launch].prompt_template`).
3. Poll sub output; `process:engram.monitor.sub-agent` subvisor watches for doom loops.
4. Sub completes relay workflow: `trace:*_subagent_relay` + `tile:*_subagent_report` related to goal.
5. Orchestrator: read report tile, `quick_trace` synthesis, scar if relay missing.
6. Subvisor H¹ inverts the tool graph (repetitive broad calls -> scar/trace for escalation).

See:
- processes/harness/sub-agent-launch.toml
- processes/harness/sub-agent-relay.toml
- processes/workflow/sub_agent_relay_v1.toml
- processes/monitor/sub-agent.subvisor.toml
- processes/monitor/subvisor.toml (base subvisor + 2026-06 meta notes)
- processes/monitor/manifold-health.toml (related lawfulness H1)
- docs/SUBSTRATE_WINS_PLAN.md WS-5 (sub-agent TOML trio + governance)
- ki_hijacker.py (hooks for intent_dirty + subvisor H1 flags)
- mcp.py (process sheaf loader registers subvisor at start)
- engram-working-memory.md (Automatic Escalation section)

## Best Practices for Your Sub-Agents

- Always use background + task_id for monitorability.
- Explicit kill on stagnation signals.
- Log lessons into scar + trace immediately (prevents repeat).
- For meta-work subs: require tile/update before heavy execution.
- Enforce via subvisor toml + helpers in all agent armies.

This pattern was critical for reliable sub-agent use during the GitHub MVP prep (prevented exploratory stagnation on self-referential tasks).

**Dogfood**: Use the subvisor process + helpers on your own sub-agent launches. The manifold will learn your governance patterns geometrically.

See also `docs/RITUALS.md` (subvisor section), SKILLS.md, and the plan for full traces/scars from real usage.