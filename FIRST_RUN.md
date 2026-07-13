# Engram — First Run Guide

> **For new users and AI agents.** Run through this once on a fresh install.
> After it, MCP works, your store is seeded, and the **8-tool lean contract** is proven.

**Default agent load set (exactly two docs after MCP works):**  
1. [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md) — 8-tool highway + composites  
2. [docs/skills/engram-wake-up.md](docs/skills/engram-wake-up.md) — one-call wake protocol  

Do **not** require five other guides at first touch. Power maps and theory are optional later.

### Who does what?

| Role | Steps |
|------|-------|
| **Human** | §1 Build · §2 MCP config in IDE · restart IDE · optional §6 embeddings · optional `./scripts/leg --live` to review memory |
| **AI agent** | After human completes §2: load the **two docs above** only · §3 `session_start` · §4 first `remember`/`recall` · §5 edit via composites (below) · §7 `session_end` every session |

**Paste to your agent (after §2):**

```
Engram MCP is configured.
Default load (only these two): docs/AGENT_MEMORY_CONTRACT.md + docs/skills/engram-wake-up.md.
Run mcp_engram_session_start(intent="First session on Engram").
Execute suggested_actions, then mcp_engram_ack_wake_queue(executed=true).
8-tool loop. Prefer mcp_engram_safe_edit_and_verify for code edits and
mcp_engram_update_with_tensor_bond for verified memory updates (not raw multi-step only).
Do not call watch_workspace, rebuild_bvh, or summarize at wake.
End with mcp_engram_session_end(summary=...).
```

---

## 1. Install the Binary

```bash
git clone https://github.com/staticroostermedia-arch/engram.git
cd engram
cargo build -p engram-server

target/debug/engram --version
# engram 0.7.0-beta.5
```

---

## 2. Configure Your MCP Client (Safe Defaults)

**One MCP process per store.** Only one `engram … mcp` may hold the flock on a given `ENGRAM_STORE` (e.g. `~/.engram/stalks/`). A second launch exits non-zero and names the **holder PID** — restart the IDE/TUI or stop the other process. Do not run harness and TUI MCP against the same store concurrently. Dev recovery: `ENGRAM_MCP_FORCE_STEAL=1` (dead PID / orphan only). Repro: `scripts/repro-mcp-lock.sh`.

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

Expected: JSON with `bundle_tier: "slim"`, `continuation` (primary goal + top 5 `suggested_actions`), `readiness`, and `session_key`. Wake should complete in under ~8s on large stores. Use `mcp_engram_get_continuation_bundle` when you need the full harness inline.

Then acknowledge the wake queue (required with `ENGRAM_PROFILE=agent`):

```
mcp_engram_ack_wake_queue(executed=true, note="first run wake queue")
```

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
| `context_for_edit(path)` | Before editing a file (or use composite below) |
| `recall(query, scope="anchors")` | When stuck; lean default |
| `quick_trace(decision, why)` | At decision forks |
| `remember(concept, text)` | New facts (recall first) |
| `session_end(summary)` | **Last call every session** |
| `get_backend_readiness()` | Check BVH/recall mode |
| `set_memory_mode("lean"\|"deep")` | Escalate for full recall |

### Preferred composites (use these for edit / update)

| Composite | Prefer over | When |
|-----------|-------------|------|
| `mcp_engram_safe_edit_and_verify` | raw `context_for_edit` → `quick_trace` → `update` chain | Substantive code edits |
| `mcp_engram_update_with_tensor_bond` | raw `update` alone | Verified memory writes / tile sync |

**~78 power tools** remain available (**86 total** — `tool_list()` in `mcp.rs`) — see [docs/MCP_TOOLS_REFERENCE.md](docs/MCP_TOOLS_REFERENCE.md). Do not call `watch_workspace`, `rebuild_bvh`, or `summarize` at wake.

---

## Common Failure Modes

| Symptom | Cause | Fix |
|---------|-------|-----|
| MCP OOM / duplicate processes | Bare `engram mcp` on large store | Use safe env (section 2) + restart IDE |
| Slow wake (>5s) | Deep tools at wake | Follow 8-tool contract; `ENGRAM_MEMORY_MODE=lean` |
| `context_for_edit` 403 | Wake queue not acked | `mcp_engram_ack_wake_queue(executed=true)` after `session_start` |
| `context_for_edit` sparse | File never ingested | `engram ingest <path>` once, or `context_for_edit` with `auto_ingest: true` |
| Low recall quality | No embedding server | Set `ENGRAM_EMBED_URL` |
| Lost context between sessions | Skipped `session_end` | Always end with structured summary |

---

## Next Steps

**Default (stay here):** keep [AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md) + [engram-wake-up.md](docs/skills/engram-wake-up.md) as the only standing instructions.

**Continuity proof (optional scripted):** `scripts/continuity-demo.sh` or `python3 examples/hello-engram-agent.py` — wake → remember → end → wake2 handoff (no power-tool flood). Shipped unit: `cargo test continuity_wake_remember_end_wake2_handoff`.

**Goal-stack discipline:** Keep TUI `/goal` Active and Engram `mcp_engram_goal_set_primary` aligned for the work block; complete both at session end.

**Optional later (not required for first sessions):**
1. [docs/GROK_BUILD_MEMORY.md](docs/GROK_BUILD_MEMORY.md) — Grok Build pitch  
2. [SKILLS.md](SKILLS.md) / [docs/RITUALS.md](docs/RITUALS.md) — deep ritual catalog  
3. Contributors: [docs/internal/MAINTAINER_WORKFLOW.md](docs/internal/MAINTAINER_WORKFLOW.md)

*First-run complete. Your manifold is ready.*