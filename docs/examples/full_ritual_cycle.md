# Full Ritual Cycle Demo: Wake → Heavy Meta-Work (with Tiles + Sub-Agent) → End → Rehydrate & Continue

**Complete end-to-end example for agents.**

This demonstrates the power of the published skills: an agent instance performs real work using the geometric loop, produces durable artifacts, hands off terminal state, and the *next* instance rehydrates and continues without flat reset or re-derivation.

## Scenario

Agent is doing GitHub MVP prep / skills exposure work (meta-work on the system itself).

Steps (follow exactly the protocols in docs/skills/):

1. **Wake-up** (engram-wake-up.md)
   - session_start with intent naming the arc + continuation.
   - Verify (manifold, spatial passive status, genesis).
   - Rehydrate: momentum for anchors, surface hot tiles/traces/goals from prior, relate continuation.
   - Spatial: watch_workspace (passive now), context_for_file on key files (mcp.rs, skills, plan).
   - Activate working-memory.

2. **Heavy Meta-Work** (engram-working-memory.md + thought-tiles)
   - Pre: recall helpers (meta_work_escalation_v1, current_meta_arc).
   - For edits: context_for_file + recall_in_file (AABB) pre, trace intent, update-prefer or write, post delta trace + relate.
   - Meta pattern: roadmap/policy/gap -> mint knowledge_graph or formal_spec tile at boundaries, with spatial_references, promote_hot.
   - Sub-agent governance: launch narrow one-shot for recon (e.g. sub-agent for "popular memory github patterns"), with supervisor monitoring, kill on doom, fallback synthesis. Record scar/trace for lessons. Escalate meta via subvisor H1 if no tile.
   - Use tiles for the plan itself, traces for decisions, relate to primary goal.

3. **Session-End** (engram-session-end.md)
   - Review goals, update statuses + completion traces.
   - Trace chain summary (key decisions with prev chaining).
   - Spatial check.
   - session_end with summary referencing exact traces/tiles, COMPRESS markers for stabilized chains, hot promote artifacts.
   - Advance anchors, produce continuation target (agent_instance_terminal relation).
   - Verify handoff.

4. **Next Instance (simulated rehydrate)**
   - New session_start.
   - Wake-up: query momentum + search_by_relation for prior terminal + hot tiles, surface the plan tile + traces from previous handoff, recall helper:session_hydration_cache if present.
   - Continue: working-memory discipline on the next phase, using the rehydrated context (no re-derivation of the plan).

## Runnable Python Sketch (see examples/hello-engram-agent.py for the tiny loader version)

```python
# Extend the hello script or use real client
client.session_start(intent="full_ritual_cycle_demo - wake meta(tile+subgov) end rehydrate")

# Wake steps (simplified; real follows full engram-wake-up.md phases)
client.verify_manifold_integrity()
client.spatial_status()  # now passive
# ... rehydrate hot tile from prior, relate continuation

# Meta-work
trace_id = client.record_reasoning_trace(decision_point="Add full cycle + sub-agent gov demo", justification="Closes missing items for external agents", spatial_context="docs/examples/", ritual_context="working-memory + tiles + subvisor")
tile_id = client.thought_tile_create(tile_type="knowledge_graph", payload="Full cycle: wake->meta with sub-agent governance (narrow+supervisor+scar for doom loops + H1 escalation for no-tile meta)", spatial_references=["docs/examples/full_ritual_cycle.md", "docs/examples/sub_agent_governance.md"])
client.promote_hot(tile_id)

# Simulate sub-agent (see sub_agent_governance.md for real narrow prompt + supervisor code)
# sub_task_id = launch_narrow_sub("recon popular github memory repos", max_calls=10, prompt="one action, MCP first, report only")
# supervisor_monitor(sub_task_id)  # kill on stagnation, fallback from artifacts/MCP
# client.scar("subagent_doom_loop_example") if failed

client.session_end(summary=f"Cycle complete. Key: {trace_id}, {tile_id}. Handoff via COMPRESS + hot. Next wake rehydrates tile+traces for continuation.", prepare_compression=True)

# Next wake simulation (in same or new process)
client.session_start(intent="next_instance - rehydrate from prior handoff")
# In real wake: momentum query surfaces the hot tile + prior traces, search_by_relation for continuation edge,
# recall the knowledge_graph tile content, continue meta-work with working-memory (no re-read of whole plan).
print("Rehydrated: ready to continue phase without flat context loss.")
```

## Why This Matters

- **Continuation**: The next agent doesn't start from zero or flat summary; it rides the p-tensor + relations + hot cache + COMPRESS from the handoff.
- **Self-improving**: Sub-agent lessons (doom loops) become scars/traces that future subs avoid. Meta-work forces tiles (per 2026-06 evolution).
- **Passive spatial**: Edits to the demo itself use context/recall automatically (no manual force/save).
- **Dogfood**: This doc + scripts were created following the rituals (pre/post traces, tile for the arc, relate to goal:1780419540, update plan).

Run the hello-engram-agent.py first for the loader, then extend with this cycle. See SKILLS.md (root) for index, docs/skills/ for full protocols, processes/monitor/subvisor.toml for the declarative governance, and GITHUB_MVP_PREP_PLAN.md for real usage history of these patterns during prep.

**As an agent**: Load docs/skills/engram-*.md, follow this cycle on your own ambitious tasks. The manifold will make you better over time.