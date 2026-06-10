## PR Checklist for Engram MVP Prep / Geometric Memory + Rituals + MCP + Rust + Non-Flat System

- [ ] **8-tool lean contract respected**: Public docs/agents lead with [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md); lean wake = one `session_start`; edit prep = `context_for_edit`; no mandatory `watch_workspace` at wake in agent-facing docs.
- [ ] **Ritual hygiene followed**: Pre-edit `mcp_engram_context_for_edit` (or `context_for_file`) + `recall_in_file` (if code) + `mcp_engram_quick_trace` or `record_reasoning_trace`; post-edit delta trace chained; `mcp_engram_scar` for rejected paths.
- [ ] **Manifold / lawfulness / spatial verified**: Ran `mcp_engram_verify_manifold_integrity`, `mcp_engram_verify_block_lawfulness` (if high-value), `mcp_engram_spatial_status`, `mcp_engram_genesis status` as relevant; updated `item1.5_spatial_ingestion_state_engram` if bootstrap impacted.
- [ ] **Enagram records**: New `trace:*` / `remember` / `remember_solution` / `scar` / `goal_*` / `relate` as appropriate; `promote_hot` for continuity artifacts; linked to `goal:1780419540...` (or primary).
- [ ] **Current build used**: Verified via `cargo check/build`, `target/debug/engram --version` (or equivalent) matches source state (see Phase 0 build_check); harness/MCP tests reference fresh binary if applicable.
- [ ] **Examples / docs run**: New or changed examples are runnable immediately (`cargo run --example ...` or python); README/docs links updated and consistent with local current state.
- [ ] **Non-flat / geometric invariants preserved**: Changes respect .leg3 (q/p/CRS/Merkle), VSA (OP_*), spatial AABB, sheaf gluing (from processes/*.toml), CRS/scar/verify gates, continuation bundles, ego.leg3/NREM, etc. No breakage to MCP tools (62 tiered, 8 essential), rituals (wake/working-memory/session-end/code-edit), or subvisor governance.
- [ ] **GH / popular best practices**: Aligns with plan (hero/comparison in README, matrix CI, templates, metadata keywords like geometric-memory/rituals/mcp/non-flat, visuals, etc.); emulates patterns from mem0/ragflow/memvid/qdrant/chroma etc. (badges, examples, explicit memory model, citations if applicable).
- [ ] **Atomic commit**: Conventional message referencing plan + traces (e.g. 'docs(readme): add geometric hero + comparison per GITHUB_MVP_PREP_PLAN.md (trace:XXXX)') ; used git-eng MCP where possible for trace.
- [ ] **Post-push validation**: After merge, online README/.github matches; run `mcp_engram_*` verifies; no regression in current build/harness.

**Related**:
- Plan: docs/GITHUB_MVP_PREP_PLAN.md (with execution log)
- Goal: goal:1780419540_prepare-and-polish-current-engram-mvp-for-public
- Prior: build_check, phase1_audit, traces from Phase 0/1/2, scar for sub-agent lessons, supervisor report.
- Sub-agents: popular (success), local (cancelled per governance), supervisor (success).

Thank you for helping represent the unique geometric/ritual/non-flat nature of Engram well on GitHub!