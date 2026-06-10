# examples/ritual_verify.md - Ritual hygiene + verify example (runnable via engram MCP or TUI)
# Current build: `target/debug/engram` (or cargo run -p engram-server) — double-check with `cargo build` before.
# Run: Use in TUI (Grok Build) or via MCP client (python examples/mcp_client.py adapted, or direct use_tool after search_tool).
# Follows engram-working-memory discipline + Code Edit Ritual v1 (pre/post for any change).

## Steps (follow Code Edit Ritual + working-memory)

1. Pre: mcp_engram_watch_workspace ("/path/to/your/engram"), mcp_engram_context_for_file("/path/to/target"), mcp_engram_record_reasoning_trace (decision_point, justification, spatial_context, goal_context, prev_trace).  # replace with your clone root; see spatial_geosphere_demo.py for pattern
2. Action (edit via search_replace/write, or test/verify).
3. Post: re-context_for_file + recall, delta trace (chained prev), mcp_engram_remember_solution or scar on friction, relate to goal/plan, promote_hot if high value.

## Example Trace (A/D/R via record_reasoning_trace or quick_trace)
- decision: Add/improve ritual_verify example per plan.
- why: Fulfill examples/ 3+ runnable + docs polish for representation of rituals (scar/verify/trace). Addresses sparse examples gap from popular recon.
- spatial: context_for_file + force_spatial_ingest on examples/plan/README (145 AST from rs files); bootstrap_in_progress noted.
- goal: goal:1780419540_prepare-and-polish-current-engram-mvp-for-public
- prev: (chain from prior like 1780422227...)

## Verify (ritual + lawfulness)
mcp_engram_verify_manifold_integrity (min_crs=0.74, sample=20)  # expect healthy, 0 issues
mcp_engram_verify_block_lawfulness (on high-value like "design:github_mvp_prep_plan_v1" or traces)
mcp_engram_spatial_status  # check item1.5 bootstrap
mcp_engram_genesis status

## End
mcp_engram_session_end with summary referencing this + plan + build confirm. prepare_compression=true for handoff.

Also: mcp_engram_remember_solution for wins, mcp_engram_scar for dead-ends, mcp_engram_relate to goal.

Run in TUI (preferred for full ritual) or via MCP client. See GITHUB_MVP_PREP_PLAN.md Phase 2/3 + docs/RITUALS.md. Current build hygiene enforced.