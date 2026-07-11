# One-time: mint Glass-Box RSI parent goals

**Operator runbook** (not a shell script). Engram MCP has no stable non-interactive batch in-repo without a client — agents execute this **once** via `search_tool` then `use_tool`.

**When:** Phase A of [Glass-Box RSI](../docs/superpowers/specs/2026-07-10-glassbox-rsi-design.md) before scheduled fires rely on durable parents.  
**Skill:** [engram-glassbox-rsi.md](../docs/skills/engram-glassbox-rsi.md)  
**Control block:** `helper:rsi_dual_loop_state` ([schema](../docs/schemas/dual_loop_state_v1.json))

Parents **serve** `goal:engram_mvp_v1`. Mint once if missing; do **not** re-mint per fire.

| Parent | Owns | Priority |
|--------|------|----------|
| `goal:dual_rsi_program` | Dual RSI S/G/M, stage machine, corpus/PEFT | high |
| `goal:ship_substrate` | Dirty-tree → test → PR | high |
| `goal:glassbox_leg` | LEG Browser split-home glass box | medium |

---

## Prerequisites

1. `mcp_engram_session_start(intent="mint glassbox parent goals")`
2. `mcp_engram_ack_wake_queue(executed=true)` before any edit path
3. Confirm program root: `mcp_engram_read_concept(concept="goal:engram_mvp_v1")` (or recall anchors)

**Idempotency:** `read_concept` each `goal:*` first. If present with a sensible statement, skip create; still run dual_loop parent list + `promote_hot` + verify.

---

## Steps (MCP)

Via Engram MCP (`search_tool` for live schema, then `use_tool` with qualified names):

### 1. Create parent goals (parent = `goal:engram_mvp_v1`)

```
mcp_engram_goal_create
  goal_id=dual_rsi_program
  statement="Dual RSI substrate+Gemma stage machine with typed verify"
  parent=goal:engram_mvp_v1
  priority=high
  affirm="One S/G/M win per fire with typed verify"
  deny="multi-track; pack dumps; stage flip without verify pass"
  reconcile="Compounds engram_mvp_v1 continuity + PEFT path"
```

```
mcp_engram_goal_create
  goal_id=ship_substrate
  statement="Ship substrate code with local verify then PR"
  parent=goal:engram_mvp_v1
  priority=high
  affirm="Ship only after ship_local / ship_skip verify pass"
  deny="claim shipped or open PR without verify pass; force-push; auto-merge"
  reconcile="Honest CI + PR path under engram_mvp_v1"
```

```
mcp_engram_goal_create
  goal_id=glassbox_leg
  statement="LEG Browser split-home glass box for process visibility"
  parent=goal:engram_mvp_v1
  priority=medium
  affirm="Read-only process visibility for dual_loop + fire goals"
  deny="LEG runs loops or auto-merges; silent success without last_verify"
  reconcile="Operators see fire lifecycle without chat archaeology"
```

Concept names resolve as `goal:dual_rsi_program`, `goal:ship_substrate`, `goal:glassbox_leg`.

### 2. Ensure serves / primary linkage

If create did not attach parent edges, relate explicitly:

```
mcp_engram_relate from=goal:dual_rsi_program  to=goal:engram_mvp_v1  label=serves
mcp_engram_relate from=goal:ship_substrate    to=goal:engram_mvp_v1  label=serves
mcp_engram_relate from=goal:glassbox_leg      to=goal:engram_mvp_v1  label=serves
```

(Use `search_tool` for exact `relate` parameter names on your MCP build.)

Do **not** change `primary_goal` unless the operator is deliberately switching focus — parents sit under `engram_mvp_v1`.

### 3. Update `helper:rsi_dual_loop_state` parents + schema fields

```
mcp_engram_read_concept(concept="helper:rsi_dual_loop_state")
```

Then `mcp_engram_update` (or remember-if-missing) so the control block includes at least:

```json
{
  "version": 1,
  "track_next": "S",
  "mcp_restart_required": false,
  "parents": [
    "goal:dual_rsi_program",
    "goal:ship_substrate",
    "goal:glassbox_leg"
  ]
}
```

Preserve existing `track_next` / `open_pr` / `gemma` / `last_fire_goal` / `last_verify` when present — only ensure `version`, `parents`, and required fields. Validate shape against [dual_loop_state_v1.json](../docs/schemas/dual_loop_state_v1.json) (`python3 scripts/validate_dual_loop_schema.py` when the block is exported as JSON).

### 4. Promote hot

```
mcp_engram_promote_hot(concept="goal:dual_rsi_program")
mcp_engram_promote_hot(concept="goal:ship_substrate")
mcp_engram_promote_hot(concept="goal:glassbox_leg")
mcp_engram_promote_hot(concept="helper:rsi_dual_loop_state")
```

Or one batch:

```
mcp_engram_promote_hot_batch(concepts=[
  "goal:dual_rsi_program",
  "goal:ship_substrate",
  "goal:glassbox_leg",
  "helper:rsi_dual_loop_state"
])
```

### 5. Verify

```
mcp_engram_goal_status / goal_get (each parent) — status active
mcp_engram_goal_get_children(parent="goal:engram_mvp_v1")  — or goal_list; parents visible
mcp_engram_read_concept(concept="helper:rsi_dual_loop_state") — parents array complete
mcp_engram_recall(query="goal:dual_rsi_program", scope="anchors") — hit after promote
```

Optional: `mcp_engram_quick_trace` decision="minted glassbox parents" why="Phase A process contract for fire lifecycle".

### 6. Session handoff

```
mcp_engram_session_end(
  summary="Minted goal:dual_rsi_program, goal:ship_substrate, goal:glassbox_leg under engram_mvp_v1; dual_loop.parents set; promote_hot",
  prepare_compression=true
)
```

---

## Acceptance

- [ ] Three durable parents exist and serve `goal:engram_mvp_v1`
- [ ] `helper:rsi_dual_loop_state.parents` lists all three
- [ ] Goals + helper on hot path
- [ ] Child fires can use `parent=goal:dual_rsi_program|ship_substrate|glassbox_leg` without re-mint

## Related

- Loop prompts: [docs/skills/loop-prompts/](../docs/skills/loop-prompts/)
- Reschedule: [docs/skills/loop-prompts/RESCHEDULE.md](../docs/skills/loop-prompts/RESCHEDULE.md)
- Plan: [docs/superpowers/plans/2026-07-10-glassbox-rsi.md](../docs/superpowers/plans/2026-07-10-glassbox-rsi.md) Task 4
