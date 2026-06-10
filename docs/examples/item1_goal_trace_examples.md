# Item 1 Examples: Goals + Traces + Payloads

This document contains concrete, copy-paste-ready examples for working with the new Goal Stack system (Item 1).

All examples assume the `engram-goal` skill and the updated trace tools (with `goal_context`) are available.

---

## 1. Example Goal Block (Structured Payload)

**Concept**: `goal:2026-06-01_complete_item1_ego_goal_stack`

**Payload (top of block)**:

```
goal_statement: Complete the full geometric Goal Stack system with Ego binding for the Grok Build TUI agent
parent_goal: 
success_criteria: Working /goal skill exists, goals are queryable via ego resonance, demotion traces are created, and the TUI agent can articulate current intent on wake-up
status: active
priority: high
created_at: 2026-06-01T12:00:00Z
last_updated: 2026-06-01T14:30:00Z
decomposition_notes: Requires data model, /goal skill surface, NREM integration, and ki_hijacker surfacing. Depends on Phase 2 trace infrastructure.
```

**Key Relations** (created during work):
- `serves` from multiple reasoning traces
- `ego_binding` to the ego state at creation time

---

## 2. Example Reasoning Trace Linked to a Goal

Using the new `goal_context` parameter:

```json
{
  "decision_point": "Decided to implement goal_context parameter on trace tools before full MCP goal lifecycle surface",
  "justification": "This allows immediate linking of traces to goals even before dedicated goal tools exist, providing early value and correct data for the /goal skill and ki_hijacker",
  "alternatives_considered": "Wait for full goal tool surface / only use manual relate calls / skip linking for now",
  "falsifiability": "If the parameter adds noticeable friction or is rarely used, we can deprecate it later",
  "related_entities": "mcp.rs, engram-goal/SKILL.md",
  "ritual_context": "engram-working-memory",
  "spatial_context": "crates/engram-server/src/mcp.rs",
  "goal_context": "goal:2026-06-01_complete_item1_ego_goal_stack",
  "prev_trace": "trace:2026-05-31_design_expansion"
}
```

After creation, the system automatically creates the relation:
`trace:xxx → serves → goal:2026-06-01_complete_item1_ego_goal_stack`

---

## 3. Goal Completion / Demotion Trace Example

When closing a goal:

```json
{
  "decision_point": "Marked goal 'Complete Item 1 Ego+Goal Stack' as completed",
  "justification": "All core deliverables for Phase 1 (data model, /goal skill, ritual integrations, first MCP support, examples) are done and geometrically recorded. The TUI agent now has working goal visibility on wake-up.",
  "evidence": "Updated design doc v0.2, engram-goal/SKILL.md created, wake-up + working-memory + session-end integrations, goal_context parameter added to trace tools, examples documented",
  "residual_open_questions": "Deeper NREM ego boosting for active goals, stronger ki_hijacker goal surfacing, full dedicated goal MCP tools",
  "ritual_context": "engram-session-end",
  "goal_context": "goal:2026-06-01_complete_item1_ego_goal_stack"
}
```

Relation created:
`trace:xxx → completes_goal → goal:2026-06-01_complete_item1_ego_goal_stack`

---

## 4. Quick Trace with Goal Context (Low Friction)

Using the ultra-low-friction tool:

```json
{
  "decision": "Chose to implement goal_context on trace tools as first coding step after design",
  "why": "Gives immediate linking capability to the TUI agent and the new /goal skill without waiting for full goal lifecycle MCP surface",
  "goal_context": "goal:2026-06-01_complete_item1_ego_goal_stack"
}
```

---

## 5. Example /goal Skill Usage During a Session

```
> goal status

Active Goals:
1. goal:2026-06-01_complete_item1_ego_goal_stack (high)
   Recent activity: 3 traces today
   Momentum: dv=0.18 (moderate), heat=0.0047
   Status: active

> goal link trace:2026-06-01_goal_context_implementation to goal:2026-06-01_complete_item1_ego_goal_stack

Linked successfully.
```

---

These examples are intended to be used both as documentation and as living reference material that can be turned into actual manifold blocks during development.

They demonstrate the intended flow: **Goals provide intent context → Traces capture execution with goal_context → Demotion traces preserve history → Everything remains queryable and geometrically meaningful**.