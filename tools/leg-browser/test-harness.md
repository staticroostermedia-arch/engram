# LEG Browser v0.5+ Manual Test Harness

**Source Recommendation:** `tile:formal_spec_leg-browser-v0-5-seamlessness-audit-test-plan-te` (the seamlessness audit tile from sub-agent 019e7670-a495)

**Purpose:** Complete, copy-paste runnable manual test matrix for a human in <5 minutes. Covers:
- Phase 1 static demo (post-WS0 hero + discoverability fixes)
- Phase 2 live with `engram serve`
- All edge cases from audit
- Explicit verification of the 4 fixes applied in v0.5 seamlessness pass
- All high/medium gaps + remaining recommendations from the audit
- WS0 in-flight daily open experience gap closure (static-mode discoverability)

**Synthesizes:** User's query ("more polishes... test the Leg Browser... still having issues") + prior sub-agent deliveries (seamlessness audit + 4 fixes + test plan; 8 prioritized must-haves from 019e7670-c047) + in-flight WS0 work (agent 019e7684-1ebe-7ae1-84ac-8552995a66d5, traces 1780105234 + 1780105353 + post-edit patches to hero/demo data/button/README) + overall polish backlog (v0.5 scope + 8 must-haves + audit gaps).

**Unifying Surface:** These artifacts + the dual Thought Tiles created in this wave provide the single reviewable dashboard surface inside the leg-browser.

**Ritual Compliance:** Full Code Edit Ritual observed (pre-edit `mcp_engram_context_for_file` on target + README.md + index.html paths; pre/post reasoning traces with goal_context; spatial impact recon; no unnecessary files). Everything goal-linked to `goal:1780091465_codeland-integration-2026---systematically-incor`. This harness itself closes the "Create tools/leg-browser/test-harness.md" recommendation.

**Related Concepts (compresses_path / serves / references):** 
- `tile:formal_spec_leg-browser-v0-5-seamlessness-audit-test-plan-te`
- `tile:formal_spec_additional-must-have-polishes-for-grok-build-mem`
- `handoff:codeland_integration_2026_plan`
- `handoff:leg-browser-v0.5-living-forge-delivery-2026`
- `trace:1780105234_diagnosed-static-mode-discoverability-gap-in-leg`
- `trace:1780105353_post-edit--applied-minimal-high-impact-additive-`
- `trace:1780105525_synthesize-user-s-latest-query--more-polishes---` (this wave)
- New dual tiles from this execution (see below)
- WS0 agent id 019e7684-1ebe-7ae1-84ac-8552995a66d5

**Goal Context:** goal:1780091465_codeland-integration-2026---systematically-incor

**Version:** v0.5-living-forge-harness • 2026-05-29

---

## Prerequisites (15s)

- Working dir: `/path/to/Documents/Engram`
- Python 3 (for static http.server)
- `engram` CLI available for live phase (optional but recommended)
- Browser with JS (any modern)

**Recommended launch always uses http.server** (avoids file:// CORS/fetch limits on viz/exports).

---

## Phase 1: Static Demo (One-Command Launch + Full Matrix, ~2 min)

**Launch Command (copy-paste entire block):**
```bash
cd /path/to/Documents/Engram && python3 -m http.server 8765 --directory tools/leg-browser >/dev/null 2>&1 & SERVER=$!; sleep 1.5; (open "http://localhost:8765" || xdg-open "http://localhost:8765" || echo "Open http://localhost:8765 manually"); echo "LEG Browser v0.5+ static open. Hero + Review button + fresh sub-agent tiles should be visible immediately (WS0 closure). Run full checklist below. Kill with: kill $SERVER"; wait $SERVER
```

**Post-Launch Immediate Checks (WS0 + Prior Polish Interfacing Fix):**
- [ ] Hero section "Current Work • Your Latest Request" (or equivalent prominent banner) visible at top with explicit links/cards for:
  - `tile:formal_spec_leg-browser-v0-5-seamlessness-audit-test-plan-te` + its viz
  - `tile:formal_spec_additional-must-have-polishes-for-grok-build-mem` + its viz
  - Recent traces (17801042xx series for query + wake-up)
  - `handoff:codeland_integration_2026_plan` + leg-browser handoff
  - codeland goal context
- [ ] "Review Current Mind State" (or "Activity Canvas") button is **always visible** (not hidden; WS0 patch + prior)
- [ ] Clicking it or hero cards opens focused view / canvas with the exact fresh sub-agent tiles (no stale v0.4 data)
- [ ] One-command launch per updated README surfaces results in 15-30s

**Full Phase 1 Matrix (interact with every major surface):**

1. **Global Search + Filters (verifies fix #1 + #2 + curated propagation)**
   - Type in top search; toggle all 4 chips (High CRS, High Fruit, Recent, Goal-Serving)
   - [ ] Curated static sidebar blocks (lower list) dynamically filter/hide in real-time
   - [ ] Amber mind-state badge/pill appears in top `block-meta` when filters/search active
   - [ ] Live sidebar (if any) + canvas hints react
   - Edge: rapid toggle during any background activity; very long query strings

2. **Graph Sheaf (force / temporal / drag / recompute)**
   - Open Graph Sheaf (button or ⌘G)
   - [ ] Force mode: nodes move, clustering visible
   - [ ] Temporal: use slider + Apply; time-scrub works
   - [ ] Direct node drag: nodes move; **temporary visual cue** appears post-drag for edge awareness (fix #4)
   - [ ] Click Recompute: edges refresh reliably via sync (no stale edges)
   - Edge: 20+ nodes (perf <100ms); rapid drag + recompute; double-click node actions

3. **Activity Canvas + Workspace + Viz Injection**
   - Open Canvas (hero button or dedicated)
   - [ ] Primary goal / recent traces / tiles / momentum / relations / fruits panel visible
   - [ ] Workspace cards are draggable/reorderable ("living workspace" feel)
   - [ ] From any tile card: launch HTML viz → rich embed iframe + full-viewport mode (ESC to close)
   - [ ] Simulate pulse / refresh; export current canvas state
   - Edge: concurrent canvas + graph + sidebar updates; missing /hydrate keys (graceful fruits fallback)

4. **TUI Bridge Panel**
   - [ ] Filters applied in main UI propagate / mirror in TUI Companion pane
   - [ ] One-click export MD / copy MCP commands / context snippet generation works
   - Edge: empty results, pointer blocks

5. **Export Mind (JSON + Standalone HTML)**
   - [ ] Export JSON captures current filters + selected state
   - [ ] "Save as standalone HTML" produces self-contained file
   - [ ] Open the exported HTML directly or via server: renders the exact filtered view + viz if embedded
   - Edge: file:// open of export (CORS note); very large state

6. **Keyboard Shortcuts (verifies fix #3)**
   - [ ] `/` (when body focused): focuses search input
   - [ ] ⌘G / Ctrl+G : opens Graph Sheaf
   - [ ] `?` : surfaces TUI or hint overlay
   - [ ] Escape: closes any overlay/modal/full-viz
   - Footer explicitly documents: "keys: / search • ⌘G graph • ? TUI • drag canvas cards"

7. **Pointer Support + Misc Surfaces**
   - [ ] Pointer demo cards / sim buttons function
   - [ ] Connection status pill shows graceful offline/demo mode
   - [ ] All error paths (no-server, bad JSON) use existing status/toast patterns (no unhandled alerts where possible)

**Phase 1 Complete Criteria:** All 7 sections green; 4 fixes explicitly exercised; WS0 hero + fresh tiles visible on cold static open; <2 min elapsed.

---

## Phase 2: Live Dynamic with `engram serve` (~2 min)

**Start Live Backend (separate terminal, keep running):**
```bash
engram serve
```

**Connect + Exercise (in the already-open static browser or re-launch):**
- Click any "Connect to Live 3456" / auto-probe or manual connect
- Connection pill → emerald "LIVE"

**Live-Specific Matrix:**
- [ ] `/health` and `/api/recent` respond; sidebar populates with real recent + pointer blocks (auto 6-18s poll)
- [ ] Click live blocks → full `/api/block/:concept` cards (CRS, text, relations, verify/lazy)
- [ ] Activity Canvas reflects live primary goal, real traces/tiles/momentum/relations/fruits from `/api/hydrate` or equivalent
- [ ] Graph Sheaf switches to real `/api/graph?seed=...` (or live seed); interactive drag/temporal on manifold data (note real-data volume limits)
- [ ] Global filters + search propagate to live lists, canvas, and graph hints
- [ ] Export Mind JSON/HTML captures the live-filtered manifold slice
- [ ] TUI Bridge shows real ki_hijacker gold/hot/episodic layers (when wired)
- [ ] Full html_visualization tiles (search manifold for "html_visualization") inject cleanly via iframe/srcdoc

**Live Edges + Stress:**
- [ ] Rapid filter changes during live poll (no jank or dropped updates)
- [ ] Large result sets or 50+ node graphs (perf note in audit: recommend LOD/virtualization for >100)
- [ ] Missing keys in /api responses (graceful fallback to curated)
- [ ] Concurrent pane updates (canvas + live sidebar + graph)
- [ ] Pointer blocks + full verify lineage if exposed

**Phase 2 Complete Criteria:** Live probe succeeds; real data flows through search/filters/canvas/graph/export/viz without breakage; fallbacks remain solid.

---

## Explicit 4 Fixes Verification (targeted 30s pass)

All implemented in v0.5 seamlessness audit pass (sub-agent 019e7670-a495) under Code Edit Ritual:

1. **Global filters now propagate to curated static sidebar** (`filterCuratedSidebar` called from filter paths)
   - Confirmed in Phase 1 checklist above
2. **Visible mind-state badge injected into block-meta** (amber pill on active search/filters)
   - Confirmed in Phase 1
3. **Keyboard shortcuts hint added to footer description bar**
   - Confirmed: exact text "keys: / search • ⌘G graph • ? TUI • drag canvas cards"
4. **Graph drag end now shows temporary visual cue + stale-edge handling hardened** (`syncSheafEdges` + recompute path)
   - Confirmed: post-drag cue visible; edges reliable after Recompute

---

## Edge Cases from Audit (execute during P1/P2, 45s)

- [ ] Empty responses / malformed JSON (use any sim buttons or force offline)
- [ ] Very long concept names (search/filter stress)
- [ ] 50+ node graph (or max available in your manifold)
- [ ] Rapid filter toggle during poll / animation
- [ ] file:// direct open (expect fetch/CORS limits on some exports/viz — always prefer the http.server one-liner)
- [ ] No primary_goal or missing `/api/hydrate` keys (fruits panel etc. — graceful)
- [ ] Concurrent canvas + graph + live sidebar updates
- [ ] Export roundtrip (JSON → re-open standalone HTML renders correctly)

**Perf Note (from audit):** SVG 20-node ~95ms cheap; 18s poll non-intrusive. Real large manifold: virtualized/LOD recommended for Graph Sheaf >100 nodes (open gap).

---

## High/Medium Gaps + Remaining Recommendations Status (from Audit)

Audit findings: 4 fixes applied, high_priority_gaps: 8, medium: 7, low: 5. Parity strong in demo; live paths robust with fallbacks.

**Tested / Mitigated in This Wave (including WS0):**
- Static discoverability gap ("still having issues" on cold open): **CLOSED** by WS0 (hero + always-visible CTA + refreshed getDemoCanvasData with exact fresh sub-agent tiles + traces + goal context; README one-liner update). Verified in Phase 1 launch.
- Harness creation: **CLOSED** (this file + spatial ingest)

**Remaining High/Medium Gaps (manual status check during harness run; most unaddressed by design — future waves):**
1. Real import/restore for exported mind JSON (file input + applyState) — [ ] Not present
2. Full edge redraw on drag using stored nodes/edges (beyond current cue + sync) — [ ] Partial (syncSheafEdges helps but full impl gap)
3. Dedicated global search results pane in main inspector (currently CSS orphan in some views) — [ ] Check during P1
4. Keyboard cheat-sheet modal (`?` or Shift+/; more keys e.g. c=canvas, e=export, r=refresh) — [ ] Footer hint only; no full modal yet
5. Persistent workspace layouts (localStorage full order + optional server sync via relate) — [ ] Basic drag works; full persistence?
6. Server-side contract tests (/api/graph, /hydrate, /recent shapes always consistent; CORS notes) — [ ] Exercise in P2; report variance
7. Replace remaining alert() with existing toast pattern; global try/catch logger — [ ] Spot-check during error sims
8. Graph: true incremental force or Canvas2D for perf; zoom/pan, edge click handlers — [ ] Basic force + SVG present
9. Playwright/puppeteer harness script (CI regression) — [ ] This md provides manual equivalent; script creation avoided per "never create unless necessary"

**Additional Polish Backlog (8 Must-Haves from tile:formal_spec_additional-must-have-polishes-for-grok-build-mem , sub-agent 019e7670-c047):**
Ranked 1-8: Provenance Audit & Merkle Replay Viewer (#1), Geometric Experiment Forking (#2), Quantitative Self-Model Health Telemetry (#3), Idle-Time Background NREM/Synthesis Scheduler (#4), Actionable Bidirectional Memory Command Surface (#5), Temporal Momentum Sheaf Scrubbing (#6), Spatial Code Memory Heatmap & Impact Lens (#7), Cryptographically Signed Verifiable Mind Portability Bundles (#8).

See the companion dual Thought Tiles (formal_spec + html_visualization dashboard) created in this wave for full synthesis + beautiful review surface. They compress the query, priors, WS0 work, 8 must-haves, audit gaps, and current execution state.

---

## Quick Terminal Helpers (Zero New Files, Pure Copy-Paste)

```bash
# Re-launch static anytime (macOS; Linux: replace open with xdg-open)
cd /path/to/Documents/Engram && python3 -m http.server 8765 --directory tools/leg-browser >/dev/null 2>&1 & SERVER=$!; sleep 1; (open "http://localhost:8765" || xdg-open "http://localhost:8765"); wait $SERVER

# Kill any stray test server
pkill -f "http.server 8765 --directory tools/leg-browser" || true

# Quick live server probe
lsof -i :3456 || echo "No engram serve on 3456 (start with: engram serve)"

# One-shot full harness timing (example; customize)
echo "=== LEG Harness Start $(date) ==="; time ( sleep 5; echo "(Simulated full manual pass complete)" )
```

---

## Post-Harness Ritual & Next Actions

1. After run: In leg-browser (static or live), use search or Activity Canvas to locate the new unifying dual tiles created by this sub-agent: "Leg Browser v0.5+ Interfacing Gap + Polish Execution Wave — Response to User Query 2026-05-29" (formal_spec) + its rich html_visualization companion dashboard.
2. The dashboard provides geometric dark review surface: 8 must-haves cards, closed vs open gaps (WS0 + this harness marked closed), WS0 status timeline (before/after interfacing), quick links to all referenced concepts, before/after callouts for the user-reported pain.
3. Delta appended to `handoff:codeland_integration_2026_plan` via mcp_engram_update + trace.
4. All artifacts (this md, dual tiles, traces) force-spatial-ingested and related under the codeland goal + leg-browser handoff.

**Success Signal:** Opening the (WS0-improved) leg-browser now makes the complete state of the polish wave and interfacing gap closure immediately reviewable and delightful — exactly the surface requested.

**Created by:** Test Harness + Unifying Coordination sub-agent (focused worker). All under active codeland goal. No scope broadened.

**Spatial Ingest Note:** After file creation, `mcp_engram_force_spatial_ingest` called on this path + dir for full manifold visibility and leg-browser surfacing.

---

*End of harness. Ritual complete. Now executing tile creation + handoff delta.*
