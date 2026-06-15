# Personal Knowledge Wiki

**A compounding knowledge base that grows across sessions — not a per-query RAG scratchpad.**

Engram gives you a personal wiki on a lawful geometric substrate (`.leg3` blocks in `~/.engram/`). Agents ingest raw sources, synthesize structured tiles, link them with typed relations, and evolve entries without losing history. You review the result in **LEG Browser** — a local viewer for your store.

---

## The problem with flat notes

Traditional note apps and chat-based RAG share a failure mode: every new session re-discovers context from scratch. Notes sit in folders; agents summarize and forget. Five chats about the same topic means five times re-explaining yourself.

A personal wiki should **compound** — raw material becomes synthesized pages, pages link to each other, and the next session picks up where the last one left off.

---

## How Engram maps the pattern

| Layer | Karpathy-style wiki | Engram equivalent |
|-------|---------------------|-------------------|
| **Raw sources** | `raw/` — immutable notes, clips, code | Any file ingested via `context_for_edit` (spatial AABB + anchors) |
| **Compiled wiki** | `wiki/` — LLM-synthesized `.md` with backlinks | **Thought tiles** — structured blocks with `human_forward` narrative first, then facts, provenance, and relations |
| **Schema / playbook** | `schema/` — agent instructions | This doc + [engram-leg-wiki-starter.md](skills/engram-leg-wiki-starter.md) |
| **Viewer** | Obsidian, graph, search | **LEG Browser** — `./scripts/leg --live` |
| **Ops** | ingest, compile, query, lint | ingest → synthesize → query → verify |

---

## Core workflow

### 1. Ingest raw material

Before touching any source file, call `context_for_edit` on its absolute path. This performs spatial ingest and returns anchors, traces, and edit context — your raw layer enters the manifold with file:line precision.

### 2. Synthesize into tiles

Compile sources into **thought tiles** via `thought_tile_create`. Every wiki entry must lead with `human_forward` — a plain-language thesis (what happened, why it matters, so what) before technical detail.

### 3. Link with relations

Connect raw sources to compiled tiles and tiles to each other:

- `synthesizes_from` / `ingested_into` — raw → compiled
- `filed_back_into` — insight loops back to a hub
- `extends` / `contradicts` — productive tensions preserved, not flattened

### 4. Evolve, don't replace

Use `update` on existing concepts to preserve momentum and history. Avoid delete-and-rewrite patterns that annihilate prior reasoning.

### 5. Review in LEG Browser

```bash
./scripts/leg --live
```

Live mode reads your `~/.engram/` store through `engram serve`. You see recent tiles, momentum trends, relation graphs, and the activity feed — what your agents actually wrote, not what chat logs claim.

### 6. Lint at session close

Before `session_end`, run `verify_manifold_integrity(min_crs=0.74)` to catch lawfulness violations. Healthy wikis maintain CRS ≥ 0.74 across the cluster.

---

## Multi-chat maintenance

One chat for research ingestion, another for synthesis, a third for review — all writing to the same `~/.engram/` store. Each session:

1. `session_start` — rehydrate from continuation bundle
2. `recall(scope="anchors")` — find your wiki hub and recent tiles
3. Do one focused operation (ingest, synthesize, or review)
4. `session_end` — hand off structured state to the next chat

For logical separation within a shared store, set a namespace at session open: `set_namespace("research")` vs `set_namespace("synthesis")`.

See [examples/personal-wiki-cookbook.md](../examples/personal-wiki-cookbook.md) for a concrete 5-chat walkthrough.

---

## Agent bootstrap

Give your agent the starter skill: [docs/skills/engram-leg-wiki-starter.md](skills/engram-leg-wiki-starter.md). It contains the exact MCP sequence — wake, ingest, synthesize, relate, verify, handoff — aligned with the [8-tool lean contract](AGENT_MEMORY_CONTRACT.md).

Power tools (`thought_tile_create`, `query_with_momentum`, `search_by_relation`, `promote_hot`) extend the lean loop when compiling and navigating wiki entries.

---

## Before and after

| Flat notes / RAG | Engram personal wiki |
|------------------|----------------------|
| Re-derive context every chat | Continuation bundle + anchor recall |
| Markdown files in folders | Spatially indexed `.leg3` blocks |
| Manual backlinks | Typed relations with graph in LEG |
| Summarize and forget | `update` preserves momentum and history |
| No lawfulness gate | CRS ≥ 0.74 verify at close |

---

## Related docs

- [LEG_BROWSER.md](LEG_BROWSER.md) — viewer modes, architecture, beta caveats
- [AGENT_MEMORY_CONTRACT.md](AGENT_MEMORY_CONTRACT.md) — 8 essential tools
- [skills/engram-leg-wiki-starter.md](skills/engram-leg-wiki-starter.md) — copy-paste agent bootstrap
- [skills/engram-thought-tiles.md](skills/engram-thought-tiles.md) — tile types and requirements
- [examples/personal-wiki-cookbook.md](../examples/personal-wiki-cookbook.md) — 5-chat maintenance example