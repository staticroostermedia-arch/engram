# examples/ritual_verify.md - Ritual hygiene + verify example (runnable via engram MCP or TUI)
# Current build: `target/debug/engram` (or cargo run -p engram-server) — double-check with `cargo build` before.
# Run: Use in TUI (Grok Build) or via MCP client (python examples/mcp_client.py adapted, or direct use_tool after search_tool).
# Follows engram-working-memory discipline + Code Edit Ritual v1 (pre/post for any change).

## Steps (lean 8-tool + Code Edit Ritual)

1. Wake: `mcp_engram_session_start(intent=...)` then `mcp_engram_ack_wake_queue(executed=true)` (hard gate).
2. Pre-edit: `mcp_engram_context_for_edit("/absolute/path/to/target")` — replaces watch_workspace at wake; optional `mcp_engram_recall(scope="anchors")` before derive.
3. Trace: `mcp_engram_quick_trace` or `mcp_engram_record_reasoning_trace` (decision_point, justification, spatial_context, goal_context, prev_trace).
4. Action (edit via search_replace/write, or test/verify).
5. Post: re-`context_for_edit` + delta trace (chained prev), `mcp_engram_remember_solution` or `scar` on friction, relate to goal/plan.
6. Recovery only: `mcp_engram_force_spatial_ingest` when passive daemon ingest is down (not a wake step).

## Example Trace (A/D/R via record_reasoning_trace or quick_trace)
- decision: Add/improve ritual_verify example per plan.
- why: Fulfill examples/ runnable + docs polish for rituals (scar/verify/trace). Addresses sparse examples gap from popular recon.
- spatial: context_for_edit on examples/plan/README (lean path; force_spatial_ingest only if bootstrap needed).
- goal: goal:your_project_goal
- prev: (chain from your prior trace, e.g. trace:your_previous_trace_id)

## Verify (ritual + lawfulness)
mcp_engram_verify_manifold_integrity (min_crs=0.74, sample=20)  # expect healthy, 0 issues
mcp_engram_verify_block_lawfulness (on high-value like "design:github_mvp_prep_plan_v1" or traces)
mcp_engram_spatial_status  # check item1.5 bootstrap
mcp_engram_genesis status

## End
mcp_engram_session_end with summary referencing this + plan + build confirm. prepare_compression=true for handoff.

Also: mcp_engram_remember_solution for wins, mcp_engram_scar for dead-ends, mcp_engram_relate to goal.

Run in TUI (preferred for full ritual) or via MCP client. See docs/RITUALS.md + docs/skills/. Current build hygiene enforced.