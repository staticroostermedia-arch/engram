# Personal Wiki Cookbook

**A practical guide for maintaining a compounding knowledge wiki across multiple chat sessions.**

No expertise required. Load the [engram-leg-wiki-starter](../docs/skills/engram-leg-wiki-starter.md) skill in your agent, open LEG Browser, and follow the pattern below.

---

## Before and after

**Before (flat notes):** Every chat starts with "remember my notes on X?" Context is re-derived. Five chats means five times re-explaining.

**After (Engram wiki):** Raw sources become structured tiles with relations. The next session rehydrates from anchors and momentum. Progress is visible in `./scripts/leg --live`.

---

## One-time setup

1. Build Engram: `cargo build -p engram-server`
2. Connect MCP (via `scripts/engram-grok` or your IDE plugin)
3. Create a folder for raw notes, e.g. `~/wiki-raw/`
4. Open LEG Browser: `./scripts/leg --live`
5. Give your agent the starter skill: `docs/skills/engram-leg-wiki-starter.md`

---

## Five-chat weekly flow

Use separate chats for separate jobs. All write to the same `~/.engram/` store.

### Chat 1 — Ingest a raw note

You have a new idea, clip, or meeting note. Save it as a markdown file.

**Agent does:**
1. `session_start` with intent "ingest today's note"
2. `context_for_edit` on the note's absolute path
3. `thought_tile_create` with `human_forward` leading (plain summary first)
4. `relate` raw file → tile (`synthesizes_from`)
5. `promote_hot` on the new tile
6. `session_end`

**You see:** New tile in LEG Browser recent sidebar.

### Chat 2 — Synthesize across sources

You found a related article or had a new insight.

**Agent does:**
1. `session_start` + `recall(scope="anchors")` for yesterday's tile
2. `query_with_momentum` to see what is trending
3. `thought_tile_create` for the synthesis
4. `relate` new tile → prior tile (`extends`)
5. `promote_hot` + `session_end`

**You see:** Relation edge between tiles in LEG graph view.

### Chat 3 — Review and lint

Weekly hygiene pass.

**Agent does:**
1. `session_start` + `recall` for wiki hub
2. `verify_manifold_integrity(min_crs=0.74)`
3. File a reflection tile noting gaps or contradictions (use `contradicts` relation for productive tensions)
4. `session_end`

**You see:** CRS health and any flagged issues.

### Chat 4 — Plan and index

Organize growing material.

**Agent does:**
1. `session_start` + `search_by_relation` from hub tile
2. `thought_tile_create` for an index/hub tile cataloging entries
3. `relate` entries → hub (`filed_back_into`)
4. `promote_hot` on hub + `session_end`

**You see:** Hub tile with backlinks in LEG block inspector.

### Chat 5 — Reflect and rehydrate

End-of-week reflection.

**Agent does:**
1. `session_start` — read continuation bundle
2. `recall(scope="anchors")` + `query_with_momentum`
3. `verify_manifold_integrity(min_crs=0.74)`
4. `session_end(prepare_compression=true)`

**You see:** Full week's evolution in LEG momentum sidebar and activity feed.

---

## Copy-paste snippet for any chat

Paste this into a new agent session after loading the starter skill:

```
Follow docs/skills/engram-leg-wiki-starter.md exactly.

Today's job: [ingest / synthesize / review / index / reflect]
Raw source (if ingesting): /absolute/path/to/file.md

Rules:
- context_for_edit on every source before work
- human_forward leads every tile payload
- relate raw → tile and tile → hub
- promote_hot on new tiles
- verify_manifold_integrity(min_crs=0.74) before session_end
- Open LEG Browser: ./scripts/leg --live
```

---

## Tips

| Situation | What to do |
|-----------|------------|
| Multiple topics in parallel | `set_namespace("topic-a")` per chat |
| Agent forgot prior context | `recall(scope="anchors")` on hub tile name |
| Tile already exists | `update` instead of `remember` (preserves history) |
| Review without agent | `./scripts/leg --live` — read-only, no MCP needed |
| Serve restart | `./scripts/restart-leg-serve.sh` (won't kill IDE MCP) |

---

## Success checklist

- [ ] Raw notes ingested via `context_for_edit`
- [ ] Every wiki entry has `human_forward` as first payload field
- [ ] Relations link raw → compiled and entries → hub
- [ ] `verify_manifold_integrity` passes at session close
- [ ] LEG Browser shows tiles, relations, and momentum in live mode

---

## Related docs

- [docs/PERSONAL_KNOWLEDGE_WIKI.md](../docs/PERSONAL_KNOWLEDGE_WIKI.md) — product overview
- [docs/skills/engram-leg-wiki-starter.md](../docs/skills/engram-leg-wiki-starter.md) — full MCP sequence
- [docs/LEG_BROWSER.md](../docs/LEG_BROWSER.md) — viewer guide
- [docs/AGENT_MEMORY_CONTRACT.md](../docs/AGENT_MEMORY_CONTRACT.md) — 8-tool lean loop