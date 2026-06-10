# Agent Memory MVP — "The Memory Tool I Wish I Had"

**Status:** In progress (2026-06-06)  
**Goal:** Make Engram MCP the default continuity layer for AI agents on large stores (181k+ blocks) without ritual tax or RAM death.

## Problem

Agents need: goal + last decisions + file context + trace on forks + handoff on end.  
Current: 5+ wake tools, sampled recall without anchor priority, watch_workspace memory bombs (fixed), 60-tool surface.

## Design Principles

1. **Lean by default, deep on demand** — `ENGRAM_MEMORY_MODE=lean|deep`
2. **One-call wake** — `session_start` returns continuation bundle inline
3. **Anchor-first recall** — goals/traces/scars/rituals before hot sample
4. **Edit-scoped spatial** — `context_for_edit(path)` not whole-repo watch
5. **Structured handoff** — `session_end` produces machine-readable packet
6. **8-tool agent contract** — power tools remain, docs show minimal path

## Phase A (this sprint)

| ID | Deliverable | Files |
|----|-------------|-------|
| A1 | `session_start` inline bundle + readiness + optional `include_spatial` | mcp.rs, store.rs, wake-up.toml |
| A2 | `ENGRAM_MEMORY_MODE` + `memory_mode()` + `set_memory_mode` MCP | main.rs, store.rs, mcp.rs |
| A3 | Anchor-first `recall` tiers + `scope` param | store.rs, mcp.rs |
| A4 | `mcp_engram_context_for_edit` unified tool | store.rs, mcp.rs |
| A5 | `session_end` structured handoff JSON | store.rs, mcp.rs |
| A6 | Agent contract docs + skill update | docs/AGENT_MEMORY_CONTRACT.md, docs/skills/engram-wake-up.md, SKILLS.md |

## Phase B (follow-up)

- Async `load_process_sheaf` (lazy on first goal/trace call)
- `note()` write primitive alias over remember/update
- Store hygiene hints in readiness
- Harness suite `agent-memory-mvp`

## Success Metrics

- Lean wake: 1 MCP call, <500MB RSS, <2s on 181k store
- `recall(scope=anchors)` returns goal/trace before episodic noise
- `context_for_edit` no full `list()` on large store
- Transport lifetime: 10-iteration harness pass

## Execution Log

- 2026-06-06: Plan created; sub-agents launched for A1–A6
- 2026-06-06: A1–A6 implemented; `cargo build -p engram-server` passes; wake-up.toml → 1-call + context_for_edit
- 2026-06-06: Config: `ENGRAM_MEMORY_MODE=lean` in engram-grok + ~/.grok/config.toml