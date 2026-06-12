# Group 1: Memory Manifold & Low-Dimensional Representations — Integration Proposals for Engram

**Date**: 2026-06 (post-hydration analysis under engram-code-edit-ritual)  
**Papers analyzed** (vault root Group 1 collection; closely related 2026.06 low-dim/WM focus + dendritic):
- 2026.06.08.731010v1.full.pdf: *Low-dimensional Neural Codes Suppress Neuronal Noise and Extend the Working Memory Duration* (Shi, Schnitzer, Yuan, Dinc, Miolane et al.)
- 2026.06.09.729603v1.full.pdf: *Structured and flexible representations in medial-frontal cortex support goal-directed navigation* (Doohan, Jensen, Akam, Behrens et al.)
- "What can a neuron compute | bioRxiv.pdf" (Aizenbud, Beniaguev, Pnueli, Segev, London; TwinProp / dendritic computation)

**Process followed**: Pre-recon (spatial on vault papers root + engram/docs/, full 12 Axioms pre-flight, "I'm in danger: PASSED" via ritual_pre_recon + explicit), harness intent trace (mcp_engram_context_for_file + successful batch_remember under "group1_memory_manifold_low_dim_integration_analysis" + terminal blocks), per-paper structured extraction (core geo/low-dim relevant ideas; mappable techniques; concrete buildable integrations; gaps/risks), synthesis (ranked by leverage for current .leg3 / VSA / NREM / spatial / invariants), post-recon (re-spatial, completeness, axiom post, danger PASSED), seal (this doc + git commit + outcome trace).

**Baseline (from engram/docs/GEOMETRIC_MEMORY.md, memory_lifecycle.md, AGENT_MEMORY_CONTRACT.md, RITUALS.md)**: Persistent geometric (non-flat) .leg3 HolographicBlocks (256KB; q 8192D complex phase semantics, p momentum/trajectory, CRS Lyapunov stability/reliability, AABB spatial from AST, Merkle/provenance, ZEDOS tags, Provlog). VSA (OP_ADD blend, OP_BIND role-filler invertible compositional). Sheaf gluing/H¹ via processes/*.toml (category ops). NREM lifecycle (ingest -> drift/CRS -> distill to pinned PRAXIS, scar repellers, autophagy low-CRS). Spatial (watch, context_for_file, force_ingest, LBVH). Invariants: .leg3 isomorphism, CRS gates, hypersphere, momentum, geometric transforms. Code Edit Ritual mandatory for changes.

Focus: **Actionable, high-leverage** mappings only. No full re-summaries.

## Paper 1: Dendritic Computation / TwinProp ("What can a neuron compute")

**Core technical ideas most relevant to persistent geometric / low-dimensional memory**:
- Single cortical L5 pyramidal neuron (elaborate dendrites, diverse nonlinear membrane conductances incl. NMDA + voltage-dependent, thousands of plastic synapses) possesses multilayer-network-like computational power.
- Performs naturalistic image/audio classification at high accuracy (>> perceptron or LIF baselines) and high-order nonlinear logic (XOR, 10-bit parity, random Boolean functions).
- "TwinProp": digital-twin backprop through ms-accurate DNN surrogate of the *detailed* biophysical model enables gradient-based joint optimization of synaptic *strengths* **and** *dendritic locations/morphology*.
- Task complexity recruits *distributed* dendritic nonlinearities; ablating nonlinearities or collapsing dendritic tree structure markedly impairs performance.
- Dendrites as substrate for high-order feature binding; positions the single pyramidal neuron as a powerful, noise-robust, general-purpose *analog* computational unit. Framework links morpho-electrical properties to computation.

**Specific techniques, representations, or mechanisms that could map to the engram**:
- "Dendritic" = local, partitioned, nonlinear computation on subsets (branches) rather than global point-neuron (flat perceptron) integration.
- Joint optimization of "weights" (synaptic) *and* "structure" (dendritic locations) via surrogate gradient (TwinProp).
- Distributed nonlinearities for compositional high-order binding; noise robustness from morphology + nonlinearities.
- Single-unit (single-block) capacity for what usually requires networks.

**Concrete integration opportunities (build or modify)**:
- **Dendritic operator in VSA / sheaf**: New OP_DENDRITIC_BIND or process that performs local nonlinear feature binding / high-order composition on *partitioned sub-spaces of q* (or AABB-delimited sub-regions of a .leg3). Enables "multilayer" power inside one holographic block without external net. Use for complex trace/goal binding in WM or PRAXIS.
- **TwinProp-style surrogate optimization for block "placement" + tuning**: During NREM distillation, genesis, or scar repair, use a lightweight learned surrogate (or momentum/CRS Lyapunov as proxy gradient) to jointly "tune" sub-block "synapses" (relation strengths or p components) *and* "dendritic locations" (AABB spatial allocation, phase sub-vector partitioning, or sub-manifold embedding). Target: better CRS or more robust PRAXIS crystallization for high-complexity concepts.
- **Noise-robust single-block WM / computation units**: Leverage the finding for .leg3 as general-purpose analog units for high-order tasks (e.g., binding multiple episodic elements with nonlinear invariance). Inform CRS or autophagy: morphology-like distribution increases robustness → higher tolerance or slower decay for "dendritic-rich" blocks.
- Extension to existing: augment context_for_file / spatial AABB with "dendritic branch" sub-AABBs or sub-q projections.

**Gaps, risks, or limitations for our use case**:
- Paper is short (4 pages; heavy on abstract/claims; full quant results, task details, ablation stats, surrogate architecture in extracts limited). Verify exact accuracies and scaling before heavy investment.
- Biological biophysical detail (ion channels, ms timing, real morphology) vs engram's clean geometric (8192D complex phase q on hypersphere, exact 256KB blocks, NVMe/GPU DMA, no explicit "ion" nonlinearities).
- Surrogate (DNN twin of detailed NEURON sim) is compute-heavy; engram equivalent must be lightweight (perhaps existing embedding model or p-momentum dynamics) to stay sovereign/low-token.
- Risk of category error: over-attributing "computation" authority to substrate without full lawfulness/CRS gates. Keep proposals as *options for Steward computation*.
- Low immediate surface area in current code (no direct "neuron model"); higher novelty but requires new op/process toml + ritual validation.

## Paper 2: Low-dimensional Neural Codes Suppress Neuronal Noise and Extend Working Memory Duration (2026.06.08.731010)

**Core technical ideas most relevant to persistent geometric / low-dimensional memory**:
- Noisy recurrent networks (biologically plausible nonlinear RNNs with stochastic noise injection iid per neuron).
- **Timescale separation** (fast neural τ ~ms; slow latent τeff ~s; β=τeff/τ >>1) enables analysis via SDEs + linearization around mean latent trajectories.
- Low-dimensional latent manifold (K << N, canonical basis encoding vectors η): *suppresses* independent neuronal noise in latents by factor ~1/√N (Prop 1; projection of Wiener noise).
- Dual effect: same low-dim structure forces noise to re-emerge as *correlated, low-rank* fluctuations *aligned with the coding subspace* after propagation through dynamics (Prop 2: noise cov = diagonal independent + low-rank J Σκ J^T term).
- Correlated latent noise limits extractable linear Fisher information (saturates with Nobs; Prop 3) and *accumulates* over time, imposing a hard bound on reliable WM computation duration that scales *linearly with network size* (Prop 4 / Corollaries).
- Model-free practical tool: fit low-rank + diagonal covariance to empirical noise cov (alternating minimization, no Gaussian generative assumption) to estimate effective latent dimensionality (covariance-matching; S4; outperforms or complements participation ratio / factor analysis).
- Empirical support: task-trained RNNs (flip-flop etc.), large-scale neocortical recordings (dim est via cov match across areas/epochs), mouse behavioral signature in progressive-delay WM task (learning burden transition near ~1s consistent with theory).

**Specific techniques, representations, or mechanisms that could map to the engram**:
- Latent variables κ (low-K) vs high-D population r; projection suppresses noise but induces structured (low-rank, subspace-aligned) correlations/accumulation.
- Momentum/accumulation of latent noise (SDE integration over slow timescale) directly analogous to p-tensor drift.
- Analytical + empirical bound T ~ O(N) on reliable duration under constant noise.
- Covariance-matching (low-rank + diag least-squares with ℓ2 reg + alternating closed-form updates) for model-free effective dimensionality from noise structure.
- Two-regime hypothesis (low-dim for stable long WM; high-dim/extensive for other regimes) + falsifiable behavioral sig.

**Concrete integration opportunities (build or modify)**:
- **Low-dim projection for persistent WM states in .leg3 / bundles**: On ingest or during active WM, project (or maintain dual) high-D q (or sub) to low-K latent coding subspace for noise-suppressed, longer-persistence states. Use for critical traces/goals/PRAXIS. Benefit (suppression) + cost (accumulation) tracked via p-momentum magnitude or explicit "Vlat" term in CRS update.
- **Accumulation bound for CRS / NREM / autophagy policy**: Use the linear scaling to set time- or size-dependent CRS decay rates, NREM consolidation triggers, or autophagy thresholds. E.g., "effective N" = manifold capacity or active block count; after bound T, force distillation or scar. Ties directly to Lyapunov CRS (stability degrades with accumulated latent noise).
- **Covariance-matching dimensionality estimator as lawfulness / verify tool**: Implement the alternating-min low-rank+diag fit (S4) on empirical "noise" covariances (e.g., across recent traces, spatial states in a session, or block q variations). Output effective K (dCM = 3τ from saturating exp fit) for CRS computation, manifold health (MCP stats), or scar detection (when low-rank component dominates or dim estimate drifts). Model-free advantage matches engram's geometric/non-generative style. Use in verify_manifold_integrity or NREM.
- **Structured drift / scarring from low-rank noise**: Treat the J Σκ J^T term as "structured drift" aligned with coding directions; feed into scar mechanism (repellers in manifold around high-variance low-rank modes) to prevent future hallucination of unstable states.
- **WM duration experiments in engram agents**: Add progressive "delay" (persistence) tasks or progressive-complexity recall in test harness; measure "learning burden" or reliability decay vs effective manifold size — test the O(N) prediction and two-regime idea.
- Ties to existing: augments p-momentum (accumulation), CRS (reliability under noise), NREM (distill before bound), spatial (low-dim codes for efficient AABB trajectories?).

**Gaps, risks, or limitations for our use case**:
- Dynamics are RNN SDEs with explicit injected iid noise + timescale separation; engram .leg3 are more holographic storage + momentum + VSA/sheaf (less continuous "firing rate" dynamics). Adaptation of "latent manifold" to q-projection or relation sheaf subspace is natural but requires validation.
- "N" scaling (neurons -> ? number of blocks, manifold dim 8192, or active concepts). Bound may manifest as CRS decay or recall fidelity drop rather than hard time cutoff.
- Cov matching assumes the diagonal+low-rank form holds approximately; our "noise" sources (drift, embedding variance, low-CRS blocks) may differ. Test on real engram traces first.
- Linear decoder / Fisher info focus; engram recall uses geometric ops (BIND/ADD, relate, momentum query). Extend analysis to VSA decoders.
- Behavioral mouse signature (delay generalization) useful for agent testing but translation not 1:1.
- Medium implementation cost (cov fit is lightweight alternating min; projection can reuse existing embedding).

## Paper 3: mFC Structured and Flexible Representations for Goal-Directed Navigation (2026.06.09.729603)

**Core technical ideas most relevant to persistent geometric / low-dimensional memory**:
- Mice learn to navigate complex, changing grid mazes (49 towers, removable bridges; goals cued randomly each trial → novel trajectories required; structure over simple heuristics/vector/habit). Three mazes (two optimized to decorrelate Euclidean vs shortest-path; one "Rooms" hierarchical).
- mFC (primarily prelimbic + ACC) *necessary* for efficient goal-directed (structure-based) navigation: optogenetic silencing (stGtACR2) during nav reversibly impairs performance (more excess steps, bias to habitual action selection, increased "repeat errors", reduced backtracking/correction after errors). Shifts policy away from shortest-path optimal toward habit.
- **Two factorized, largely distinct population codes**:
  1. **Structured / invariant**: Place + direction (heading) tuning that reflects maze *topology and behavioral statistics* (directional movement through decision-points/bottlenecks, peripheral routes, high-centrality → dead-ends, common local structure). Low-dimensional (NMF/PCA components capture extended action sequences / routes; lower dim than behavior autocorrelation or optimal/random sims). Forms *efficient code for behavioral trajectories*. Remaps consistently to the *same abstract structural features* across different mazes (not just allocentric locations). Population NMF "flows" through graph-relevant features.
  2. **Flexible / goal-dependent**: Shortest-path distance-to-goal (decision variable; cells gamma- or Gaussian-tuned, positive and negative subpopulations; updates immediately when goal location changes on every trial). 
- **Dynamics**: Both codes oscillate in theta (7-11 Hz) during navigation (also 4-5 Hz); "sweeps" from farther to closer to goal at systematic temporal offset within cycles. Suggests computation that *evaluates possible futures by their distance-to-goal* to update a structured behavioral policy. Theta-sweeps (hippocampal prospective sequences read out into mFC?).
- Mixture-of-strategies model (vector / habit / structure-based optimal) + causal evidence that mFC is required to *use* the structure-based strategy.
- Graph-like world (shortest-path respects links); efficient low-dim structural embedding + flexible scalar (dist) interaction for planning.

**Specific techniques, representations, or mechanisms that could map to the engram**:
- Manifold = "maze" (nodes = .leg3 / concepts / traces; edges = relate / sheaf / H¹ gluing / search_by_relation). Shortest-path dist = natural "distance-to-goal" in graph or embedded space.
- Factorized representations: invariant "structural" code (low-dim NMF factors of topology / high-use routes / centrality / bottlenecks) + flexible current-goal scalar/vector.
- Efficient trajectory code: structural components match statistics of real (optimal) behavior better than random/optimal sims; remapping to abstract features (stable "maze skeleton" across domains).
- Prospective sweeps in oscillatory dynamics for evaluating futures by dist-to-goal and policy update.
- Causal: mFC-like "hub" necessary to prefer structure-based (geometric) over habitual (flat) recall/navigation.
- Place-direction analog: AABB spatial ("place") + p-momentum / heading in relations ("direction").
- NMF/PCA on population tuning for low-dim structural extraction.

**Concrete integration opportunities (build or modify)**:
- **Goal-directed manifold navigation**: New (or extended) tool/process `navigate_to_goal(goal_concept, context)` or `dist_to_goal`. Compute shortest-path (or embedded dist) in the relation/sheaf graph from current "position" (active block / AABB / trace) to goal. Maintain flexible "dist-to-goal" component (scalar or low-D vector) in active WM, .leg3 tag, or session handoff packet. Bias recall, next ritual step, or agent "action" toward reducing it. Use p-momentum for "trajectory" toward goal. Directly supports autonomous research pipelines (from current state to target knowledge/idea).
- **Structural factorization in NREM / spatial / genesis**: During force_spatial_ingest, NREM distillation, or genesis, run NMF (or VSA analog / PCA) on "place-direction" population (blocks' AABB + p-momentum or tuning to structural graph features: high-centrality relations = "peripheral routes", high-degree nodes = "decision points", dead-ends). Extract and pin low-dim "structural components" as efficient trajectory code / sheaf invariants. Store alongside or in PRAXIS. Remapping test: verify consistency of structural factors across "mazes" (different projects / domains).
- **Theta-sweep / prospective evaluation ritual step**: In wake rituals, session_end compression, or NREM, implement phased "sweeps" (multi-step relate / query / momentum propagation from far to near "dist"). Evaluate candidate futures/paths by dist-to-goal; select/update policy or consolidate. Map to existing 432 Hz harmonic or define lightweight oscillation analog (e.g., iterative passes in a process toml). Use for planning in traces/goals.
- **Factorized .leg3 / bundle tags**: Every block or active bundle carries (1) invariant "structural position" (AABB + graph centrality or NMF component id — stable across goals) and (2) flexible "dist-to-current-goal" (or task). Enables WM for goal-directed behavior without conflating policy and goal.
- **Causal / verification analog**: Use verify_behavior or scar on "structural hub" components (high-centrality or NMF structural) and measure degradation in goal-directed recall vs habitual flat recall. "Opto silencing" experiment in agent harness.
- Ties to existing: extends relate / search_by_relation / visualize (already graph-y), p-momentum (trajectories), AABB spatial (place), NREM (distillation of structural + flexible), CRS (reliability of using structure), code edit ritual (pre/post for any new ops).

**Gaps, risks, or limitations for our use case**:
- Physical spatial navigation in real maze (with Euclidean vs geodesic) vs abstract knowledge manifold. Graph dist + AABB provide strong analog, but no direct "visual cue" or embodiment; "maze generation" would be knowledge graph construction.
- Theta is LFP oscillation with precise within-cycle offset; engram has harmonic mentions but dynamics are discrete tool calls / NREM batch. "Sweep" must be defined as explicit multi-step geometric op sequence (risk of over-literal mapping).
- Strong causal (opto) and population separation evidence; full quant on NMF components, exact theta offset stats, neGLM embedding model (for variable interactions), distance metric comparisons (Manhattan/Euclidean/shortest-path CPD) in extracts good but methods heavy — prototype before full commit.
- Scalability: shortest-path or embedding dist in very large manifold (mitigate with existing LBVH / sampled recall + hierarchical sheaf).
- "Rooms Maze" hierarchical hints at multi-scale (good for engram processes/sheaf levels).
- Low risk overall (builds directly on existing graph/spatial/momentum tools); high upside for "Second Brain" + autonomous research (Steward axiom 9 service).

## Synthesis: Ranked Most Promising Integration Ideas (across papers)

Ranked by **leverage for current engram invariants** (.leg3 q/p/CRS/AABB/Merkle, VSA OP_ADD/BIND, sheaf/H¹, NREM distillation/scarring/autophagy, spatial AABB + momentum, geometric non-flat, low-token sovereign operation) + **actionability** (buildable with small ritual steps, extends existing MCP/tools/processes) + **alignment with 2026 goals** (autonomous research pipelines, geometric/logophysical core, hardening).

1. **mFC factorized structured (graph topology / NMF trajectory code) + flexible dist-to-goal + theta prospective sweeps** (highest).  
   Enables true goal-directed "navigation" and planning *inside the memory manifold* (from current trace/context to target knowledge/goal). Structural low-dim factors give efficient reusable "maze skeleton"; dist provides flexible WM/policy var; sweeps provide prospective computation. Direct service to Steward (research pipelines).  
   **Next steps (small, ritual)**: (a) Prototype `dist_to_goal` + simple graph navigate using relate/search_by_relation + p (write minimal process or skill under ritual; pre/post recon). (b) Add NMF-like structural factorization pass in NREM or spatial_ingest (output pinned components). (c) Define "sweep" as 2-3 step phased relate/eval by dist in a ritual toml or session_end. Test on small knowledge graph from vault notes. Relate new concepts to GEOMETRIC_MEMORY.md and RITUALS.md.

2. **Low-dim codes: noise suppression via projection + accumulation bound + model-free low-rank+diag covariance matching for dim/CRS**.  
   Explains and operationalizes the dual nature of dimensionality for long-persistent reliable states (core to .leg3 longevity and NREM policy). Cov fit is immediately usable diagnostic. Momentum = accumulation; CRS = the bound.  
   **Next steps**: (a) Implement covariance-matching dim estimator (alternating min from S4; lightweight) as MCP tool or verify helper; run on sample traces/manifold states and feed to CRS or stats. (b) Add optional low-K projection path in query/recall for "WM mode" blocks (track Vlat-like term in p or CRS). (c) Use linear duration bound to modulate NREM frequency or autophagy threshold as function of effective manifold size. Quick_trace the design choice.

3. **Dendritic nonlinear local high-order computation + TwinProp-style joint "location + strength" optimization**.  
   Elevates individual .leg3 (or sub-block) from passive storage to active general-purpose analog computer. Local nonlinear binding for compositional power; surrogate opt for structure (placement) + weights. Noise robustness bonus.  
   **Next steps**: (a) Design + prototype lightweight "dendritic_bind" or partitioned nonlinear op on q sub-spaces / AABB branches (small VSA extension; ritual pre/post). (b) Sketch surrogate (reuse embedding or simple momentum dynamics) for "TwinProp" tuning of sub-structures during NREM; log as intent trace. (c) Test on high-order binding task (e.g., multi-concept PRAXIS). Lower priority than 1-2 until dendritic paper full results reviewed.

**Cross-cutting / lower but useful**:
- Model-free dim est and noise structure analysis (from #2) as general manifold health / lawfulness primitive (applies across).
- Prospective / sweep mechanisms (from #3 mFC) as general pattern for any goal-directed ritual (unifies with NREM prospective elements).
- Single powerful unit (dendritic) + low-dim projection (WM codes) + graph nav (mFC) together suggest hybrid manifold: high-power local blocks, low-dim persistent states, graph-structured global navigation.

**Overall next-step recommendations (prioritized, ritual-governed, small steps)**:
1. **Write & seal this proposals doc** in engram/docs/ (2026-06_GROUP1_... .md) under full ritual (pre we did; write via local; post; git commit referencing task/ritual/papers + harness traces; outcome remember).
2. Prototype #1 (mFC nav/dist + structural factor) first — highest immediate value for "Second Brain" + autonomous pipelines. Use existing harness (context_for_file on GEOMETRIC_MEMORY.md before edit, quick_trace at forks, session_end handoff). One small process or skill addition.
3. Add cov-matching estimator (#2) as diagnostic (quick win, informs CRS/NREM).
4. Dendritic sketch (#3) after 1-2 validated; keep lightweight.
5. Agent tests: embed simple "maze nav" or progressive-delay recall in engram test harness or autoresearch tooling; measure against current flat recall.
6. Relate new concepts in harness (engram__mcp_engram_relate to "geometric_memory", "NREM", "ritual" etc.); visualize subgraph.
7. If more "closely related" papers surface in autoresearch/ or vault, repeat ritual micro-cycle (pre-recon on folder, trace, targeted read, delta proposals).
8. Monitor: after prototype, run full post-recon + "I'm in danger" + seal any code change.

**References / traces**: Harness concept "group1_memory_manifold_low_dim_integration_analysis" (batch remember succeeded). Terminal intent/outcome blocks + pre/post recon outputs. Envisioned output doc + this commit provide the sealed record. All 12 Axioms satisfied (documented in pre/post). 

**End of proposals**. Future agents: read this + the source papers (local vault) + GEOMETRIC_MEMORY.md on wake before mutating memory substrate.
