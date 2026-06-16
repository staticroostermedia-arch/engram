# Code Atlas & Agent Continuity

**Status:** Phases 1–5 shipped (atlas v2, trace wiring, LEG panel, cold stalk, ingest parity)  
**Audience:** Grok, Cursor, Claude, future agents, humans in LEG  
**Goal:** Each session leaves the substrate **better situated** for the next instance — not more comment noise in source.

---

## The problem

Agents and humans bury edit history in source:

```rust
// OLD: mutex poison path — scarred trace:1779990956
// TODO: reconcile with context_for_edit sampling gap
pub fn context_for_edit(...) { ... }
```

That history is **flat, unqueryable, session-local**. The next agent re-reads the file and may miss it. Git blame shows *who* changed a line, not *what was decided and ruled out*.

## The superpower

**Situated edit memory** — geometric, evolvable, at the locus where code changes:

| Block | Role | Tool |
|-------|------|------|
| `{stem}__fn__{name}` | **Structure** — current AST, provlog, AABB lines | tree-sitter ingest (refresh on save) |
| `{stem}__fn__{name}__arc` | **Narrative** — edit history, rejected paths, design evolution | `update` (p-momentum, Lyapunov) |
| `trace:*` | **Decision forks** — why at this `file:line` | `quick_trace` + `spatial_context` |
| `scar:*` | **Ruled-out approaches** — never repeat | `scar` + relate to locus |

Source stays clean. Manifold holds the archaeology.

---

## Continuity thesis

> Does each agent pass the system to the next version of itself **better than before**?

**Yes — when the ritual is followed:**

1. **Wake** — `session_start` surfaces goals, traces, harness queue, `ego_snapshot`, `continuity_playbook` (not 187k cold blocks). See `processes/meta/agent_evolution.toml`.
2. **Approach edit** — `context_for_edit(path, line_start, line_end)` returns atlas v2:
   - `spatial_items` + `edit_arc` per locus
   - `traces_at_locus` — decisions at this file/line window
   - `scars_at_locus` — dead ends to avoid
   - `spatial_siblings` — related functions in the same file graph
3. **Fork** — `quick_trace(decision, why, spatial_context="store.rs:4023")`
4. **Edit** — change source; structure block refreshes on re-ingest (p preserved).
5. **Post** — `update("{concept}__arc", "delta: …")` + `relate(trace, ast_concept, edited_at)`
6. **End** — `session_end` handoff packet for next wake.

The next instance inherits **decisions + momentum + structure**, not grep and hope.

Humans oversee via LEG (memory review UI, trace chain, geosphere) without reading every agent step.

---

## Atlas v2 payload (`context_for_edit`)

```json
{
  "atlas_version": "v2",
  "spatial_items": [
    {
      "concept": "store__fn__context_for_edit",
      "line_start": 4240,
      "line_end": 4360,
      "snippet": "pub fn context_for_edit(...)",
      "edit_arc": {
        "concept": "store__fn__context_for_edit__arc",
        "present": true,
        "drift_velocity": 0.12,
        "stability": "converging",
        "snippet": "EDIT ARC — …"
      }
    }
  ],
  "traces_at_locus": [
    {
      "concept": "trace:…",
      "spatial_context": "store.rs:4023",
      "decision_point": "…",
      "justification": "…"
    }
  ],
  "scars_at_locus": [],
  "spatial_siblings": [],
  "continuity_ritual": { "pre": "…", "fork": "…", "post": "…", "anti_pattern": "…" }
}
```

---

## What agents should do (lean contract)

```
# Before edit
mcp_engram_context_for_edit(path="/abs/path/to/file.rs", line_start=4020, line_end=4140)

# At fork
mcp_engram_quick_trace(
  decision="…",
  why="…",
  spatial_context="file.rs:4023"
)

# After edit
mcp_engram_update(
  concept="store__fn__context_for_edit__arc",
  text="delta: added traces_at_locus to atlas v2; rejected full list() scan"
)
mcp_engram_relate(trace_concept, "store__fn__context_for_edit", "edited_at")
```

**Never:** `forget` + `remember` on the same concept. **Never:** `// OLD:` graveyards when `update(__arc)` exists.

---

## Roadmap

| Phase | Deliverable | Status |
|-------|-------------|--------|
| **1** | Atlas v2 in `context_for_edit`; `__arc` mint on ingest; structure refresh preserves p | **Done** |
| **2** | Auto `edited_at` + `decision_at_locus` when `spatial_context` set (`quick_trace` + `record_reasoning_trace`) | **Done** |
| **3** | LEG code-atlas panel + `GET /api/code-atlas` | **Done** |
| **4** | Cold atlas stalk split (`ENGRAM_ATLAS_STALK_SPLIT=1` → `{workspace}_ast` or `cold_atlas`) | **Done** |
| **5** | `glue_ast_file_relations` shared by daemon + `force_ingest_ast_file` | **Done** |

---

## For humans

As agent autonomy grows, you won't read every step. You **will** read:

- LEG hygiene + memory review surface (what's active)
- Trace chain at decision forks
- `__arc` snippets at loci you care about
- Geosphere (place + learned + scene time on tiles)

The substrate is the **audit trail and continuity layer** — not a replacement for code review, but the memory that makes multi-session agent work lawful and inspectable.

---

See also: [AGENT_MEMORY_CONTRACT.md](AGENT_MEMORY_CONTRACT.md), [GROK_BUILD_MEMORY.md](GROK_BUILD_MEMORY.md), [RITUALS.md](RITUALS.md) (Code Edit Ritual v1).