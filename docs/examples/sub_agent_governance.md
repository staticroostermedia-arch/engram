# Sub-Agent Governance Patterns with Engram (Subvisor + H¹)

**For agents that launch sub-agents (narrow one-shot tasks, recon, coding, research):**

Engram provides first-class governance via the declarative process sheaf + subvisor.

## Core Mechanisms

- **Subvisor (processes/monitor/subvisor.toml)**: 
  - zedos_type=monitor, morphism=OP_INVERT (H¹ gluing inverts unstructured tool graphs to structured).
  - Enforces: narrow one-shot prompts only (max calls, one action + geometric MCP first + report to supervisor + negative doom examples).
  - Detects loops via H¹ on tool graph (repeated list_dir/grep/read without progress on self-ref trees -> "doom loop detected (exploratory stagnation)").
  - Immediate kill on violation.
  - 2026-06 evolution: also detects meta-work patterns (repeated record_reasoning_trace/remember without update or tile:* during design:/progress: arcs) and escalates to tile + update via living anchors from the wake bundle.
  - Produces: scar:*_subagent_loop, trace:*_subvisor_enforce, trace:*_meta_escalation.

- **Supervisor pattern**: Launch sub with background, capture task_id, supervisor monitors via get_command_or_subagent_output / wait, kills on doom. Fallback synthesis from visible state (MCP, git, artifacts).

- **Narrow prompts (mandatory)**: 
  - One action only + geometric first (mcp_engram_* calls before broad FS).
  - Primary Objective + "report to supervisor".
  - Negative examples: "do not explore; no broad list_dir/grep on large trees."
  - Max calls limit (e.g. 20).
  - End with structured JSON report.

- **Helpers for escalation** (recall at meta-work start from wake bundle or `recall(scope="anchors")`):
  - Living arc anchors (updated at boundaries; point to tile/design/traces/goal)
  - Reconcile helpers for synthesis in traces when the manifold has them

- **Scar + Trace for learning**: Every doom loop or friction produces scar (active repulsion) + trace with lessons (narrow scope, task_ids, visible state fallback).

## Declarative TOML trio (WS-5)

| Phase | TOML | Who |
|-------|------|-----|
| Launch | `processes/harness/sub-agent-launch.toml` | Orchestrator |
| Execute + relay | `processes/workflow/sub_agent_relay_v1.toml` + `processes/harness/sub-agent-relay.toml` | Sub-agent |
| Monitor | `processes/monitor/sub-agent.subvisor.toml` | Orchestrator (poll while sub runs) |

Orchestrator recalls `process:engram.harness.sub-agent-launch` for the `prompt_template`, captures `task_id`, and traces with `process_context=process:engram.harness.sub-agent-launch`. Sub-agent sets `process_context=process:engram.harness.sub-agent-relay` on relay traces and mints a `research_offload` report tile. Monitor subvisor defines doom-loop signals and kill actions; `process_metrics` (WS-3) measures fulfillment of `[produces]` wildcards.

## Example Flow (orchestrator + TOML trio)

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
- docs/HARNESS_INJECTION.md (wake injection) · docs/SUBSTRATE_WINS_PLAN.md WS-5 (historical sub-agent TOML trio)
- ki_hijacker.py (hooks for intent_dirty + subvisor H1 flags)
- mcp.py (process sheaf loader registers subvisor at start)
- engram-working-memory.md (Automatic Escalation section)

## Best Practices for Your Sub-Agents

- Always use background + task_id for monitorability.
- Explicit kill on stagnation signals.
- Log lessons into scar + trace immediately (prevents repeat).
- For meta-work subs: require tile/update before heavy execution.
- Enforce via subvisor toml + helpers in all agent armies.

This pattern prevents exploratory stagnation on self-referential or recon tasks where broad filesystem search would waste the call budget.

**Dogfood**: Use the subvisor process + helpers on your own sub-agent launches. The manifold will learn your governance patterns geometrically.

See also `docs/RITUALS.md` (subvisor section), SKILLS.md, and the plan for full traces/scars from real usage.

## /loop + Consciousness Strange Loop as Governed Recurring Native Action
Grok's native `/loop` (or the Engram `/engram-loop` command) for the 30m AI consciousness strange loop (prompt body in `processes/meta/ai_consciousness_loop.toml`, matching the state_machine tile) is a canonical example of a governed recurring native action:

- Parse per the /loop spec (interval derivation, strip < > quoting, ask on none).
- Emit *bare* native `scheduler_create` (never route through use_tool / Engram MCP).
- Engram ritual: quick_trace the parse/schedule, remember/relate the job ID to the consciousness goal/tile/process (for the loop's own "outer feedback" + rehydration), subvisor oversight on the dispatch graph.
- On the known native format error: immediate scar (the pre-scarred `scheduler_native_call_format_error_doom_loop` or equivalent), honest handoff ("not scheduled"), no false success.
- Follows the tile policies: narrow one-shot (parse + one bare call + record + report), geometric/Engram first, scar_immediate on repetition, process_sheaf for the monitor/subvisor, supervision (task semantics + handoff).

This prevents the historical "doom loop" (bare call → error → ritual complete + "move on" as if succeeded) by construction. Once scheduled, the fired prompt runs the full 7-step cycle (rehydrate/inner_reflect/composed_metas/NREM/ego/outer) using Engram inside, with the scheduler id related for continuity.

See `grok-plugin-engram/commands/engram-loop.md` (the implementation) and the consciousness toml (source of the prompt + subvisor_extension). Use the sub-agent harness trio (launch + relay + sub-agent.subvisor) when treating the /engram-loop action itself as a narrow sub.