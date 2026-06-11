# Engram Rituals

Rituals turn the geometric substrate into living self-model and continuity for agents.

**Dogfooding:** When the agent uses these rituals (and the underlying mcp_engram_remember/relate/record_reasoning_trace/update/goal/scar/verify/spatial + thought_tile tools) *on its own work and decisions*, the activity becomes first-class persistent geometry (traces, relations, CRS evolution, structured tiles for compression). The GitHub MVP prep work was dogfooded this way (with 2026-06 correction pass for better update + tile usage) — see GITHUB_MVP_PREP_PLAN.md execution log, tiles, and traces. This is "eating your own dog food" on a non-flat memory system.

**Recognition for update and tiles + automatic escalation (ritual process update 2026-06):** See working-memory "Recognition Heuristics" and "Automatic Escalation", thought-tiles "Recognition triggers" and "Expected for Re-hydration". In practice: recall `helper:meta_work_escalation_v1` + `helper:current_meta_arc` for meta arcs; ki/wake/session/subvisor auto prompt for tile/update; tiles expected (not optional) for bundles/re-hydration. Reconcile step via helper:reconcile_step_v1. Full plan/execution in GITHUB_MVP_PREP_PLAN.md. Subvisor enhanced for H1 meta detection.

## Core Rituals (Skills)

**Full detailed protocols for agents are published in `docs/skills/`** (load these .md files as your operating procedures):

- `docs/skills/engram-wake-up.md` — Full geometric continuation (living anchors via momentum/relations, session_start + bind, Phase 1.5 lawfulness, rehydrate, goal stack, spatial hygiene, success criteria).
- `docs/skills/engram-working-memory.md` — The runtime discipline (geometric priority, update vs remember, traces/scars, Code Edit pre/post AABB, thought tiles for meta, hot promotion, quick templates).
- `docs/skills/engram-session-end.md` — Terminal handoff (crystallize, goal review + traces, COMPRESS, anchors, verification, success criteria).
- `docs/skills/engram-thought-tiles.md` — When and how to mint (mandatory for meta-work, types, hot promotion).
- `docs/skills/README.md` — Index + "For Agents" quickstart loop.

**Summary**:
- **engram-wake-up**: ... (as before, now delegated to the full file).
- **engram-working-memory**: ... (as before).
- **engram-session-end**: ... (as before).
- **engram-goal** + **engram-thought-tiles**: See the dedicated skills/ files + working-memory Item 2 section.

Others (harness-gate, lawfulness-metrics, substrate tools, etc.): See MCP_TOOLS_REFERENCE.md and the individual skills when needed.

**Runnable demos & governance**: root SKILLS.md, docs/examples/full_ritual_cycle.md (complete wake→meta (tiles+sub-agent gov)→end→rehydrate), docs/examples/sub_agent_governance.md (H¹, narrow, escalation, doom prevention), examples/hello-engram-agent.py (loads skills + loop).

This structure lets any agent (Grok or otherwise) discover and follow the exact rituals we dogfood without depending on the private .grok/ TUI config.

## Process Architecture Sheaf

Declarative processes/*.toml (two-level naming agent:engram.<type>.<domain>-<action>) registered dynamically at session_start via load_process_sheaf in mcp.rs. Category table (object, morphism OP_*, sheaf_role, h1_handler). Gluing/H¹ for subvisor (monitor for sub-agent governance: narrow one-shot, loop detect via H¹ on tool graph, geometric enforce, scar repetitive).

See processes/ (7 tomls), design/processes/, mcp.rs:90, prior sheaf execution traces.

## Code Edit Ritual (v1)

Mandatory for substrate changes (crates/, skills/, etc.):
1. Pre: watch_workspace, context_for_file, recall_in_file, momentum/relation on AST nodes, intent trace (decision, why, spatial_context, goal_context).
2. Edit (update-prefer).
3. Post: re-context, delta trace (chained prev), relate to goal/arc/praxis:spatial_manifold_impact_analysis, scar if needed.

## Governance for Sub-agents

Narrow one-shot only (single action, mcp geometric first, Primary Objective + negative examples in prompt, "report to supervisor"). Kill on loop/stagnation. Main high-cognition. Subvisor process (OP_INVERT + H¹) for oversight. See scar for past doom loops, subvisor.toml.

## Lawfulness & Metrics

verify_manifold_integrity, verify_block_lawfulness, genesis, spatial_status, ki freshness. metric:wake_up_verification_<iso> + trend. overall_lawful + score.

See SKILL.md files in .grok/skills/, GITHUB_MVP_PREP_PLAN.md, 2026-06_Substrate_CS_Gap_Closure_Roadmap.md, engram-wake-up/SKILL.md (Phase 1.5), working-memory/SKILL.md (Code Edit + hygiene).

For external agents: follow rituals for lawful use of the substrate.

## Phase 5: Linguistic Rituals (ritual_linguistic_wake + NREM/ego.leg3 for calculus; P5 tomls)

Additive P5 (coord + sub5): `processes/ritual/ritual_linguistic_wake.toml` + extensions in `nrem-consolidation.toml` (and sibling `processes/linguistic/*.toml` for sheaf).

- `agent:engram.ritual.linguistic-wake`: zedos_type=ritual; sheaf_role linguistic wake gluing CRS0.85/homotopy via fibered; h1=OP_GEOMETRIC_PRODUCT; mcp_tools: session_start/context/trace/remember/relate/verify/quick; requires wake+linguistic-calculus; produces: ego.leg3, linguistic_bundle_high_crs, NREM_promoted_linguistic, trace:*_linguistic_wake; invariants: leg3 isomorphism, crs_0.85_for_calculus_ops, fibered_homotopy_coherence, NREM promotion of high-CRS linguistic bundles to ego.leg3, categorical class mixing rules (scar on violation e.g. number/word without geometric_product), lyapunov stable.
- Integrates NREM (produces ego.leg3 + hot + linguistic crs gate 0.85 + homotopy fibered check) with mcp linguistic calculus surface (P3 compress/de/fibered + P4 calculus ops) + session.
- load_process_sheaf (mcp.rs) picks "ritual" dir (and linguistic/) at wake; P5 tomls now present + active.
- Full pipeline survival: Leg3 mint linguistic (P1) → op compress/diff/operad (P3/P4 via mcp_linguistic_calculus etc) → decompress → NREM (toml + promote/records) → ego.leg3 (verify/concept high CRS) with homotopy/text-coeff fidelity >=0.85.
- Dogfood: traces/relates to goal:mvp_gap_closure_v1; see Phase 6 sub-agent, e2e test in crates/engram-server/src/mcp.rs, MCP_TOOLS_REFERENCE.md, GITHUB_MVP_PREP_PLAN.md.

These close the categorical/linguistic calculus loop for geometric self-model (non-flat).

## Phase 6: Public Polish + Documentation (CATEGORICAL_LINGUISTIC_CALCULUS.md, README, final refs)

Additive Phase 6 (Sub-agent 1, 3-iter loop per handoff): 
- New public file `docs/CATEGORICAL_LINGUISTIC_CALCULUS.md` (overview of “Calculus over Words”, geometric/sheaf foundation, full P1–P6 surface with exact refs to ZEDOS_LINGUISTIC 0x4C etc/types.rs/ops.rs/processes/linguistic/*.toml + ritual_linguistic_wake.toml, mcp_linguistic_calculus + mixed ops usage exs with CRS 0.85+/0.87, word/number bridging, full NREM/ego.leg3 lifecycle + invariants/CRS gates, “How to Try” with build/MCP/session_start/verify/hygiene + link to examples/hello-engram-agent.py, links to RITUALS §Phase5 / MCP_TOOLS / GEOMETRIC_MEMORY / GITHUB_MVP_PREP_PLAN / AGENTS).
- README.md: prominent new “Categorical Linguistic Calculus” section placed after Memory Model (~l41) with short mcp ex (differentiate), mixed bridge ex, “How to try the new calculus” (cargo + target/debug/engram --version, MCP wire, session_start, mcp call, extend python ex, verify), link to full doc, note on advancing to geometric *reasoning* substrate + Phase 6 polish complete.
- Updates to this RITUALS.md (this Phase 6 section), MCP_TOOLS_REFERENCE.md (linguistic section), GITHUB_MVP_PREP_PLAN.md (exec log appends x3 iters with hygiene verbatim, CRS, sub_id 019eb27f-d94f-7ea0-b580-36adb41c19c8, readiness).
- Full e2e ready (mint mixed → P3 compress → P4 calc/mixed bridges → decompress → P5 NREM/ego promotion CRS0.85+ homotopy/fidelity + class-mix guard), public docs polished, ready for sharing. Hygiene (cargo test -p engram-core -p engram-server, check, target/debug/engram 0.4.0 + ls, mcp verify) + mcp verify at start + end each iter. Dogfood: quick_trace/remember/relate/plan appends to goal:mvp_gap_closure_v1 + prior linguistic traces. Narrow (3 iters, pre context/read/grep, additive only, no core edits). CRS gates passed. Sub calls narrow.

See new CATEGORICAL_LINGUISTIC_CALCULUS.md for public “How to try”. All invariants (.leg3/CRS/p-mom/VSA/sheaf/H¹) preserved. Ready for Sub-agent 2 commit+push.