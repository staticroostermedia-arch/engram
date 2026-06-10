# Engram — First Run Guide

> **For new users and AI agents.** Run through this guide once when you first install Engram.
> After completing it, MCP is verified, your store is seeded, and you know the **8-tool lean contract**.

**Canonical reference:** [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md)

---

## 1. Install the Binary

```bash
git clone https://github.com/staticroostermedia-arch/engram.git
cd engram
cargo install --path crates/engram-server

engram --version
# engram-server 0.4.x
```

---

## 2. Configure Your MCP Client (Safe Defaults)

Add Engram to your IDE's MCP config. Use **`ENGRAM_PROFILE=agent`** (via `scripts/engram-grok`) — not the legacy 8-var env block.

```json
{
  "mcpServers": {
    "engram": {
      "command": "/path/to/Engram/scripts/engram-grok",
      "args": ["mcp"],
      "env": {
        "ENGRAM_STORE": "~/.engram/stalks/",
        "ENGRAM_PROFILE": "agent"
      }
    }
  }
}
```

See [`integrations/README.md`](integrations/README.md) for Grok, Cursor, Claude, Antigravity, and Codex.

**Restart your IDE** after changing MCP config.

> **Store location:** `~/.engram/stalks/` is the default. Override with `ENGRAM_STORE`.

---

## 3. Wake — One Call (Verify MCP Works)

In your AI agent, run:

```
mcp_engram_session_start(intent="First run — verifying Engram MCP connection")
```

Expected: inline JSON with `continuation_bundle`, `backend_readiness`, and `session_key`. Wake should complete in <2s even on large stores.

If this fails, check `engram --version` and that the MCP server appears in your IDE's tool list.

---

## 4. Store and Recall Your First Memory

```
mcp_engram_remember("first_run_test", "Engram is working. This is my first memory block.")
mcp_engram_recall("first memory working", k=3, scope="anchors")
```

Or via CLI:

```bash
engram --store ~/.engram/stalks/ remember first_run_test "Engram is working."
engram --store ~/.engram/stalks/ recall "first memory" --k 3
```

You should see `first_run_test` with score > 0.5.

---

## 5. Edit-Scoped Spatial (No Mandatory watch_workspace)

**Lean contract:** you do **not** need `watch_workspace` at first run or every wake.

Before editing a file, use:

```
mcp_engram_context_for_edit("/absolute/path/to/your/file.rs")
```

This returns file-scoped spatial context + related memories in one call.

**Deep mode only:** if you need passive daemon ingest across a whole project:

```
mcp_engram_set_memory_mode(mode="deep")
mcp_engram_watch_workspace("/absolute/path/to/your/project")
```

Or bulk-ingest once via CLI:

```bash
engram ingest /path/to/your/project
```

---

## 6. (Optional) Neural Embedding Server

Engram works out of the box with BLAKE3 hash encoding. For better semantic recall:

```bash
export ENGRAM_EMBED_URL="http://localhost:8086/v1/embeddings"
```

Add to your MCP `env` block. Without it, recall still works — paraphrased queries may score lower.

---

## 7. End Your Session (Handoff)

```
mcp_engram_session_end(summary="First run complete. MCP verified, first memory stored, lean contract understood.")
```

This produces a structured handoff packet. Your **next** `session_start` will surface it in the inline continuation bundle.

---

## Quick Reference — The 8 Essential Tools

| Tool | When |
|------|------|
| `session_start(intent)` | **First call every session** |
| `context_for_edit(path)` | Before editing a file |
| `recall(query, scope="anchors")` | When stuck; lean default |
| `quick_trace(decision, why)` | At decision forks |
| `remember(concept, text)` | New facts (recall first) |
| `session_end(summary)` | **Last call every session** |
| `get_backend_readiness()` | Check BVH/recall mode |
| `set_memory_mode("lean"\|"deep")` | Escalate for full recall |

**62 power tools** remain available — see [docs/MCP_TOOLS_REFERENCE.md](docs/MCP_TOOLS_REFERENCE.md). Do not call `watch_workspace`, `rebuild_bvh`, or `summarize` in lean mode unless needed.

---

## Common Failure Modes

| Symptom | Cause | Fix |
|---------|-------|-----|
| MCP OOM / duplicate processes | Bare `engram mcp` on large store | Use safe env (section 2) + restart IDE |
| Slow wake (>5s) | Deep tools at wake | Follow 8-tool contract; `ENGRAM_MEMORY_MODE=lean` |
| `context_for_edit` sparse | File never ingested | `engram ingest <path>` once, or deep `watch_workspace` |
| Low recall quality | No embedding server | Set `ENGRAM_EMBED_URL` |
| Lost context between sessions | Skipped `session_end` | Always end with structured summary |

---

## Next Steps

1. Load [SKILLS.md](SKILLS.md) + `docs/skills/engram-wake-up.md` into your agent instructions.
2. Read [docs/GROK_BUILD_MEMORY.md](docs/GROK_BUILD_MEMORY.md) for Grok Build integration.
3. Run `examples/hello-engram-agent.py` to see the lean loop.
4. For deep rituals: [docs/RITUALS.md](docs/RITUALS.md) + [HOW_WE_ACTUALLY_USE_THIS_IN_2026.md](HOW_WE_ACTUALLY_USE_THIS_IN_2026.md).

*First-run complete. Your manifold is ready.*