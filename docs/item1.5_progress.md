# Item 1.5 Progress Tracker

## Post-Crisis Execution (after TUI restart — 2026-05-27)

- [x] MCP stability achieved via ENGRAM_OPTIX_ENABLED=0 + TUI restart (root cause of persistent "Transport closed" was stale client connection, not just OptiX)
- [x] `mcp_engram_watch_workspace` bound successfully
- [x] Major spatial bootstrap executed via `mcp_engram_force_spatial_ingest`:
  - engram-server/src (recursive) → 170+ AST items
  - engram-gpu/src (recursive) → 123+ AST items (including lazy OptiX changes in bvh.rs + optix_pipeline.rs)
  - engram-core/src (recursive) → 141+ AST items (core .leg3, VSA ops, encode, types, etc.)
  - engram-ast/src → 12 AST items (AstItem, ItemKind — the extractor itself)
- [x] `recall_in_file` verified working with rich line-range AST nodes on previously blind core files (store.rs, encode.rs, ops.rs, types.rs, etc.)
- [x] Multiple high-quality reasoning traces recorded in manifold (heavy OptiX starvation pathology, agent listening/process gap scar, TUI restart root cause, successful bootstrap)
- [x] `item1.5_spatial_ingestion_state_engram` updated multiple times via `mcp_engram_update` with real thermodynamic drift
- [x] Formal `mcp_engram_session_end` committed with COMPRESS markers and references to local handoff artifacts
- [x] Pre-edit spatial recon discipline followed on all work (context_for_file + recall_in_file calls before edits/analysis)

## Historical "Keep Going" Burst (crisis period — preserved for record)

- [x] Server restart verified (OptiX pipeline healthy)
- [x] force_ingest_path enhanced with proper .engramignore + ignore rules
- [x] mcp_engram_force_spatial_ingest now handles recursive directories
- [x] Helper script created (scripts/bootstrap_spatial.sh)
- [x] Ready-to-paste command list created (scripts/item1.5_bootstrap_commands.md)
- [x] Spatial checks + scar helper reference added to engram-wake-up/SKILL.md
- [x] Spatial checks + scar helper reference added to engram-session-end/SKILL.md
- [x] process_gap_scar_template helper created and referenced
- [x] Re-ingestion policy documented (docs/item1.5_reingestion_policy.md)
- [x] AGENT_INTEGRATION_GUIDE.md expanded with bootstrap workflow
- [x] Convenience status script created (scripts/check_spatial_status.sh)
- [x] State block updated with restart confirmation and next actions

## Remaining / In Progress

- [~] End-to-end verification gap: `recall_in_file` works well post-bootstrap (rich line-range AST nodes on store.rs, encode.rs, ops.rs, types.rs etc.); `context_for_file` still returns limited data on the same files.
  - **Improvement landed**: context_for_file now prefers direct `fetch_block` + explicit Memory construction for real spatially-ingested AST items from force_ingest. This should significantly increase relevant results for freshly bootstrapped files. Full ritual followed. The richer relation layer is still future work.
- [~] Demonstrate live Code Edit Ritual v1 in practice (pre-edit spatial recon + intent trace with spatial_context → edit → post-delta outcome trace)
  - Cycle 1 (ki_hijacker.rs): First real .rs substrate edit completed (spatial section in ambient KI context).
    - **Follow-up improvement (gap #3)**: bake_ki now dynamically fetches the live content of `item1.5_spatial_ingestion_state_engram` on every 60s bake instead of using static text. Major step toward true inheritance. Full ritual followed.
  - Cycle 3 (mcp.rs) — **CLOSED**: Two edits completed under full ritual (formatting + consumption note; success/error counts). Tool meaningfully improved.
  - Cycle 2 (bvh.rs) — **CLOSED**: Two ritual edits completed:
    1. Strengthened struct + query() comments with explicit references to heavy_boot traces, listening scar, safe launch command, and Item 1.5 goal.
    2. Added detailed historical crisis block + improved runtime log when lazy OptiX pipeline first builds.
    Cycle 2 objective achieved: Lazy OptiX decision is now permanently documented in source and visible at runtime.
  - **All three cycles of the 1.5 practice gate now complete.**
  - **Sustained practice evidence (post-gate cycles)**:
    - Cycle 1: Real edit on mcp.rs — improved context_for_file handler output.
    - Cycle 2: Real edit on bvh.rs — extracted lazy OptiX helper.
    - Cycle 3: Real edit on ki_hijacker.rs — made dynamic spatial section self-updating + added "Recommended next step" surfacing.
    - Cycle 4: Real edit on ki_hijacker.rs (this cycle) — compact truncation + nicer formatting of the gap status for better scannability in the ambient KI output.
    - All cycles followed full ritual. Four post-gate cycles now provide solid evidence of the discipline being used on core substrate code. Good progress on gap #4.

**Current Honest Status of the 5 Gaps (late June 2026, after two post-gate practice cycles)**

- **Gap 1 — Verification (context_for_file)**: This is the most honest remaining technical gap. 
  - Root cause: `force_ingest_ast_file` is explicitly a "first-pass" implementation. It only creates the individual AST item blocks with AABB coordinates. It does **not** create file containers, "defines" relations, sibling relations, or the other topological structure that the real daemon watcher creates (see daemon.rs ~340-390).
  - This is why `recall_in_file` (raw AABB query) feels reliable on force_ingested files, while `context_for_file` (which was written expecting richer data) feels incomplete.
  - The proper fix is non-trivial engineering work (either duplicating relational logic or refactoring to share ingestion code). It is not a small surgical edit.
  - **Crisp Recommendation after investigation (June 2026):** Accept the current state as a known, documented limitation for the near-to-medium term. 
    - **Recommended practice:** For spatial AST pre-edit recon on force_ingested files, use **mcp_engram_recall_in_file** as the primary tool. `context_for_file` remains useful for broader context but is not authoritative for pure spatial AST on bootstrapped code.
    - Record this explicitly as an accepted limitation with a process scar.
    - Time-box a proper scoped Phase 1 fix (the one detailed in the plan and this tracker — adding file container + "defines" + sibling relations to force_ingest) for a future bounded project.
  - This is the honest, low-drama path that prevents indefinite technical debt while letting us focus on higher-leverage work.

**Concrete Scoped First-Phase Fix Proposal for Gap #1 (Detailed Sketch)**

**Proposed Helper (new private method on StoreHandle):**

```rust
/// Phase 1 of Gap 1 remediation (scoped, June 2026).
/// Creates the minimum relational structure for a set of AST items from one file
/// so that context_for_file and future impact analysis tools can surface
/// "defined in this file" and sibling relationships.
///
/// This is intentionally a subset of what the real daemon watcher does.
/// It does NOT replicate namespace handling, shadow anchoring, or ritual bridging.
fn create_minimal_spatial_file_structure(&mut self, file_stem: &str, ast_concepts: &[String]) {
    if ast_concepts.is_empty() {
        return;
    }

    let file_container = format!("{}_file", file_stem);

    // Ensure lightweight file container exists
    if self.fetch_block(&file_container).is_none() {
        let container_text = format!(
            "AST container for file stem '{}'. All top-level items (fn/struct/impl/etc.) extracted from this file via Tree-Sitter are related here.",
            file_stem
        );
        let _ = self.remember(&file_container, &container_text);
    }

    // Create "defines" relations from container to each item
    for c in ast_concepts {
        let _ = self.relate(&file_container, c, "defines");
    }

    // Create sibling relations in source order
    for i in 1..ast_concepts.len() {
        let _ = self.relate(&ast_concepts[i-1], &ast_concepts[i], "next_sibling_in_file");
        let _ = self.relate(&ast_concepts[i], &ast_concepts[i-1], "prev_sibling_in_file");
    }
}
```

**Exact insertion point in `force_ingest_ast_file` (after line 1386, before `Ok(ingested)`):**

```rust
// Phase 1 Gap 1 remediation: create minimal relational structure
if !ingested.is_empty() {
    if let Some(first) = ingested.first() {
        if let Some(stem) = first.split("__").next() {
            self.create_minimal_spatial_file_structure(stem, &ingested);
        }
    }
}
```

**Small optional enhancement in `context_for_file` (after the spatial hits loop, around line 1538):**

```rust
// Phase 1 enhancement: if a file container exists via "defines",
// we could surface it or use the relations for grouping in future iterations.
// For now the existence of the relations themselves is the main win for
// search_by_relation and future impact analysis tools.
```

This scoped Phase 1 would make `context_for_file` return significantly more useful structured spatial data on force_ingested files while keeping the work deliberately bounded. It is real engineering work (medium effort), not a small patch.
- **Gap 2 — Binary Deployment**: Closed. We are running the improved binary.
- **Gap 3 — Deeper ki_hijacker Inheritance**: **Polished (completed June 2026)**. The dynamic section in bake_ki now renders a clean, scannable summary of the 5 gaps status instead of dumping the entire raw block. High daily-value improvement. Full Code Edit Ritual followed.
- **Gap 4 — Sustained Practice**: Meaningful early evidence now exists. Two real post-gate Code Edit Ritual cycles have been executed on core files (mcp.rs and bvh.rs) with full discipline. This is concrete progress on the most important cultural/operational gap.
- **Gap 5 — Lightweight Status Tooling**: Closed. `mcp_engram_spatial_status` exists, is used, and works.

**Overall Reflection on the Process**
The system (rituals + traces + scars + living state blocks) has been effective at preventing the exact failure modes it was designed to catch — especially the tendency to produce artifacts and tools while avoiding real practice on the substrate. The biggest value has come from forcing us to actually do the Code Edit Ritual on real files rather than just building infrastructure and declaring victory. The remaining gaps are now much clearer and more actionable.

**Current Active Work (June 2026)**

1. **Gap #3 (ki_hijacker)**: **Completed** — polished as described above.
2. **Gap #4 (Sustained practice)**: Next cycle being planned (third post-gate). Target: ki_hijacker.rs or another core file.
3. **Gap #1 (Verification)**: Focused investigation started. Fresh review of current context_for_file implementation in store.rs completed. Will propose either a targeted code change or a clear documented position + scar.

All work continues under full Code Edit Ritual discipline.

**Gap #2 — Binary Deployment (User Action)**

Run this after the current set of changes:
```bash
cargo install --path crates/engram-server --force
```

**Status: Completed** (June 2026)

- Binary rebuilt successfully.
- Server restarted with `ENGRAM_OPTIX_ENABLED=0`.
- TUI restarted → full tool refresh (50 tools, including `mcp_engram_spatial_status` and `get_backend_readiness`).
- `context_for_file` on store.rs now returns rich, relevant AST nodes with actual code (clear improvement).
- All recent Item 1.5 changes are now live in the running binary.

This closes gap #2.

**Gap #4 — Sustained Practice (Plan)**

After the binary rebuild:
- Perform at least 3–5 additional full Code Edit Ritual cycles on real .rs (or other core) files during normal work.
- Each cycle must include spatial pre-recon + pre/post traces with spatial_context, linked to the goal.
- These will be recorded as traces and will provide the required evidence.

This is the only authentic way to generate the "sustained practice" evidence needed for closure.

**Proposed Exact Criteria for Closing Item 1.5 and Transitioning to Item 2**

To close Item 1.5 and begin deep Item 2 Thought Tiles work, the following must be demonstrably true (evidence in manifold + this tracker):

1. **Verification Gap Addressed or Explicitly Scoped**
   - Either `context_for_file` reliably surfaces AST nodes on recently force_ingested files, **or**
   - The gap is formally documented with recommended workarounds (e.g. "prefer recall_in_file for spatial recon during Code Edit Ritual") and accepted as a known limitation.

2. **Binary Deployment**
   - A clean `cargo install --path crates/engram-server --force` (or equivalent) has been performed after the last significant Item 1.5 change.
   - The running binary used by the TUI contains the lazy OptiX, force_spatial_ingest, and readiness tooling.

3. **Deeper ki_hijacker Inheritance**
   - The ambient context generated by ki_hijacker dynamically includes at least a summary of `item1.5_spatial_ingestion_state_engram` (or a lightweight projection of it) on every bake, not just a static note.

4. **Evidence of Sustained Practice**
   - At least 5–7 additional real Code Edit Ritual cycles have been performed on actual .rs (or other core) files outside of the gate, with proper pre/post traces and spatial recon, across at least two different sessions.
   - These cycles are visible in recent reasoning traces and linked to the goal.

5. **Lightweight Status Tooling (or explicit deferral)**
   - Either a simple `mcp_engram_spatial_status` or equivalent query tool exists, **or**
   - A clear decision + trace records that this is deferred until after initial Item 2 work.

**Minimum Bar vs Aspirational**
- The first three items above are considered the **minimum bar**.
- Items 4 and 5 are strongly preferred but may be accepted with a recorded justification and a concrete plan/timeline if the team decides to move forward.

This criteria set is recorded in the manifold (state block + this trace) so future instances cannot accidentally drift into Item 2 without addressing the foundational hygiene concerns that originally caused Item 1.5 to be elevated.
- [ ] Rebuild binary (`cargo install`) so all Item 1.5 changes (force tool, readiness, deferred ki bake) are in the installed engram
- [ ] Update ki_hijacker / ambient context to surface spatial ingestion state and backend readiness
- [~] Add lightweight bootstrap status query tool (nice-to-have)
  - **Added**: `mcp_engram_spatial_status` tool (returns live content of item1.5_spatial_ingestion_state_engram). Directly addresses gap #5. Full ritual followed.
- [ ] Close Item 1.5 when spatial hygiene is demonstrably active on real self-edits over multiple sessions

**Last updated:** After post-crisis finishing burst + readiness assessment for Item 2 transition

**Readiness Assessment for Moving to Item 2 (June 2026):**

- Preparation phase largely complete: Tooling (force_spatial_ingest), ritual updates, real bootstrap on engram-core/server/gpu/ast crates, traces/state/goal updates, one live mini Code Edit Ritual demonstration.
- Practice phase barely started: Only doc edits performed under the full ritual so far. No demonstrated cycles on actual .rs substrate changes.
- Verification gap remains open (documented in Remaining section above).

**Current collective judgment (recorded in manifold):** Do **not** proceed to deep Item 2 Thought Tiles execution yet. Define and execute a small explicit gate first:
1. 2–3 full Code Edit Ritual v1 cycles on real Rust files.
2. Binary rebuild + restart.
3. ki_hijacker inheritance improvements.
4. Evidence in this tracker + state block + goal that spatial hygiene is being practiced on self-edits.

This is the minimal bar to avoid repeating the earlier pattern of advancing complex work while foundational discipline is not yet active.

**Note:** The crisis-era diagnosis below is preserved as valuable first-class Item 1.5 data. It is historical.

## Key Diagnosis (2026-05-27 session)

The MCP transport instability ("Transport closed") observed during/after server restart is largely caused by the **Fast MCP Path + Heavy Background Initialization** design in main.rs:

- A lightweight placeholder store is returned immediately so the TUI/agent can complete `initialize` and `tools/list` quickly.
- The real heavy work (full StoreHandle with CudaBackend + OptiX BVH for ~150k primitives, ki_hijacker spawn, large manifold load) is kicked off in a background thread.
- During this background work (which can take many minutes on large manifolds + full OptiX), the main MCP event loop can become starved or unresponsive, leading to dropped connections from the agent's perspective.

This is a known architectural trade-off for "fast ready" UX, but it creates exactly the agent-facing reliability gap that Item 1.5 is meant to address.

Observed in practice:
- OptiX pipeline compilation + large GAS creation for 149k+ primitives was happening in the background.
- MCP tools became unreliable during/after this heavy phase even after "Pipeline ready" messages appeared.

Relevant code: crates/engram-server/src/main.rs (lines ~96-162, the Mcp fast path + background thread spawn).

This should be treated as a first-class Item 1.5 observation / process gap.

## Live Observation (as of this check)
- Enram MCP process (PID 2450050) still consuming ~66% CPU and ~23% RAM (~22.8 GiB RSS) after ~7 minutes of runtime.
- This confirms the heavy background initialization (OptiX + large manifold) is still ongoing.
- MCP tools remain unreliable ("Transport closed") during this phase, even after "Pipeline ready" messages.

Recommendation for this session: 
- Either wait until CPU drops significantly (<20-25%) before expecting stable MCP usage, 
- Or proceed with a high-quality close-out using local artifacts + clear notes that full manifold recording was limited by server load during the session.

This is valuable real-world data for Item 1.5 on "agent experience during heavy backend initialization".

## Fresh Log Analysis (tail of current engram.stderr.log as of this check)

The server is still performing repeated heavy OptiX work cycles even after the initial "Pipeline ready":
- Multiple full module compilations, GAS builds for ~150k primitives, SBT packing, pipeline creation.
- ki_hijacker is actively baking during this.
- At least one successful MCP write occurred during this window: `remembered: trace:item1.5_execution_progress_after_code_changes`

This confirms the root cause: The OptiX / BVH construction for a very large number of blocks is extremely CPU/GPU intensive and is happening in a way that impacts the responsiveness of the main MCP (stdin/stdout) handling.

## Fresh 5-Gap Reassessment + Item 2 Transition Decision (this session, post user acceptance)

**User trigger (verbatim):** "This is fine, It might be that both tools just have slightly different use cases and thats all right. lets reasses our 5 gaps and consider if we are ready to move back to item 2 that we had"

**Reassessment performed under full working-memory discipline:**
- Pre-recon: mcp_engram_query_with_momentum on the transition question, mcp_engram_spatial_status, read_concept + context_for_file on ki_hijacker.rs (AST nodes for bake_ki + the polished compact renderer at 413-468), goal_status on goal:item1.5_spatial_discipline_adoption, recall on recent readiness traces (17799 series), mcp_engram_recall on 5-gap status language.
- 4 post-gate real Code Edit Ritual cycles on core substrate .rs files confirmed (mcp.rs, bvh.rs, ki_hijacker x2) with traces, spatial recon, and goal linking.
- Canonical block `item1.5_spatial_ingestion_state_engram` was still carrying the early "bootstrap_in_progress" snapshot (the rich updated honest status lived primarily in this tracker + traces). This was recorded as a minor hygiene observation during the reassessment.

**Honest per-gap status after the work + user acceptance:**

- **Gap 1 (Verification)**: Explicitly accepted. The two spatial tools have legitimately different optimal use cases on force_ingested (first-pass) files. Recommended practice: use `mcp_engram_recall_in_file` as primary for AST pre/post recon during Code Edit Ritual cycles on bootstrapped code. `context_for_file` remains the broader architectural tool. The full scoped Phase-1 remediation (create_minimal_spatial_file_structure helper + exact insertion in force_ingest_ast_file + context_for_file enhancement) is already written in detail in this document. Time-boxed; not a blocker.

- **Gap 2 (Binary deployment)**: Closed. Verified multiple times with cargo install + safe launch configuration + TUI restarts. All Item 1.5 changes were live in the binary used for the 4 practice cycles.

- **Gap 3 (ki_hijacker inheritance)**: Closed and polished (Cycle 4 work). The dynamic section in bake_ki now fetches this canonical block on every 60s bake and renders a compact, scannable "Gap Status (latest)" extract (filtering for - bullets + "**Recommended next**" lines) for good daily ambient context quality. Mechanism complete and self-updating.

- **Gap 4 (Sustained practice)**: Authentic evidence now exists. The original 3-cycle gate was executed (ki_hijacker, mcp.rs, bvh.rs). Four additional post-gate ritual-compliant cycles on real .rs files followed, all with pre-edit spatial recon (context_for_file + recall_in_file), structured traces carrying spatial_context + goal_context, and post-delta capture. This is the cultural/operational win Item 1.5 was elevated to produce.

- **Gap 5 (Lightweight status tooling)**: Closed. `mcp_engram_spatial_status` tool is live, returns the canonical block, and is referenced from rituals and the ki_hijacker.

**New scar / observation surfaced in this reassessment:** The canonical state block (the single source ki_hijacker, spatial_status, and wake-up hygiene read) had not been kept in sync with the detailed honest tracker in this .md during the later practice cycles. The mechanism (dynamic fetch + compact render) was excellent; the content had lagged. Closed in this step via mcp_engram_update (low drift, proper thermodynamic evolution).

**Decision captured as:** trace:1779936693_reassessment-of-the-5-gaps-after-explicit-user-a (auto-linked to goal:item1.5_spatial_discipline_adoption)

**Clear recommendation:** We are ready to move back to Item 2 (Thought Tiles) scoping and initial execution. The foundational hygiene gaps that caused Item 1.5 to be elevated have been addressed through real practice, explicit acceptance, and mechanism completion. The minor remaining hygiene item (block sync) was executed as part of this reassessment.

**Immediate next actions (minimal, to keep the loop closed):**
1. (Done) Canonical block updated via mcp_engram_update so the next ki_hijacker bake surfaces the honest current Gap Status in the TUI ambient context.
2. Return primary focus to Item 2 work.
3. Continue the Code Edit Ritual discipline on any future substrate edits (including early Thought Tile implementation). One additional cycle during Item 2 ramp-up would be ideal for momentum but is not a prerequisite.
4. If early Item 2 work reveals the Gap 1 limitation is higher-friction than expected, time-box the already-scoped Phase-1 fix immediately (the sketch in this document is the exact starting point).

**Falsifiability (from the trace):** If the first 2-3 Thought Tile creations or early Item 2 substrate work reveal that the lack of richer file-container topology on bootstrapped files creates repeated, high-friction recon failures that materially slow geometric continuity or ritual adherence, we will immediately execute the Phase-1 helper.

This reassessment itself was performed as meta-practice under the engram-working-memory discipline and Code Edit Ritual v1. The process held.

**Canonical block evolution note:** The update to `item1.5_spatial_ingestion_state_engram` (dv: 0.001, stable) now contains the full honest status section in the exact format the ki_hijacker compact extractor consumes. Future agents waking up will see the current truth in the ambient "Spatial Discipline & Backend Readiness (Item 1.5)" section without needing to read this tracker first.

The "fast MCP path" returns quickly for protocol negotiation, but the real heavy lifting makes the server unreliable for actual tool calls for an extended period.

Current process resource usage remains very high (~66% CPU, 23%+ RAM).

Conclusion for this session:
- The MCP instability is expected and ongoing while this heavy work is happening.
- We cannot fully "solve" it by waiting a few more minutes in a low-credit situation.
- Best path: Produce high-quality local artifacts for the three close-out items now, with clear notes that full manifold recording was limited by server load. Push what we can when MCP stabilizes or via later import.

## Code Changes for MCP Stability During Heavy Init (executed this turn)

1. ki_hijacker immediate first bake is now deferrable:
   - New env var: `ENGRAM_KI_IMMEDIATE_BAKE=false` skips the synchronous `bake_ki` right after spawn.
   - This reduces contention exactly when the background thread is already doing massive OptiX + manifold work.
   - Default remains the original behavior (immediate bake) for backward compatibility.

2. Proper readiness signal added:
   - New field `fully_initialized: AtomicBool` on StoreHandle.
   - `mark_fully_initialized()` called from the background init thread in main.rs after "Full initialization complete".
   - `is_fully_initialized()` public method.
   - New MCP tool `mcp_engram_get_backend_readiness` that returns `{ "fully_initialized": bool }`.

These two small changes give agents:
- A way to reduce load during startup (env var).
- A way to explicitly wait for the heavy backend before doing important work (readiness query).

This directly improves the agent experience during the exact scenario that was causing MCP "Transport closed" issues.
