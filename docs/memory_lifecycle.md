# Engram Memory Lifecycle & NREM Pipeline

The geometric memory manifold within Engram does not behave like a standard CRUD database. It operates on a continuous, biologically-inspired lifecycle known as the **NREM (Non-Rapid Eye Movement) Pipeline**. This pipeline manages the ingestion, crystallization, stability evaluation, and eventual eviction (autophagy) of HolographicBlocks.

## 1. Waking Ingestion (The Senses)
When an agent or the background daemon ingests code or text, it creates a new HolographicBlock.
- **Encoding:** The text is transformed into an 8192-dimensional phase vector (`q` tensor) using the configured embedding model.
- **Spatial Binding:** For AST components, the physical line and column numbers are extracted and stored as 3D bounding boxes.
- **Initial Confidence:** The block starts with a Coherence-Reliability Score (CRS) of 1.0 (Maximum Confidence).

## 2. Active Session & Thermodynamic Drift
As an agent interacts with memory during an active session, it may use the `mcp_engram_update` tool to modify concepts. 
Instead of blind overwrites, Engram calculates the **Lyapunov Stability Drift** between the original and modified vectors.
- **Low Drift (Refinement):** If the agent slightly tweaks a function, the semantic drift is low. The CRS remains stable or increases.
- **High Drift (Contradiction/Hallucination):** If the agent fundamentally alters a load-bearing concept in a conflicting way, the system registers high drift and heavily penalizes the CRS.

## 3. The NREM Pipeline (Sleep & Consolidation)
When the session ends (`mcp_engram_session_end`), the system enters the NREM phase. During NREM, Engram processes the day's episodic and operational memories to consolidate knowledge.

### 3.0. Large-store bounds (`ENGRAM_PROFILE=agent`)
On stores with 100k+ blocks, NREM does **not** walk the full manifold. The agent profile defaults `ENGRAM_NREM_DISABLE=1` and uses a bounded candidate scan (`nrem_candidate_concepts`, cap ~8000) so `session_end` and subsequent MCP calls stay responsive. Deep consolidation remains available via explicit `engram distill` / deep-mode rituals — not on every lean handoff.

### 3.1. Distillation (Crystallization)
The `engram distill` command sweeps through recent EPISODIC and OPERATIONAL blocks, clustering them using DBScan over their `p` (momentum) and `q` tensors.
- Highly related problem/solution pairs are crystallized into new **ZEDOS_PRAXIS** blocks.
- These PRAXIS blocks represent permanent "lessons learned" and are often pinned (CRS=1.0).

### 3.2. Verification & Scarring
Hypotheses that are empirically tested (`mcp_engram_verify_behavior`) inform the NREM cycle.
- **Confirmed:** CRS is elevated, and the block is promoted.
- **Refuted:** The block's CRS is heavily penalized. If the failure is catastrophic, an `mcp_engram_scar` creates a geometric repeller, permanently bending the manifold space around the concept to prevent future agents from hallucinating the same approach.

## 4. Autophagy (Garbage Collection)
Unlike standard append-only logs, Engram prevents context-window bloat by periodically evicting low-quality or obsolete memories.
- Triggered manually via `mcp_engram_forget_old`.
- Blocks with a CRS below the defined threshold (e.g., 0.2) are permanently purged from the NVMe manifold.
- **Exceptions:** Pinned blocks (CRS=1.0) and recent session memories are mathematically exempt from autophagy.

## Summary
The NREM pipeline ensures the memory manifold remains a highly curated, geometrically stable foundation for autonomous agentic reasoning, automatically filtering out noise and preserving load-bearing architectural axioms.
