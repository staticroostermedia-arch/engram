# Item 1: Ego as Intentional Actor — Goal Stack Design

**Status**: Ideal Implementation Design v0.2 (Expanded with data model, commands, demotion traces, payloads, vocabulary, and roadmap sketch)  
**Parent**: `design:ego_intent_logophysics_memory_palace_framework` + Item 1 of the 4-Item Sequential Roadmap  
**Date**: Current  
**Scope**: Local Grok Build TUI agent continuity substrate. Must be generalizable to any agent using the Engram memory system.

---

## 1. Executive Vision for Item 1

The current `ego.leg3` is a powerful but **passive** mechanism — a reconciled narrative centroid that provides resonance-based gating and scoring. 

**Goal of Item 1**: Evolve the ego from a reflective lens into the **geometric root of directed agency**.

The agent should not only know "who I am" (current ego state), but have a live, queryable, continuable representation of:
- What I am actively trying to accomplish
- How those goals decompose
- What work is serving which goals
- The execution state and history of those goals

This must be deeply geometric (tied to the ego q-vector + Logenergetics) so that the rest of the memory system (recall, ki_hijacker, wake-up, consolidation, BVH) can naturally prefer goal-relevant material using the existing physics.

---

## 2. Core Data Model (Detailed)

### 2.1 Goal Block

A Goal is a first-class `.leg3` block.

**ZEDOS Tag Recommendation (Phase 1 of Item 1)**: Start with `ZEDOS_OPERATIONAL` (0x52) for simplicity and compatibility. A dedicated tag can be introduced later if volume and distinct query patterns justify it.

**Recommended concept naming**:
- `goal:<unix_ts>_<kebab-slug>`
- Namespaced example: `goal:engram__complete_item1_ego_goal_stack`

**Payload Structure** (structured header + rich ProvLog)

Structured section (JSON-like or key-value at the top of the payload for easy parsing):

```json
{
  "goal_statement": "Complete the full geometric Goal Stack system with Ego binding",
  "parent_goal": null,
  "success_criteria": "Working /goal skill exists, goals are queryable via ego resonance, demotion traces are created, and the TUI agent can articulate current intent on wake-up",
  "status": "active",
  "priority": "high",
  "created_at": "2026-...",
  "last_updated": "2026-...",
  "decomposition_notes": "Requires data model, /goal skill, NREM integration, and ki_hijacker surfacing"
}
```

**Dynamic / Physics Fields** (leveraging existing .leg structure):

- `q` vector: Resonance fingerprint of the goal.
- `p` vector: Updated on significant events (new linked traces, status changes) to represent momentum toward or away from the goal.
- Logenergetics:
  - `dv`: High when the goal is actively being worked (many recent linked traces with momentum).
  - Lyapunov alphas + `h_in/h_out`: Stability vs. convergence signal for the goal.
  - `heat_dissipated`: Cumulative effort invested in the goal.

### 2.2 Goal Stack & Relationships (Full Vocabulary)

Core relations (created via `mcp_engram_relate` or wrapped by the /goal skill):

- `decomposes_into` — parent goal → child subgoal
- `serves` — execution artifact (trace, tile, code change) → goal
- `next_in_goal_stack` / `prev_in_goal_stack` — ordering of the active stack
- `ego_binding` — explicit relation from goal to the ego state at the time it was made active (or to `ego.leg3` directly)
- `completes_goal` / `demotes_goal` — completion or demotion trace → goal
- `was_active_during` — goal ↔ intent epoch marker

**Active Goal Stack Definition**:
The ordered set of goals where:
- `status` is `active`
- They have the strongest combined geometric resonance (q) + relational proximity to the current live `ego_q`

### 2.3 Demotion & Serialization of Accomplished Goals (Detailed)

**Requirement (from user)**: Accomplished goals must be demoted (not deleted) and serialized in a trace-like way so they remain usable as historical markers for back-tracing intent and reasoning. This pattern must be generalizable to any agent using Engram for long-running work.

**Demotion Process**:

1. Agent (via /goal skill) calls `goal update <id> status:completed` or `status:demoted` with a note.
2. The Goal block is updated in place (`status` field changed, `last_updated` advanced).
3. A dedicated **Goal Completion / Demotion Trace** is created using the existing `mcp_engram_record_reasoning_trace` (or a thin wrapper).

**Structure of a Goal Demotion/Completion Trace** (modeled directly on reasoning traces):

```
TRACE (type: goal_completion):
decision: Marked goal "Complete Item 1 Ego+Goal Stack" as completed
why: All core deliverables done, design recorded, first implementation phase sketched, and the TUI agent now has working goal visibility on wake-up
evidence: [links to key traces, the design doc, skill updates]
residual_open_questions: [any remaining polish or integration with Item 2 tiles]
demotion_note: "This goal is now historical. Kept for back-tracing how the intentional self-model was built."
goal: goal:12345_item1_ego_goal_stack
```

Relation created: `completes_goal` or `demotes_goal`

**Long-term Behavior**:
- Demoted goals can gradually have their CRS lowered by normal thermodynamic processes.
- They remain fully queryable and participate in back-tracing ("what goals were active when this decision was made?").
- They become excellent candidates for the middle consolidation layer (Item 3).

This mechanism is explicitly intended to be a general primitive for any agent doing serious, multi-session work on Engram.

---

## 3. Binding to the Ego Vector (Refined)

- Active goals receive a measurable resonance boost with the current `ego_q`.
- On NREM consolidation, active goals (especially recently updated or high-priority) contribute a small weighted influence to the ego centroid (extending the existing `design:ego_trace_bridging_sketch` idea).
- New traces/tiles created while goals are active are encouraged to carry `goal_context`, which can influence their initial resonance profile.

**Intent Epochs**: When the top of the active goal stack changes significantly, a lightweight epoch marker block is created. This gives future agents clear "chapter headings" in the intentional history.

---

## 4. Role of the Explicit `/goal` Skill (Detailed Commands & Behavior)

The `/goal` Skill is the primary interface for the TUI agent.

### Core Commands (Proposed Surface)

**Lifecycle**
- `goal create "<statement>" [parent: <goal>] [priority: high|medium|low]`
- `goal decompose <goal> into "<sub1>", "<sub2>", ...`
- `goal update <goal> status:completed|demoted|blocked|active [note: "..."]`
- `goal pin <goal>` (forces CRS=1.0 on the goal block)
- `goal unpin <goal>` (reverts to normal thermodynamic behavior)

**Execution Linking**
- `goal link <trace_or_tile> to <goal>`
- During working-memory discipline, the skill can offer: "Link this decision to active goal?"

**Visibility**
- `goal status` — Current active stack with momentum signals (recent traces, dv, heat).
- `goal show <goal>` — Full details + linked execution artifacts + history.
- `goal history <goal>` — All completion/demotion traces and back-trace markers.

**Integration Hooks** (called by other rituals)
- The skill exposes helpers that `engram-wake-up`, `engram-session-end`, and `engram-working-memory` can call.

**UX Notes for TUI Agent**
- The skill should provide copy-paste friendly templates and scaffolding so linking work to goals feels lightweight.
- `goal status` should be one of the high-visibility things that can appear in the ki_hijacker context on restart.

---

## 5. Example Payloads & Relation Vocabulary

### Example Goal Block Payload (abbreviated)

```
goal_statement: Complete Item 1 Ego + Goal Stack design and first implementation phase
status: active
priority: high
success_criteria: ...
decomposition_notes: ...

Linked execution (via relations):
- trace:phase2_... (serves)
- design:item1_ego_goal_stack_v1 (serves)
```

### Relation Vocabulary (Core Set)

- `decomposes_into`
- `serves`
- `next_in_goal_stack` / `prev_in_goal_stack`
- `ego_binding`
- `completes_goal` / `demotes_goal`
- `was_active_during` (to intent epochs)

---

## 6. Draft SKILL.md Content for /goal (Skeleton)

```markdown
---
name: engram-goal
description: >
  Manage the agent's explicit goal stack as a first-class, geometrically bound
  extension of the living ego. Provides lifecycle, linking, visibility, and
  integration with wake-up / working-memory / session-end rituals.
when-to-use: >
  Any time the agent needs to declare, decompose, track, or close goals that
  should influence its self-model and future recall behavior.
---

# Engram Goal Skill — Intent as Geometry

## Core Protocol

1. Create and decompose goals with clear statements and success criteria.
2. Link execution artifacts (traces, tiles) to active goals.
3. Update status, including proper demotion with serialized completion traces.
4. Use `goal status` as a primary visibility tool on wake-up and in session-end.

## Commands

(See detailed list in the main design document)

## Integration Points

- Called by engram-wake-up to surface current intent.
- Used inside engram-working-memory to encourage goal linking.
- Required step in engram-session-end for major work.

Success Criteria:
- The TUI agent can reliably articulate its current goal stack on restart.
- Accomplished goals leave clean, queryable historical traces.
```

---

## 7. Implementation Roadmap Sketch (First Executable Phase)

### Phase 1 of Item 1 (Minimal Viable Intentional Ego)

**Goal**: Make the TUI agent able to create goals, link work to them, see the current stack on wake-up, and properly demote goals with traces.

**Scope (narrow & achievable)**:
1. Data model + relation vocabulary finalized and documented.
2. Small additions to `mcp_engram_record_reasoning_trace` (optional `goal_context` parameter).
3. New MCP surface or thin wrappers for goal lifecycle (can start as a few new remember/relate patterns + a helper skill).
4. Updates to the three core ritual skills to mention and lightly scaffold goal linking.
5. One visible win: `goal status` output appears in ki_hijacker or wake-up for the TUI agent.
6. Demotion trace pattern implemented and tested (using existing trace tool).
7. Full geometric recording of the work using the new mechanisms.

**Estimated Surface**: Moderate — mostly skill updates, light MCP helpers, and design + examples. Heavy reuse of Phase 2 trace infrastructure.

**Success Criteria for Phase 1**:
- Agent can create a goal, link 2–3 traces to it, see it in ambient context on restart, and demote it with a proper serialized trace.
- The ego vector shows measurable influence from the active goal(s).

Subsequent phases can add deeper NREM integration, stronger BVH/ki_hijacker biasing, richer /goal skill commands, etc.

---

## 8. Invariants (Unchanged from v0.1)

1. Geometry First
2. Auditability (especially for demoted goals)
3. Ego as Root
4. Pinning Semantics
5. Generalizability
6. Respect for .leg Invariants

---

**End of Expanded Item 1 Design (v0.2)**

This version significantly deepens the previous sketch with concrete details on data model, commands, demotion traces, payloads, vocabulary, skill content, and a first executable phase roadmap — exactly as requested.

All work is recorded in the manifold as `design:item1_ego_goal_stack_v1`.

Ready for your feedback on any part, or to begin the first implementation phase.