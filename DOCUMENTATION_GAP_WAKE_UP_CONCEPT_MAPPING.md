# Engram Documentation Gap: Wake-Up Protocol & Concept Mapping / Relational Grounding

**Date:** 2026-05-25
**Status:** Proposed Updates
**Author:** Analysis by Grok (with human)

## Executive Summary

Engram currently has two layers of documentation that are out of sync:

1. **Public Surface / Canonical Protocol** (`integrations/workflows/wake_up.md` + README tool tables + basic AGENT_INTEGRATION_GUIDE "Correct Operating Protocol")
   - Focused on object loading: `session_start`, `summarize`, targeted `recall`, `watch_workspace`, `praxis` loading.
   - Does not mention custom multi-sorted ontologies (`conv:*`, `ops:*`, `vis:*`).
   - Does not elevate relational tools (`search_by_relation`, `visualize`) to the mandatory wake-up flow.
   - Does not address "concept mapping" or sheaf-like gluing between differently-typed concepts.

2. **Deeper / Codeland-Leaning Guidance** (AGENT_INTEGRATION_GUIDE.md sections on Domain Prefix Legend, sheaf zones, grounding_count rules, NREM promotion by prefix)
   - Already documents the exact ontology from the user's working image (`conv:task:`, `ops:hw:`, `conv:arc:`, etc.).
   - Talks about `sheaf_router::route_by_domain()` and prefix-based routing.
   - Contains "Graph Traversal over Grep" advice.
   - This thinking is **not reflected** in the public wake_up surface that agents are told to follow.

**Result:** When an agent follows the documented `/wake_up` (or the protocol in the AGENT_INTEGRATION_GUIDE), it gets good individual high-value blocks but does **not** reliably reconstruct the full relational "concept mapping" shown in working snapshots (the image with cross-cutting `conv:task`, `ops:hw`, `conv:vis`, etc.). The sheaf/functor/ops structure that Codeland is trying to ground remains implicit and fragile on the public Engram MCP surface.

This is the primary documentation gap preventing Engram from being the best memory MCP device.

## Evidence from Current Docs

### wake_up.md (the public canonical protocol)
- Steps focus exclusively on:
  - Connection verification
  - `session_start`
  - `watch_workspace`
  - `summarize` + keyword `recall`
  - Praxis recall
  - Daemon health
- Zero mention of:
  - `search_by_relation` / `visualize`
  - Custom prefixes or ontology
  - Reconstructing mappings between operational vs. conversational concepts
  - Sheaf zones or functorial lifting

### AGENT_INTEGRATION_GUIDE.md (deeper but still "agent-facing")
- Has an excellent "Domain Prefix Legend" table that matches the user's image almost 1:1.
- Explicitly references `monad_storage::sheaf_router` and sheaf zones.
- Has "Graph Traversal over Grep" as a token conservation rule.
- The "Correct Operating Protocol" section is nearly identical to wake_up.md and does **not** incorporate the prefix or relation thinking.
- The prefix/sheaf material appears later as advanced usage rather than core wake-up hygiene.

### README.md
- Documents the relation tools (`mcp_engram_relate`, `search_by_relation`, `visualize`) in the tool table.
- Does not connect them to the wake-up story.

## Proposed Updates (Concrete)

### 1. Update `integrations/workflows/wake_up.md` (Highest Priority)

Add a new mandatory step after "Rehydrate Your Context" and before "Load Praxis":

**New Step: Reconstruct Concept Mapping & Relational Grounding**

```markdown
## Step 3.5: Reconstruct Concept Mapping (MANDATORY for multi-layer work)

When your work spans multiple conceptual layers (operational hardware/network facts, conversational architecture arcs, active tasks, visions), explicitly reconstruct the mapping between them:

```bash
# 1. Discover the relational structure around your current focus
mcp_engram_search_by_relation("current active task or arc", direction="both", k=8)

# 2. Visualize the subgraph for the agent (and human) to see the gluing
mcp_engram_visualize("conv:arc:your_current_architecture", depth=2)
```

**Why this step exists**

Engram's `.leg` primitives support first-class relations via `OP_BIND` (stored as `ZEDOS_RELATION` blocks). The public wake_up must treat these relations as load-bearing for grounded reasoning, not optional power tools.

Custom ontologies (visible in working memory snapshots) use prefixes such as:
- `ops:hw:`, `ops:net:`, `ops:sw:` — operational grounding (hardware, networks, software state)
- `conv:arc:`, `conv:vis:`, `conv:task:` — conversational / architectural / visionary layer

A pure object-centric wake_up (summarize + keyword recall) loads the leaves but often misses the gluing. This step ensures the agent wakes up with the **coherent section**, not just a bag of high-CRS concepts.

**Integration with Codeland thinking**

This step is the Engram-surface expression of the deeper sheaf/functor model. Prefixes act as sites; relations act as the restriction maps / gluing data. `search_by_relation` + `visualize` let the agent (and human) inspect whether the local data is coherently glued into a global working picture.

Agents following only the old wake_up steps will repeatedly rediscover the same mapping work across sessions.
```

Also update the final checklist to include:
- ✅ Concept mappings and cross-layer relations reconstructed (via `search_by_relation` + `visualize`)

### 2. Update the "Correct Operating Protocol" in AGENT_INTEGRATION_GUIDE.md

Insert the new "Reconstruct Concept Mapping" step into the SESSION START block, and add a short note tying it to the Domain Prefix Legend.

### 3. Add a short "Multi-Sorted Ontologies & Sheaf-like Grounding" section (new or in AGENT_INTEGRATION_GUIDE)

Create or expand a section that:
- Documents the current working prefix ontology (lift the Domain Prefix Legend table to be more prominent).
- Explains that these prefixes are the Engram surface's way of supporting the sheaf zones that Codeland implements more deeply in `monad_storage`.
- Recommends that high-value cross-cutting work always includes explicit `OP_BIND` relations between `ops:*` and `conv:*` blocks.
- Positions `search_by_relation` and `visualize` as core wake-up hygiene for any agent that wants to avoid rediscovery on layered problems.

### 4. Minor README / Tool Reference Polish

In the MCP Tools table, add a short note under the Knowledge Graph tools:
> "Strongly recommended during wake-up when working with multi-layer concepts (see `wake_up.md` Step 3.5 — Concept Mapping)."

### 5. Optional but Recommended: Version the wake_up Protocol

Add a small "Protocol Version" header to wake_up.md (e.g., "Wake-Up Protocol v2 — Relational Grounding Edition") so it is clear that following the latest version includes the mapping reconstruction step.

---

## Why This Matters for "Best Memory MCP on the Market"

If Engram's public surface (the MCP tools + the documented wake_up agents are told to follow) does not reliably deliver the "complete working picture" that experienced users actually maintain (the image with its rich `conv:` / `ops:` mapping), then Engram will be perceived as "just another memory store with some extra features" rather than the best substrate for grounded, long-lived agent cognition.

Closing this gap makes the public Engram surface honest about the deeper structure that Codeland is exploring, while still keeping Engram as the clean, MCP-first, agent-usable layer.

## Next Actions (Human + AI)

- [ ] Review and refine the proposed text for wake_up.md Step 3.5
- [ ] Apply the edit to wake_up.md
- [ ] Sync the change into AGENT_INTEGRATION_GUIDE.md "Correct Operating Protocol"
- [ ] Decide whether to lift more of the sheaf/prefix material into the public surface or keep it in the deeper guide
- [ ] Consider adding a small example in the docs that uses a snapshot similar to the user's working image

This document itself can be deleted or archived once the updates are applied.
