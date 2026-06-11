# Grok Marketplace Demo — Two-Session Handoff (90 seconds)

**Audience:** xAI evaluators, new plugin users  
**Plugin:** `engram-geometric`  
**Contract:** 8-tool lean path only

---

## Setup (once)

```bash
git clone https://github.com/staticroostermedia-arch/engram.git
cd engram
./scripts/install-engram-plugin.sh
```

Open a **new Grok Build session** on the Engram repo (or any project with the plugin trusted).

---

## Session 1 — Learn and hand off (~45s)

Ask the agent (or run tools):

```
/engram-wake
```

Then:

1. **`mcp_engram_remember`**
   - concept: `demo:marketplace_user_goal`
   - text: "Ship engram-geometric as the best Grok Build memory plugin."

2. **`mcp_engram_quick_trace`**
   - decision: "Use 8-tool lean contract for marketplace listing"
   - why: "Proves continuity without ritual tax on 180k+ stores"

3. **`mcp_engram_session_end`**
   - summary: "Demo session 1: remembered demo goal, traced marketplace decision. Next wake should rehydrate handoff."
   - prepare_compression: true

**Expected:** Structured handoff packet in response; `helper:session_handoff_latest` updated.

---

## Session 2 — Rehydrate (~45s)

Open a **new Grok chat** (same machine, plugin still trusted).

```
/engram-wake
```

**Expected without user paste:**

- Agent states continuation from prior session
- `continuation_bundle` includes handoff preview
- `mcp_engram_recall("marketplace demo", scope="anchors")` surfaces `demo:marketplace_user_goal`

Optional edit demo:

```
mcp_engram_context_for_edit(path="/absolute/path/to/README.md")
```

**Expected:** File-scoped spatial + related traces (no `watch_workspace`).

---

## Health check (anytime)

```bash
./scripts/engram-mcp-health.sh
```

While a Grok session is open: `OK: Live engram MCP already running` — **not** a failure.

Do **not** use `grok mcp doctor` during an active session (lock contention).

---

## CI proof

```bash
STABLE_BIN=target/debug/engram tools/test-harness/bin/engram-harness.sh --suite agent-memory
```

Must exit 0 with `passed: true` and `helper:session_handoff_latest present`.

---

## Differentiation one-liner (for listing copy)

> *Not flat markdown memory — geometric blocks with CRS trust, edit-scoped code context, and machine-readable session handoff.*