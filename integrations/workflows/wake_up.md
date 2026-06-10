# Engram Wake-Up Protocol — Lean 8-Tool Contract

> **For agents:** Run at the start of every session or after reconnecting.
> **Canonical:** [docs/AGENT_MEMORY_CONTRACT.md](../../docs/AGENT_MEMORY_CONTRACT.md)

---

## Step 0: Verify MCP Connection

If your IDE shows Engram MCP tools, you're connected. If not:

```bash
engram --version
ls ~/.engram/stalks/
```

Ensure MCP config uses **safe defaults** on large stores — see `integrations/grok-build/mcp.json`.

---

## Step 1: Wake — One Call (MANDATORY)

```
mcp_engram_session_start(
  intent="<what you're working on today>",
  include_spatial=false
)
```

**This single call returns:**
- `continuation_bundle` — primary goal, last session_end preview, active artifacts
- `backend_readiness` — bvh_ready, recall_mode, leg_block_count
- `session_key` — epistemic anchor for this session

**Lean mode — you do NOT need:**
- `get_continuation_bundle` (redundant)
- `watch_workspace` at wake
- `summarize` / `query_pure` / `query_with_momentum`
- `promote_hot_batch` / `incremental_spatial_ingest`

### After the response

1. Read `continuation_bundle.primary_goal` and `last_session_end.preview`.
2. State continuation: *"I am continuing prior work on X; last session ended with Y."*
3. If you need full text on an artifact: `mcp_engram_recall(query="<keywords>", scope="anchors", k=5)`.
4. Proceed to work per [engram-working-memory.md](../../docs/skills/engram-working-memory.md).

**Target:** <2s wake, <500MB RSS on 181k+ stores.

---

## Step 2: Work (Per File)

Before editing any file:

```
mcp_engram_context_for_edit("/absolute/path/to/file.rs")
```

At decision forks:

```
mcp_engram_quick_trace(decision="...", why="...")
```

New facts only (recall first):

```
mcp_engram_remember("concept:name", "text")
```

---

## Step 3: Deep Mode (On Demand Only)

Escalate when lean bundle is empty or task needs full manifold navigation:

```
mcp_engram_set_memory_mode(mode="deep")
```

Then optionally:
- `mcp_engram_search_by_relation("<seed>", direction="both", k=8)`
- `mcp_engram_watch_workspace("/absolute/path/to/project")` — once per project if passive ingest needed
- `mcp_engram_verify_manifold_integrity(min_crs=0.74, sample_size=50)` — cold boot / high-stakes

Reset to lean before `session_end` on long meta sessions.

---

## Step 4: End Session (MANDATORY)

```
mcp_engram_session_end(summary="Decisions, files changed, open questions, next steps.")
```

---

## You're Ready (Lean Checklist)

- Session context bound (one `session_start`)
- Continuation bundle read (primary goal + last handoff)
- Working-memory discipline active (`context_for_edit` before edits)
- `session_end` planned for handoff

**Power tools** (62 total): [docs/MCP_TOOLS_REFERENCE.md](../../docs/MCP_TOOLS_REFERENCE.md)

**Grok Build integration:** [docs/GROK_BUILD_MEMORY.md](../../docs/GROK_BUILD_MEMORY.md)