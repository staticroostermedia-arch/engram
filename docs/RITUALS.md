# Engram Rituals

Rituals turn the geometric substrate into living self-model and continuity for agents.

**Dogfooding:** When the agent uses these rituals (and the underlying mcp_engram_remember/relate/record_reasoning_trace/update/goal/scar/verify/spatial + thought_tile tools) *on its own work and decisions*, the activity becomes first-class persistent geometry (traces, relations, CRS evolution, structured tiles for compression). See docs/SUBSTRATE_WINS_PLAN.md for the harness injection learning loop.

**Recognition for update and tiles + automatic escalation (ritual process update 2026-06):** See working-memory "Recognition Heuristics" and "Automatic Escalation", thought-tiles "Recognition triggers" and "Expected for Re-hydration". In practice: recall `helper:meta_work_escalation_v1` + `helper:current_meta_arc` for meta arcs; ki/wake/session/subvisor auto prompt for tile/update; tiles expected (not optional) for bundles/re-hydration. Reconcile step via helper:reconcile_step_v1. See docs/SUBSTRATE_WINS_PLAN.md.

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

See processes/ (ritual, harness, monitor, linguistic), mcp.rs load_process_sheaf, prior sheaf execution traces.

## Code Edit Ritual (v1)

Mandatory for substrate changes (crates/, skills/, etc.):
1. Pre: watch_workspace, context_for_file, recall_in_file, momentum/relation on AST nodes, intent trace (decision, why, spatial_context, goal_context).
2. Edit (update-prefer).
3. Post: re-context, delta trace (chained prev), relate to goal/arc/praxis:spatial_manifold_impact_analysis, scar if needed.

## Governance for Sub-agents

Narrow one-shot only (single action, mcp geometric first, Primary Objective + negative examples in prompt, "report to supervisor"). Kill on loop/stagnation. Main high-cognition. Subvisor process (OP_INVERT + H¹) for oversight. See scar for past doom loops, subvisor.toml.

## Lawfulness & Metrics

verify_manifold_integrity, verify_block_lawfulness, genesis, spatial_status, ki freshness. metric:wake_up_verification_<iso> + trend. overall_lawful + score.

See docs/skills/, docs/SUBSTRATE_WINS_PLAN.md, 2026-06_Substrate_CS_Gap_Closure_Roadmap.md.

For external agents: follow rituals for lawful use of the substrate.

## Phase 5: Linguistic Rituals (ritual_linguistic_wake + NREM/ego.leg3 for calculus; P5 tomls)

Additive P5 (coord + sub5): `processes/ritual/ritual_linguistic_wake.toml` + extensions in `nrem-consolidation.toml` (and sibling `processes/linguistic/*.toml` for sheaf).

- `agent:engram.ritual.linguistic-wake`: zedos_type=ritual; sheaf_role linguistic wake gluing CRS0.85/homotopy via fibered; h1=OP_GEOMETRIC_PRODUCT; mcp_tools: session_start/context/trace/remember/relate/verify/quick; requires wake+linguistic-calculus; produces: ego.leg3, linguistic_bundle_high_crs, NREM_promoted_linguistic, trace:*_linguistic_wake; invariants: leg3 isomorphism, crs_0.85_for_calculus_ops, fibered_homotopy_coherence, NREM promotion of high-CRS linguistic bundles to ego.leg3, categorical class mixing rules (scar on violation e.g. number/word without geometric_product), lyapunov stable.
- Integrates NREM (produces ego.leg3 + hot + linguistic crs gate 0.85 + homotopy fibered check) with mcp linguistic calculus surface (P3 compress/de/fibered + P4 calculus ops) + session.
- load_process_sheaf (mcp.rs) picks "ritual" dir (and linguistic/) at wake; P5 tomls now present + active.
- Full pipeline survival: Leg3 mint linguistic (P1) → op compress/diff/operad (P3/P4 via mcp_linguistic_calculus etc) → decompress → NREM (toml + promote/records) → ego.leg3 (verify/concept high CRS) with homotopy/text-coeff fidelity >=0.85.
- Dogfood: traces/relates to active goals; see Phase 6 e2e test in crates/engram-server/src/mcp.rs, MCP_TOOLS_REFERENCE.md, SUBSTRATE_WINS_PLAN.md.

These close the categorical/linguistic calculus loop for geometric self-model (non-flat).

## Phase 6: Public documentation (linguistic calculus)

- [`CATEGORICAL_LINGUISTIC_CALCULUS.md`](CATEGORICAL_LINGUISTIC_CALCULUS.md) — public overview, CRS gates, how to try.
- [`MCP_TOOLS_REFERENCE.md`](MCP_TOOLS_REFERENCE.md) — linguistic tool surface (P1–P5).
- E2e: `cargo test -p engram-server` (linguistic pipeline + CRS ≥ 0.85).
- Invariants unchanged: .leg3 layout, p-momentum on `update`, sheaf/H¹ from `processes/linguistic/*.toml`.