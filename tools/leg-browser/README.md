## LEG Browser

A slick, local, read-only HTML viewer for .leg3 HolographicBlocks.

**Strictly respects the core block invariants** (see `leg_block_invariants_guardrail_v1`):
- No writes
- No changes to binary layout or alignment
- Only reads via safe paths or the running daemon

### True One-Command Experience — "Review My Current Geometric Mind State"

**The polished launcher lives at `scripts/leg`.** It is the canonical entry point (replaces the old fragile one-liner).

```bash
# From Engram repo root (after chmod +x scripts/leg once)
./scripts/leg
```

**Exactly what the user asked for:** one clean command that surfaces the full review surface (hero with sub-agent artifacts + prominent Activity Canvas button) in seconds.

- **STATIC (default, no flags):** Instant curated view. Zero backend. All seamlessness-audit tiles, must-have polishes, handoff deltas, traces, and the geometric Activity Canvas demo are immediately visible and clickable. Ideal for rapid "what is my agent's mind doing right now?" reviews.
- **`./scripts/leg --live`:** Starts `engram serve` (port 3456) in the background with clean logging to `leg-serve.log`, then serves the viewer. The UI auto-detects the live backend; the connection pill turns emerald and the Recent + Momentum sidebar + real block hydration become active while preserving every static tile as fallback.

Additional options:
```bash
./scripts/leg --help          # full flags + mode explanations
./scripts/leg --live --port 9876 --no-open
```

The launcher:
- Picks a free port (starting 8765)
- Discovers the `engram` binary (PATH → target/debug → target/release)
- Cross-platform browser open + robust signal cleanup (no orphaned processes)
- Explicit, colorized UX text that **clearly distinguishes STATIC vs LIVE**
- Favicon 404s are gone (inline geometric SVG data URI in index.html)

See the script header for sub-goal linkage and full ritual notes.

**Within 15-30s of opening:** You see the exact tiles from the two sub-agents (formal_spec_leg-browser-v0-5-seamlessness-audit-test-plan-te + its html_visualization, formal_spec_additional-must-have-polishes-for-grok-build-mem + viz, handoff_delta_leg_browser_v0_5_seamlessness_audit_complete, the wake-up/user-query traces) front-and-center in the new hero section + always-visible Review Current Mind State (Activity Canvas) button. Click for rich visualizations or full focused canvas. Perfect for "I can't see in the TUI" review surface.

The sidebar + hero now lead with the fresh artifacts for your explicit query ("more polishes and testing the Leg Browser").

**Important:** This is a *curated static snapshot* in STATIC mode (or live-augmented when `--live`). New tiles created via MCP appear when you run `engram serve` + connect, or re-open after updates. The launcher makes both modes first-class and delightful.

**Static Demo Hardening (sub-goal:1780106172_harden-leg-browser-static-demo-mode-so-t_sub1 under parent:1780106168_make-the-leg-browser-a-seamless--truly-dynamic-g):** Hero cards for unifying formal_spec + html_visualization tiles (seamlessness-audit-test-plan, additional-must-haves), handoff_delta_leg_browser_v0_5_..., and key traces now render actual rich formatted payloads on click (test plan checklists with fixes/gaps, priority dashboards, delta patches, A/D/R trace views with decision/alternatives/justification, provenance buttons). Activity Canvas has dynamic-feeling inject buttons (Inject Handoff, Inject Trace) that mutate lists in-place. Prominent amber "STATIC QUICK REVIEW MODE" status + banner with explicit instructions vs "Full live dynamic mode (run engram serve)". All changes under full Code Edit Ritual (pre context_for_file on index.html+README, pre/post traces with goal_context, goal set primary). Trace: trace:1780106291_harden-leg-browser-static-demo-fallbacks-in-inde. GUI now genuinely useful for observability in http.server fallback as used in latest session.

### Live Dynamic Mode (v0.3+ — current)
Open `tools/leg-browser/index.html` (or serve it locally). 

**It auto-probes for a running `engram serve` on port 3456** (the default REST server — see `docs/rest_api.md`).

- When detected: 
  - Connection pill turns emerald LIVE (auto or manual click).
  - **Live Recent + Momentum sidebar** appears and auto-refreshes (`/api/recent` every ~6.5s).
  - Clicking live items loads rich block details via `/api/block/:concept` (includes CRS, type, text, outgoing relations).
  - Hybrid momentum hints via `/api/recall`.
- All original curated Phase 0/1 review tiles (mapping table, guardrail, handoff, A/D/R tiles, etc.) remain as **graceful static fallback** in the lower sidebar when no server or offline.
- Perfect foundation for Obsidian-like experience: graph (Mermaid), full html_visualization Thought Tile injection (iframe srcdoc), 3-pane layout, Agent Activity Canvas, quick switcher, and filters coming in Phases B/C.

```bash
# RECOMMENDED (canonical under goal:1780106172_create-polished-launcher-script---update_sub3):
# The true one-command experience:
./scripts/leg                 # STATIC (instant curated mind-state review)
./scripts/leg --live          # LIVE (engram serve background + dynamic manifold)

# (The launcher handles everything: port selection, binary discovery, bg serve with logging,
# browser open, cleanup, and crystal-clear STATIC vs LIVE explanations in the terminal.)

# Manual / advanced (still works):
# Terminal A — start the live backend
engram serve

# Terminal B — serve the viewer (or just open the file directly)
python3 -m http.server 8765 --directory tools/leg-browser
# or: open tools/leg-browser/index.html
```

The viewer is **single-file, zero-build, CDN-only** (Tailwind + Font Awesome + future Mermaid). It feels like a native Obsidian vault navigator for the geometric manifold / living sheaf.

### Future increments (under active development)
- Phase B: Full 3-pane multi-layout, perfect `html_visualization_*` payload injection, backlinks pane.
- Phase C (delivered + UX Doctor hardened): Agent Activity Canvas (prominent; surfaces Current Primary Intent/active goal, recent traces + new Thought Tiles in session, high-momentum items via recency, serving relations). Full beautiful html_visualization rendering (sandbox iframe embed + full-viewport overlay). 18s polling on /api/recent + /api/hydrate with SSE stub + long-poll fallback. All under live "what the agent is actually doing right now" focus. Tested on integration goal + Phase 1 tiles. **v0.5 UX Doctor patch:** ... (see prior). **This sub-task (goal:1780106172_create-polished-launcher-script---update_sub3 under parent goal:1780106168_make-the-leg-browser-a-seamless--truly-dynamic-g):** Delivered the true one-command launcher (`./scripts/leg` + `--live`), full README + root README updates, inline favicon polish, robust scripting, and explicit STATIC/LIVE UX. All changes executed under `ritual:code_edit_ritual_v1` (pre-edit `mcp_engram_context_for_file` + `recall_in_file` spatial recon on targets, `mcp_engram_record_reasoning_trace` + `quick_trace` with goal_context, post-delta capture planned via session_end).

Every significant change creates dual Thought Tiles (text + rich HTML viz) + full Code Edit Ritual traces, all linked to the active goals in the 17801061xx decomposition and the codeland handoff.

Built as part of the CodeLand Integration 2026 effort (see `handoff:codeland_integration_2026_plan`) to give humans a first-class way to inspect the living substrate. Follows `ritual:code_edit_ritual_v1` on all edits. This launcher sub-goal completes the "seamless one-command" requirement for reviewing the geometric mind state.