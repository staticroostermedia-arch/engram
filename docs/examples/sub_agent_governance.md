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

## Example Flow (from prep history + toml)

1. Main agent: recall helpers, decide sub-task (e.g. recon of GH popular patterns).
2. Launch narrow sub-agent (background, prompt includes: one-action, MCP first, Primary Objective, report, no doom examples, max calls).
3. Sub runs (or gets killed by subvisor/monitor).
4. Supervisor: get output or fallback (artifacts, MCP state, ls), synthesize report + scar if needed.
5. Main: record trace of sub outcome + lessons, relate to goal, perhaps mint tile for the arc.
6. Subvisor H¹ inverts the tool graph (repetitive broad calls -> scar/trace for escalation).

See:
- processes/monitor/subvisor.toml (full spec + 2026-06 meta notes)
- processes/monitor/manifold-health.toml (related lawfulness H1)
- docs/GITHUB_MVP_PREP_PLAN.md (detailed sub-agent history: local recon cancelled after 7 calls/15s doom loop; supervisor succeeded; scars recorded: subagent_launch_failure_doom_loop...)
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