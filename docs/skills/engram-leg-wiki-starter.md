---
name: engram-leg-wiki-starter
---

# Engram LEG Wiki Starter — Agent Bootstrap Skill

**Copy-paste MCP sequence for bootstrapping and maintaining a personal knowledge wiki.**

Agents ingest raw sources, synthesize thought tiles, link them with relations, and hand off state so the next session compounds instead of re-derives. Review results in LEG Browser: `./scripts/leg --live`.

> **Lean contract:** [AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md) — 8 essential tools cover wake → work → handoff. This skill adds power tools for wiki compilation.

---

## Prerequisites

- Engram MCP connected (`scripts/engram-grok` or your IDE plugin)
- Store at `~/.engram/` (default) or `ENGRAM_STORE`
- LEG Browser for human review: `./scripts/leg --live`

---

## Session open (lean wake)

```
mcp_engram_session_start(
  intent="Personal wiki: [describe today's focus — ingest / synthesize / review]",
  include_spatial=false
)
```

Read `continuation_bundle.primary_goal` and `last_session_end.preview`. You are continuing — not starting fresh.

Optional namespace for multi-chat isolation:

```
mcp_engram_set_namespace(namespace="my-wiki-research")
```

---

## Anchor recall (find your wiki state)

```
mcp_engram_recall(
  query="personal wiki hub human_forward momentum",
  scope="anchors",
  k=8
)
```

---

## Ingest raw source (mandatory before any edit)

Call `context_for_edit` on **every** source file before reading or modifying it:

```
mcp_engram_context_for_edit(
  path="/absolute/path/to/your/note.md"
)
```

This spatially ingests the file and returns anchors, traces at locus, and spatial siblings.

---

## Synthesize wiki entry

Create a compiled tile from ingested material. **`human_forward` must be the first payload key** — plain narrative before technical detail.

```
mcp_engram_thought_tile_create(
  tile_type="knowledge_graph",
  title="my-topic-synthesis-2026-06",
  payload={
    "human_forward": "In plain terms: [what you captured, why it matters, so what for future sessions]",
    "key_facts": ["fact 1", "fact 2"],
    "provenance": "context_for_edit on note.md + thought_tile_create",
    "raw_source": "/absolute/path/to/your/note.md"
  },
  spatial_references=["/absolute/path/to/your/note.md"]
)
```

---

## Link with relations

```
mcp_engram_relate(
  concept_a="/absolute/path/to/your/note.md",
  concept_b="<tile_concept_from_create>",
  label="synthesizes_from"
)

mcp_engram_relate(
  concept_a="<tile_concept>",
  concept_b="my-wiki-hub",
  label="filed_back_into"
)
```

Common relation labels: `synthesizes_from`, `ingested_into`, `filed_back_into`, `extends`, `contradicts`, `child_of`.

---

## Promote for rehydration

```
mcp_engram_promote_hot(concept="<tile_concept>")
```

Hot tiles surface in LEG Browser sidebar and the next session's continuation bundle.

---

## Query evolving knowledge (deep, on demand)

When anchor recall is insufficient, escalate:

```
mcp_engram_set_memory_mode(mode="deep")

mcp_engram_query_with_momentum(direction="trending", k=5)

mcp_engram_search_by_relation(
  concept="my-wiki-hub",
  relation_type="filed_back_into",
  depth=2
)
```

Reset to lean before long meta sessions:

```
mcp_engram_set_memory_mode(mode="lean")
```

---

## Evolve existing entries (don't replace)

If recall finds an existing concept with score > 0.85, use `update` instead of `remember`:

```
mcp_engram_update(
  concept="<existing_tile>",
  new_text="Additional insight filed back: [plain-language delta]"
)
```

`update` preserves momentum and CRS history. Never delete-and-rewrite.

---

## Fork decisions (lean trace)

At any non-obvious choice:

```
mcp_engram_quick_trace(
  decision="[what you chose]",
  why="[reasoning in one sentence]",
  alternatives="[what you rejected]",
  would_falsify="[what would prove this wrong]"
)
```

---

## Lint before handoff

```
mcp_engram_verify_manifold_integrity(min_crs=0.74)
```

Healthy wikis pass with CRS ≥ 0.74 and zero violations.

---

## Session close

```
mcp_engram_session_end(
  summary="Wiki session: [ingested X, synthesized Y, related Z]. Next: [open action]. Files: [paths].",
  prepare_compression=true
)
```

---

## Full copy-paste loop

```
# 1. WAKE
mcp_engram_session_start(intent="Personal wiki bootstrap")

# 2. RECALL
mcp_engram_recall(query="personal wiki hub", scope="anchors", k=5)

# 3. INGEST (absolute path, every source)
mcp_engram_context_for_edit(path="/absolute/path/to/note.md")

# 4. SYNTHESIZE (human_forward leads)
mcp_engram_thought_tile_create(
  tile_type="knowledge_graph",
  title="first-wiki-entry",
  payload={
    "human_forward": "Plain thesis first: what, why, so what.",
    "key_facts": ["..."],
    "provenance": "context_for_edit + thought_tile_create"
  }
)

# 5. RELATE + PROMOTE
mcp_engram_relate(concept_a="/absolute/path/to/note.md", concept_b="<tile>", label="synthesizes_from")
mcp_engram_promote_hot(concept="<tile>")

# 6. LINT
mcp_engram_verify_manifold_integrity(min_crs=0.74)

# 7. HANDOFF
mcp_engram_session_end(summary="Bootstrap complete. Tile created and promoted.")
```

---

## Human review

Open LEG Browser to see what the agent wrote:

```bash
./scripts/leg --live
```

Live mode connects to `engram serve` on `:3456` and reads the same `~/.engram/` store as MCP.

---

## Related docs

- [PERSONAL_KNOWLEDGE_WIKI.md](../PERSONAL_KNOWLEDGE_WIKI.md) — product overview and layer mapping
- [LEG_BROWSER.md](../LEG_BROWSER.md) — viewer modes and architecture
- [AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md) — 8-tool lean contract
- [engram-thought-tiles.md](engram-thought-tiles.md) — tile types and mint triggers
- [examples/personal-wiki-cookbook.md](../../examples/personal-wiki-cookbook.md) — 5-chat maintenance walkthrough