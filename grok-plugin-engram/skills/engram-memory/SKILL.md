---
name: engram-memory
description: >
  Engram geometric memory for AI agents — slash commands map decision moments
  to MCP rituals. You are the primary user. Lean highway + power escalation.
metadata:
  short-description: "Geometric memory — full slash command index"
---

# Engram Memory — Agent Primary User

**Canonical contract:** [docs/AGENT_MEMORY_CONTRACT.md](../../../docs/AGENT_MEMORY_CONTRACT.md) · **Ritual skills:** [docs/skills/](../../../docs/skills/)

**You are the primary user.** Use slash commands as triggers — each runs a full MCP ritual. Full map: `docs/TOOL_DECISION_MAP.md` · Index: `grok-plugin-engram/commands/README.md`

## Non-negotiable every session

| Trigger | Command |
|---------|---------|
| Start | `/engram-wake` — then **execute** `harness_injection.suggested_actions` before edits |
| End | `/engram-session-end` |

**Queue-before-edits:** After `session_start`, run the harness queue (handoff → goal → tiles → trace head) before `context_for_edit` or broad reads. LEG Browser left rail mirrors the same queue via `/api/context-window`.

## Work loop

| Trigger | Command |
|---------|---------|
| Before file edit | `/engram-edit` |
| Stuck on goals/decisions | `/engram-recall` |
| Preview truncated | `/engram-read` |
| Recall feels weak | `/engram-ready` |
| Decision fork | `/engram-trace` |

## Write path (ONE per persist)

| Trigger | Command |
|---------|---------|
| Refine existing (>0.85) | `/engram-update` |
| New concept | `/engram-remember` |
| Verified fix | `/engram-solution` |
| Dead end | `/engram-scar` |
| Graph edge | `/engram-relate` |

## Read escalation

| Trigger | Command |
|---------|---------|
| Trending / evolving | `/engram-momentum` |
| Geometric similarity | `/engram-pure` |
| Graph explore | `/engram-graph` |

## Meta & mode

| Trigger | Command |
|---------|---------|
| Multi-phase arc | `/engram-tile` |
| Goal focus | `/engram-goal` |
| Deep exploration | `/engram-deep` |
| Back to lean | `/engram-lean` |
| Lawfulness check | `/engram-verify` |
| Spatial recovery | `/engram-ingest` |
| Schedule recurring (Grok /loop e.g. consciousness strange loop) | `/engram-loop` (parse per spec → bare native scheduler_create + Enram record/relate to consciousness goal/tile/process + subvisor governance + honest confirm or scar) |

## Agent discipline

Calling tools **is** the product. Documentation without MCP calls leaves no geometric record.

**Cursor throttle:** wake + edit on substrate paths + session-end minimum; escalate at forks.  
**Grok Build throttle:** invoke liberally — edit every file, trace every fork, update design/progress blocks.

**Never** `forget` + `remember` on the same concept.

## Human review (LEG Browser)

```bash
./scripts/leg --live   # repo root — live manifold viewer on :8765
```

Slash: `/engram-leg`. Hygiene demotion: `mcp_engram_demote_from_context` (agents) or LEG inbox Demote (humans). Static `./scripts/leg` (no `--live`) shows embedded demo tiles only — not current MCP work.

## Docs

- `docs/AGENT_MEMORY_CONTRACT.md` — 8-tool highway
- `docs/TOOL_DECISION_MAP.md` — all 66 tools + mermaid
- `docs/GROK_BUILD_MEMORY.md`