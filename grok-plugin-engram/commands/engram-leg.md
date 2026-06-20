---
name: engram-leg
description: Open the LEG Browser — live geometric mind-state review surface (./scripts/leg --live)
---

Open the native LEG Browser to review the **live** Engram manifold (not the static June demo).

## Run (mandatory — execute, do not just tell the user)

From the Engram repo root:

```bash
./scripts/leg --live
```

This:
- Starts `ENGRAM_PROFILE=cockpit engram serve --no-scout` on port **3456** (same `ENGRAM_STORE` as MCP)
- **Cockpit** (default): GPU hot stratum, presentation cache, lazy galaxy — fast live LEG review
- **Legacy CPU-only**: `./scripts/leg --live --ui` (`ENGRAM_PROFILE=ui`; `--light` is deprecated alias)
- Serves `tools/leg-browser/index.html` on **8765** (fresh temp copy, cache-busted)
- Auto-probes live mode; hero pulls `helper:session_handoff_latest` + `/api/hydrate` + `/api/active-context`
- Demo consciousness emitter is **off** by default (it used to flood the UI with fake tiles). Presenter sim only: `./scripts/leg --live --demo-emitter`

## What the user should see (v2 memory review UI)

- Sidebar: Minecraft-style block cubes (green=tile, blue=trace, gold=goal, purple=handoff)
- **Every click works**: fetches `/api/block/:concept` and JIT-renders from the block's own payload
- **◉ Live**: SSE activity stream + spatial AABB overlay + trace scrubber
- **Hygiene strip**: serving sprawl, stale goals, condensation-on-stack — one-click Demote / Clear condensation
- Thought tiles: `tile_type`, title, `human_forward`, payload JSON
- Traces: decision / justification / alternatives
- Handoff: `helper:session_handoff_latest` decisions + files + open questions
- Relations are clickable backlinks

**Safe serve restart** (does not kill MCP): `./scripts/restart-leg-serve.sh`

## If it still looks static

1. Hard refresh: Ctrl+Shift+R
2. Confirm serve: `curl -s http://127.0.0.1:3456/health`
3. Confirm handoff: `curl -s http://127.0.0.1:3456/api/block/helper:session_handoff_latest | head -c 200`
4. Re-run: `./scripts/leg --live` (kills stale PIDs via trap)

## Static-only (no backend)

```bash
./scripts/leg
```

Shows curated demo tiles — useful offline, but **not** your current MCP work.

## Ritual

After opening for the user, optionally `mcp_engram_quick_trace` that LEG review was launched for visibility in the manifold.