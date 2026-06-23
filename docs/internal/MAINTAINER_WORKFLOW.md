# Maintainer Workflow — Engram Ritual Loop

**Audience:** Contributors and operators who dogfood Engram on this repo.  
**Public users:** Start with [FIRST_RUN.md](../../FIRST_RUN.md) and [docs/AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md).

This document captures how maintainers run the lean contract day to day. It is not a substitute for the published agent protocols in `docs/skills/`.

---

## Canonical sources

| Layer | Location | Role |
|-------|----------|------|
| **Lean contract** | [docs/AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md) | 8 essential MCP tools, lean vs deep mode |
| **Ritual protocols** | [docs/skills/](../skills/) | Wake, working memory, session-end, thought tiles |
| **Process sheaf** | [processes/](../../processes/) | Declarative rituals loaded at `session_start` |
| **Optional overlay** | `.grok/skills/` (local, not committed) | Grok Build / TUI convenience copies — mirror `docs/skills/` when present; repo markdown remains canonical |

Do not treat `.grok/skills/` as overriding `docs/skills/`. Keep ritual changes in `docs/skills/` and sync any local overlay after edits.

---

## Session loop (lean default)

### Wake — every MCP restart or long sleep

```
mcp_engram_session_start(intent="…")
```

One call. Read `continuation.primary_goal`, `suggested_actions`, and `trace_chain_head` from the inline slim bundle.

**At wake, do not:**
- `watch_workspace` (lean: use `context_for_edit(path)` per file touched)
- `rebuild_bvh` unless the user explicitly needs full GPU recall
- Broad `query_with_momentum` before anchor recall

**Optional after wake:** `mcp_engram_get_continuation_bundle` when you need the full harness or continuity playbook. Long-sleep return: see [docs/LONG_SLEEP_WAKEUP_PROTOCOL.md](../LONG_SLEEP_WAKEUP_PROTOCOL.md).

Executable detail: [docs/skills/engram-wake-up.md](../skills/engram-wake-up.md).

### During work

1. **Recall before derive** — `mcp_engram_recall(..., scope="anchors")` before new writes.
2. **Edit prep** — `mcp_engram_context_for_edit(path)` for the file you are changing.
3. **Decide** — `mcp_engram_quick_trace` at forks (decision, why, alternatives, falsifiability).
4. **Write** — `mcp_engram_remember` for new concepts; `update` when recall score > 0.85 on an existing concept.
5. **Hygiene** — scar visible tool/MCP failures with `mcp_engram_scar`; avoid repeated expensive reads inside an established context (see [docs/TOOL_DECISION_MAP.md](../TOOL_DECISION_MAP.md)).

Executable detail: [docs/skills/engram-working-memory.md](../skills/engram-working-memory.md).

### End of block

```
mcp_engram_session_end(summary="…")
```

Produce a real structured handoff — not a one-line diary entry. Future wakes bind to this via momentum and relations.

Executable detail: [docs/skills/engram-session-end.md](../skills/engram-session-end.md).

### Human review

```bash
./scripts/leg              # static curated view
./scripts/leg --live       # live manifold mirror
```

See [docs/LEG_BROWSER.md](../LEG_BROWSER.md).

---

## Private data vs. the repo

- **Repo:** engine, MCP server, CLI, ritual docs, LEG Browser, process sheaf.
- **Local store:** `~/.engram/stalks/` (or `ENGRAM_STORE`) — goals, traces, tiles, and episodic blocks stay on disk; never committed.

Clone the repo for tools and protocols. Mind state stays sovereign per machine.

---

## Git commits and releases

See [CONTRIBUTING.md § Commit Message & Versioning Discipline](../../CONTRIBUTING.md#commit-message--versioning-discipline) (single source of truth).

- Record `mcp_engram_quick_trace` **before** every `git commit`; put `trace:*` and `goal:*` in the message `Refs:` line.
- Never bump `Cargo.toml` version on feature/fix commits — release-only (worktree + verify + changelog + tag per `version_git_rollback`).
- Validate: `scripts/validate-commit-msg.sh .git/COMMIT_EDITMSG` (or pipe message on stdin).

---

## Maintainer checklist

- [ ] Ritual edits land in `docs/skills/` first; update `processes/*.toml` when behavior changes.
- [ ] Commits follow CONTRIBUTING commit discipline (conventional + body + trace/goal refs).
- [ ] Lean wake stays one call; no `watch_workspace` in default agent profile.
- [ ] `ENGRAM_PROFILE=agent` via `scripts/engram-grok` for IDE MCP configs.
- [ ] Power-tool changes reflected in [docs/TOOL_DECISION_MAP.md](../TOOL_DECISION_MAP.md) and [docs/MCP_TOOLS_REFERENCE.md](../MCP_TOOLS_REFERENCE.md).
- [ ] Public README / FIRST_RUN remain the onboarding surface — this file stays internal.

---

## Related docs

- [SKILLS.md](../../SKILLS.md) — public entry point for ritual skills
- [docs/RITUALS.md](../RITUALS.md) — full ritual overview
- [docs/DEFORMATION_PLAYBOOKS.md](../DEFORMATION_PLAYBOOKS.md) — JIT deformation / RSI at wake
- [CONTRIBUTING.md](../../CONTRIBUTING.md) — PR checklist and crate layout
- [docs/HARNESS_INJECTION.md](../HARNESS_INJECTION.md) — current wake injection
- [docs/SUBSTRATE_WINS_PLAN.md](../SUBSTRATE_WINS_PLAN.md) — harness program (shipped; historical)