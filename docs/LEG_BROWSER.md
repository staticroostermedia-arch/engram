# LEG Browser (beta)

**Local human mirror for Engram agent memory.** Read-only. No cloud. No build step.

Agents write memory via MCP into `~/.engram/` (or your `ENGRAM_STORE`). LEG Browser reads that same store through `engram serve` and shows you what the agent actually retained — traces, goals, tiles, handoffs, and lineage — without scrolling chat logs.

---

## Quick start

From the Engram repo root:

```bash
./scripts/leg              # STATIC — instant curated demo, no backend
./scripts/leg --live       # LIVE  — starts serve :3456 + viewer :8765
```

Open **http://127.0.0.1:8765** (auto-opens on most systems).

![LEG Browser beta — live mode](../images/leg-browser-beta-live.png)

**Safe serve restart** (does not kill your IDE/TUI MCP process):

```bash
./scripts/restart-leg-serve.sh
```

---

## Modes

| Mode | Command | Backend | What you see |
|------|---------|---------|--------------|
| **Static** | `./scripts/leg` | None | Curated demo tiles — useful offline, not your live manifold |
| **Live** | `./scripts/leg --live` | `engram serve` on `:3456` | Real traces, goals, handoff, activity feed, consciousness surface |

Live mode uses the same `ENGRAM_STORE` as `scripts/engram-grok` (MCP). TUI, Cursor, and Grok Build sessions all write to the same disk; LEG shows the union via activity feed + hot/recent APIs.

---

## What the beta includes

- **Wake queue** — prioritized `suggested_actions` from harness injection (same queue agents should run before edits)
- **Continuity playbook** — 12-step agent evolution narrative
- **Ego evolution strip** — NREM drift summary from `ego.leg3`
- **Presentation stratum** — ~40–64 distilled nodes (goals/traces/tiles/process), not the full cold manifold
- **Consciousness surface / geosphere** — lineage edges, distillate markers
- **Activity SSE** — MCP and serve events from `activity_feed.jsonl`
- **Block inspector** — click any concept → `/api/block/:concept` with relations/backlinks
- **Hygiene strip** — demote sprawl, condensation hints (beta)

---

## Beta caveats (honest)

- **Single-file SPA** (`tools/leg-browser/index.html`) — no React build, easy to fork and improve.
- **Large stores** (100k+ blocks): galaxy and context panels can take 10–15s. Presentation stratum keeps wake/agent paths lean; LEG may still feel heavy on first load.
- **Hard refresh** after `index.html` updates: Ctrl+Shift+R.
- **Not Obsidian** — no plugin ecosystem, no full graph editor. Read-only review surface today.
- **Separate MCP processes** share disk, not in-memory gate state. Both sessions appear in activity feed.

---

## Architecture

```
Agent (MCP) ──writes──► ~/.engram/stalks/  (.leg3 manifold)
                              ▲
                              │ read
                         engram serve :3456
                              ▲
                              │ fetch /api/*
                         LEG Browser :8765  (python http.server via scripts/leg)
```

Key REST endpoints consumed by the viewer:

| Endpoint | Purpose |
|----------|---------|
| `/health` | Connection pill |
| `/api/context-window` | Wake queue, harness, presentation stratum |
| `/api/consciousness-surface` | Geosphere / distilled nodes |
| `/api/block/:concept` | Full block + relations |
| `/api/recent` | Live sidebar |
| `/api/activity` | Activity feed |

---

## For contributors

- Viewer: `tools/leg-browser/index.html` (vanilla JS + Tailwind CDN)
- Launcher: `scripts/leg`
- Serve APIs: `crates/engram-server/src/serve.rs`
- Harness bundle: `crates/engram-server/src/harness_injection.rs`
- Presentation stratum: `crates/engram-server/src/presentation_stratum.rs`

Improvements welcome — graph layout, faster context-window, write paths, mobile layout, screenshots in README.

---

## Related docs

- [AGENT_MEMORY_CONTRACT.md](AGENT_MEMORY_CONTRACT.md) — 8-tool lean agent loop
- [HARNESS_INJECTION.md](HARNESS_INJECTION.md) — wake queue, ego, playbook
- [GROK_BUILD_MEMORY.md](GROK_BUILD_MEMORY.md) — Grok Build pitch
- Plugin: `grok-plugin-engram/commands/engram-leg.md`