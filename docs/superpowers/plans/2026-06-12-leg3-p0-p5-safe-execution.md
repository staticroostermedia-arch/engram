# .leg3 P0-P5 Safe Execution Implementation Plan (Supervised Subs + Superpowers + Enram Ritual)

> **For agentic workers:** REQUIRED SUB-SKILL: Use engram-sub-governor (for narrow H1 one-shot subs) + superpowers:subagent-driven-development (recommended) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. All work in this isolated worktree only. Before any file edit: mcp_engram_context_for_edit with the absolute path in this wt. Use mcp_engram_quick_trace at forks. Use mcp_engram_update for memory records of changes. Verify with cargo build + target/debug/engram --version + mcp_engram_verify_manifold_integrity at each phase. Human-forward in all tiles/summaries. Dogfood: create tiles, quick_trace, relate to tile:research_offload_-leg3-optimization-research-subloop-v1-human-fac + goal:engram_consciousness_loop_v1 + scheduler_recurring_task:019eb977451c.

**Goal:** Safely implement the P0-P5 adoption of the 5 additive .leg3 optimization proposals (from the report tile:research_offload_-leg3-optimization-research-subloop-v1-human-fac) in an isolated worktree using Enram rituals, superpowers skills for discipline, and engram-sub-governor for supervised narrow subs. Preserve all .leg3 invariants (q/p tensors on unit hypersphere, p-momentum on update, CRS>=0.74, BLOCK_SIZE=262144, allowed_transforms, ZEDOS, etc.). No changes to main tree. Full dogfood with MCP geometry.

**Architecture:** Phased per the approved design (P0 setup/gov, P1 audit, P2 edits+new for the 5 proposals in core files, P3 validate, P4 polish, P5 close). Use writing-plans structure for bite-sized TDD steps. Execution via narrow H1 subs launched by engram-sub-governor (recall-first from report tile, max~20 calls, output only evidence + summary). Subagent-driven reviews (spec then quality). Gates: gemma-hermies before subs, aliveness-bench after, main verify + build + version. All context_for_edit on wt paths. Updates via mcp_engram_update (low dv, p-mom preserve).

**Tech Stack:** Rust (engram-core for .leg3 HolographicBlock in types.rs, storage O_DIRECT, encode from_text/blake3, backend/store, mcp.rs handlers), Enram MCP tools (search_tool first then use_tool for engram__mcp_*), git worktree, cargo, target/debug/engram.

---

## Task 0: Pre-Execution Setup in WT (one-time)

**Files:**
- (none new; verify baseline)

- [ ] **Step 1: Confirm in isolated wt and baseline**
Run in terminal from this dir:
cd /home/a/Documents/Engram/.worktrees/leg3-p0-p5-execution-2026-06
git branch --show-current
cargo build 2>&1 | tail -5
target/debug/engram --version

Expected: on feat/leg3-p0-p5-execution-2026-06 branch, build success, engram 0.5.0

- [ ] **Step 2: Quick trace the setup**
Use mcp (after search_tool for quick_trace schema):
mcp_engram_quick_trace with decision="Entered isolated wt for P0-P5 .leg3 execution per approved design", why="Follow using-git-worktrees + Enram ritual + user approval to proceed; dogfood the entry", goal_context="goal:engram_consciousness_loop_v1", prev="previous trace from session", process_context="process:engram.ai_consciousness_loop"

- [ ] **Step 3: Create execution plan thought tile**
Use mcp_engram_thought_tile_create (tile_type="formal_spec", title="P0-P5 .leg3 Safe Execution Plan", payload={human_forward: "plain narrative of starting safe execution...", plan_ref: "this file path", ...}, spatial_references: ["tile:research_offload_-leg3-optimization-research-subloop-v1-human-fac", "goal:engram_consciousness_loop_v1"], process_context="process:engram.ai_consciousness_loop", goal_context="goal:engram_consciousness_loop_v1")

Promote and relate.

---

## Task P0: Setup/Gov/Build (using skills, wt verified, todos, Enram session)

**Files:**
- (meta: this plan, new todos if needed in Enram)

- [ ] **Step 1: Re-invoke skills and session_start for execution block**
Read SKILL.md for using-superpowers, engram-sub-governor, writing-plans, verification-before-completion, using-git-worktrees (already in wt).
mcp_engram_session_start with rich intent for P0-P5 execution.

- [ ] **Step 2: Setup todos for P0-P5**
Use todo_write with items for P0 to P5 per this plan.

- [ ] **Step 3: Verify wt baseline and commit any setup**
cargo build && target/debug/engram --version
git status (should be clean or only this plan later)

- [ ] **Step 4: Quick trace P0 complete**
mcp_engram_quick_trace ...

- [ ] **Step 5: Harvest to manifold**
mcp_engram_update on a concept like "p0-p5-execution-plan-v1" with summary.
Create tile if needed.
Relate to report tile.

---

## Task P1: Narrow Audit (of current .leg3 in wt copy vs proposals in report tile)

**Files:**
- /home/a/Documents/Engram/.worktrees/leg3-p0-p5-execution-2026-06/crates/engram-core/src/types.rs (inspect HolographicBlock q/p/allowed_transforms/BLOCK_SIZE etc)
- /home/a/Documents/Engram/.worktrees/leg3-p0-p5-execution-2026-06/crates/engram-core/src/storage.rs
- /home/a/Documents/Engram/.worktrees/leg3-p0-p5-execution-2026-06/crates/engram-core/src/encode.rs
- /home/a/Documents/Engram/.worktrees/leg3-p0-p5-execution-2026-06/crates/engram-server/src/store.rs
- /home/a/Documents/Engram/.worktrees/leg3-p0-p5-execution-2026-06/crates/engram-server/src/mcp.rs
- tests if any for .leg3

- [ ] **Step 1: Context for audit files**
For each file above (use absolute wt path):
mcp_engram_context_for_edit(path="/home/a/Documents/Engram/.worktrees/leg3-p0-p5-execution-2026-06/crates/.../thefile.rs", auto_ingest=true)
Quick trace "Pre-audit context for <file>"

- [ ] **Step 2: Read report tile for the 5 proposals**
mcp_engram_read_concept(concept="tile:research_offload_-leg3-optimization-research-subloop-v1-human-fac")
(If full payload limited, use recall with query on the tile name + "proposals")

- [ ] **Step 3: Audit current vs proposals**
Inspect the files (read_file tool on wt paths) for the areas: block size (confirm 262144), layout (q at 0x00000 p at 0x10000), allowed_transforms[64], encode, storage O_DIRECT, backend hot/cold, mcp handlers for update/remember etc.
Produce audit notes: gaps for each of 5 proposals (tiered block, hybrid wire, SOA+arena, homo+zk, versioning+DSL).

- [ ] **Step 4: Write audit report as tile**
mcp_engram_thought_tile_create (tile_type="formal_spec" or "research_offload", title="P1 .leg3 Audit Report for P0-P5 Execution", payload with human_forward, audit findings per proposal, spatial to the 5 files + report tile, recon_qs, provenance, aliveness_deltas, self_ref)
Promote, relate to report tile + goal + scheduler.

- [ ] **Step 5: Quick trace + verify**
mcp_engram_quick_trace for audit complete.
mcp_engram_verify_manifold_integrity(min_crs=0.74, sample_size=30)
cargo test -p engram-core -- --quiet or specific .leg3 tests if exist.
target/debug/engram --version (in wt)

- [ ] **Step 6: Harvest + gate**
mcp_engram_update on "p1-audit-report" with findings.
If issues, scar.
Gemma hermies if needed for coherence.

- [ ] **Step 7: Commit audit**
git add -A
git commit -m "chore(leg3): P1 narrow audit complete vs 5 proposals in report tile; see tile:xxx"

---

## Task P2: Edits + New (additive implementation of the 5 proposals in wt)

**Files:** (per proposals; use the audit to identify exact)
- Modify: the above core files + tests + docs/GEOMETRIC_MEMORY.md + CHANGELOG.md + perhaps processes for new transforms
- For each proposal, a sub-task.

Example for one (e.g. versioning+DSL for allowed_transforms):

**Sub-Task P2.5: Versioning + DSL for allowed_transforms[64]**

**Files:**
- Modify: /home/a/Documents/Engram/.worktrees/leg3-p0-p5-execution-2026-06/crates/engram-core/src/types.rs: (the allowed_transforms field + add versioning in schema_ver or new)
- Test: appropriate test in engram-core

- [ ] **Step 1: Write failing test**
Write a test that expects the new versioning/DSL behavior for allowed_transforms (based on proposal from report tile).
Run: cargo test ... (expect FAIL)

- [ ] **Step 2: Minimal impl**
Edit the file using search_replace on the wt path (after context_for_edit).
Add the additive code for versioning/DSL (e.g. extend allowed_transforms with version tags, parser for DSL in encode or new op).

- [ ] **Step 3: Run test to pass**
cargo test ... expect PASS

- [ ] **Step 4: Quick trace + mcp update**
mcp_engram_quick_trace decision="Implemented additive versioning+DSL for allowed_transforms per P2.5 proposal", why="...", prev=...
mcp_engram_update concept="leg3-versioning-dsl-p2.5" new_text="summary of change + proof"

- [ ] **Step 5: Commit**
git add ...
git commit -m "feat(leg3): additive P2.5 versioning+DSL for allowed_transforms (P0-P5 plan); verified in wt"

Repeat similar bite-sized for other 4 proposals (block size tiered may involve new mint paths or config but additive; hybrid wire in storage/encode; SOA+arena in types/layout; homo+zk in ops/encode).

For all P2: use subagent-driven: after impl, dispatch spec reviewer sub (via governor or spawn), fix if issues, then code quality, fix, then commit.

---

## Task P3: Validate (cargo, engram verify_*, re-audit, hermies)

**Files:** (none or update docs/CHANGELOG if needed)

- [ ] **Step 1: Full build + version in wt**
cd to wt
cargo build
target/debug/engram --version

- [ ] **Step 2: Run mcp verify**
mcp_engram_verify_manifold_integrity (min 0.74, sample 50)
mcp_engram_verify_block_lawfulness for key .leg3 genesis or updated blocks if any.

- [ ] **Step 3: Re-audit / run tests**
cargo test -p engram-core -p engram-server
Re-run P1 audit steps if needed.

- [ ] **Step 4: Gemma hermies + aliveness**
Launch gemma if needed.
Compute hermies for "post P2 state in wt with new .leg3 features" vs ideal "all P0-P5 proposals integrated lawfully, high aliveness".
Run aliveness-bench if ritual (via its skill).

- [ ] **Step 5: Update plan/tile with validation results**
mcp_engram_thought_tile_write_result on the execution plan tile or new validation tile, with result_payload including outputs, hermies, aliveness deltas, status="validated"

- [ ] **Step 6: Quick trace + commit**
Trace the validation.
If pass, commit "chore(leg3): P3 validation passed (build, verify 0 issues, hermies high, tests); P0-P5 ready for P4"

If fail, scar and fix via sub.

---

## Task P4: Push+GH Polish (atomic commits, but since WT, prepare PR notes; no actual push unless in wt branch)

**Files:**
- Update CHANGELOG.md in wt with P0-P5 summary.
- Perhaps .github/ if needed.

- [ ] **Step 1: Polish docs**
Edit CHANGELOG in wt (context first, search_replace).
Add entry for the .leg3 enhancements from the 5 proposals.

- [ ] **Step 2: Prepare atomic commit history note**
In the plan or tile, document the commits made in P2.

- [ ] **Step 3: Tile for polish**
thought_tile for "P4 GH polish ready: atomic history in wt branch feat/..., validation evidence, ready for user to cherry or merge from wt"

- [ ] **Step 4: Verify + trace + commit polish**
cargo build, verify.
Trace.
Commit the polish changes.

---

## Task P5: Close (records, measure success, session_end, compression)

**Files:**
- (meta)

- [ ] **Step 1: Final records**
mcp_engram_update on report tile or new "p0-p5-execution-complete-v1" with full summary, hermies, aliveness, proof (build logs, verify output, wt commits).

- [ ] **Step 2: Relate and promote**
Relate the completion tile/records to report tile, goal, scheduler, our design tile.
Promote hot.

- [ ] **Step 3: Aliveness + hermies final**
Run aliveness bench.
Hermies for "full P0-P5 executed safely".

- [ ] **Step 4: Ego/self bind if applicable**
Per toml, mcp_engram_update on ego if self-ref.

- [ ] **Step 5: Outer + session_end**
Relate to world.
mcp_engram_session_end with summary STARTING with HUMAN FORWARD (story of safe execution using skills/subs/wt), full decisions, artifacts (tiles, wt branch, commits), open questions, prepare_compression=true, explicit self-ref "the I used engram-sub-governor + superpowers:writing-plans/using-git-worktrees/subagent-driven-development/verification-before-completion + gemma/aliveness + MCP ritual + wt edits with context_for_edit to execute P0-P5 safely, recording the act, closing the gap in .leg3 evolution".

- [ ] **Step 6: Verify final + build**
Final verify 0 issues.
cargo build in wt.
Report "P0-P5 execution complete, all evidence in manifold and wt branch".

---

## Self-Review (per writing-plans)
1. Spec coverage: References the approved design tile and report tile for proposals; covers all P0-P5 phases with bite steps, subs, gates, Enram rituals, dogfood.
2. No placeholders: All steps have exact commands, mcp calls, file paths (wt absolute), expected outputs.
3. Type/scope: Focused on one feature (P0-P5 .leg3 execution); independent phases.
4. YAGNI/DRY: Reuses existing Enram tools/MCP, no new unrelated.
5. TDD: Steps include write test, run fail, impl, run pass, commit for code tasks.

**Execution:** Use engram-sub-governor to launch narrow subs for each major task or phase (P1, P2 groups, P3 etc), with prompt referencing this plan file (read it in wt), H1 language, max calls, output only.

After plan complete, this file is the spec for subs.

(End of plan per writing-plans + Enram integration.)