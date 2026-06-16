# LEG Browser

**Local viewer for your Engram memory store** (`~/.engram/`). Read-only. No build step. No cloud.

Agents write memory via MCP; LEG Browser shows what they actually retained — traces, goals, tiles, handoffs, and lineage — without scrolling chat logs.

## Quick start

From the Engram repo root:

```bash
./scripts/leg              # STATIC — instant curated demo, no backend
./scripts/leg --live       # LIVE  — starts engram serve + viewer
```

The browser opens at **http://127.0.0.1:8765** on most systems.

| Mode | Command | What you see |
|------|---------|--------------|
| **Static** | `./scripts/leg` | Curated demo tiles — useful offline, not your live store |
| **Live** | `./scripts/leg --live` | Real data from `~/.engram/` via `engram serve` on `:3456` |

Additional flags: `./scripts/leg --help`, `--port 9876`, `--no-open`.

To restart the serve process without killing your IDE/TUI MCP session:

```bash
./scripts/restart-leg-serve.sh
```

## What you get in live mode

- **Activity feed** — MCP and serve events as they happen
- **Recent + momentum sidebar** — what is trending in your manifold
- **Block inspector** — click any concept for full payload and relations
- **Consciousness surface** — distilled goals, traces, and tiles at a glance

The viewer is a single-file SPA (`index.html`) — vanilla JS + Tailwind CDN. Fork and improve freely.

## Related docs

- [docs/LEG_BROWSER.md](../../docs/LEG_BROWSER.md) — full beta guide, architecture, and API endpoints
- [docs/PERSONAL_KNOWLEDGE_WIKI.md](../../docs/PERSONAL_KNOWLEDGE_WIKI.md) — build a compounding personal wiki with Engram
- [docs/AGENT_MEMORY_CONTRACT.md](../../docs/AGENT_MEMORY_CONTRACT.md) — 8-tool lean agent loop