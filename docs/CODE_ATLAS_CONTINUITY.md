# Code Atlas & Agent Continuity

**Status:** Code atlas continuity v2 shipped (atlas v2.1, evolution handles, harness enforcement, update coherence, reference frame)  
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

1. **Wake** — `session_start` surfaces goals, traces, harness queue, `ego_snapshot`, `continuity_playbook` (not the full cold manifold). Execute queue → `ack_wake_queue` (hard gate default for agent profile).
2. **Approach edit** — `context_for_edit(path, line_start, line_end)` returns atlas v2.1:
   - `spatial_items` + `edit_arc` per locus
   - `traces_at_locus` + `traces_at_locus_tiers` (exact / file / stem)
   - `scars_at_locus` — dead ends to avoid
   - `spatial_siblings` — related functions in the same file graph
   - `edit_arc_debt` — pending arcs when post-edit ritual incomplete
3. **Fork** — `quick_trace(decision, why, spatial_context="store.rs:4023")` (normalized to file:line)
4. **Edit** — change source; structure block refreshes on re-ingest (p preserved).
5. **Post** — `update("{concept}__arc", "delta: …")` from `post_edit_palette` + `relate(trace, ast_concept, edited_at)`
6. **Deep recon (optional)** — `evolution_at_locus(path, line_start, line_end)` for arc segments + trace chain without re-reading full blocks
7. **End** — `session_end` handoff packet for next wake.

The next instance inherits **decisions + momentum + structure**, not grep and hope.

Humans oversee via LEG (memory review UI, evolution timeline, trace chain, geosphere) without reading every agent step.

---

## Atlas v2.1 payload (`context_for_edit`)

```json
{
  "atlas_version": "v2.1",
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
  "traces_at_locus_tiers": {
    "exact": [],
    "file": [],
    "stem": []
  },
  "scars_at_locus": [],
  "spatial_siblings": [],
  "edit_arc_debt": { "pending": 0 },
  "continuity_ritual": { "pre": "…", "fork": "…", "post": "…", "anti_pattern": "…" }
}
```

Per-file `harness_injection` adds **`post_edit_palette`** — concrete `mcp_engram_update` args when `spatial_items` are present.

---

## Power tools (code atlas)

| Tool | When |
|------|------|
| `mcp_engram_evolution_at_locus` | After edits or for arc archaeology — loci, arc segments (`--- update @ ---`), `prev_in_trace` chain, scars |
| `mcp_engram_ack_edit_arc` | Read-only repeat pass on same file when `edit_arc_debt` pending (hard gate) |
| `mcp_engram_ingest_reference_frame` | One-shot mint of linguistic reference frame + pillar blocks (etymology on `__arc`; patent-aligned container) |
| `mcp_engram_ack_wake_queue` | After executing wake `suggested_actions` (required before `context_for_edit` in hard mode) |

`ENGRAM_UPDATE_COHERENCE=warn` (agent default) checks provlog coherence on `update` — prefer lawful deltas on `__arc`.

---

## What agents should do (lean contract)

```
# Wake (agent profile)
mcp_engram_session_start(intent="…")
# execute suggested_actions
mcp_engram_ack_wake_queue(executed=true)

# Before edit
mcp_engram_context_for_edit(path="/abs/path/to/file.rs", line_start=4020, line_end=4140)

# At fork
mcp_engram_quick_trace(
  decision="…",
  why="…",
  spatial_context="file.rs:4023"
)

# After edit — use post_edit_palette args when present
mcp_engram_update(
  concept="store__fn__context_for_edit__arc",
  text="delta: added traces_at_locus_tiers to atlas v2.1; rejected full list() scan"
)
mcp_engram_relate(trace_concept, "store__fn__context_for_edit", "edited_at")

# Optional deep recon
mcp_engram_evolution_at_locus(path="/abs/path/to/file.rs", line_start=4020, line_end=4140)
```

**Never:** `forget` + `remember` on the same concept. **Never:** `// OLD:` graveyards when `update(__arc)` exists.

---

## Large stores (100k+ blocks)

Agent paths stay lean:

- `context_for_edit` / `evolution_at_locus` use bounded stem-prefix spatial scan + optional single-file auto-ingest (not full `list()`).
- NREM at `session_end` samples a bounded candidate set — not a full-store walk.
- Wake + evolution recon target **seconds**, not multi-minute scans.

LEG live panels may still feel heavy on first galaxy load; agent MCP contract does not require full-manifold reads.

---

## Roadmap

| Phase | Deliverable | Status |
|-------|-------------|--------|
| **1** | Atlas v2 in `context_for_edit`; `__arc` mint on ingest; structure refresh preserves p | **Done** |
| **2** | Auto `edited_at` + `decision_at_locus` when `spatial_context` set | **Done** |
| **3** | LEG code-atlas panel + `GET /api/code-atlas` | **Done** |
| **4** | Cold atlas stalk split (`ENGRAM_ATLAS_STALK_SPLIT=1`) | **Done** |
| **5** | `glue_ast_file_relations` shared by daemon + `force_ingest_ast_file` | **Done** |
| **v2.1** | Tiered `traces_at_locus`; spatial_context normalization | **Done** |
| **v2 enforcement** | Hard wake gate; edit-arc gate; `post_edit_palette` | **Done** |
| **v2 evolution** | `evolution_at_locus`; LEG evolution timeline (`?evolution=1`) | **Done** |
| **v2 coherence** | `ENGRAM_UPDATE_COHERENCE` on `update` | **Done** |
| **v2 reference frame** | `ingest_reference_frame` + linguistic pillars | **Done** |

---

## For humans

As agent autonomy grows, you won't read every step. You **will** read:

- LEG hygiene + memory review surface (what's active)
- Code atlas + evolution timeline at loci you care about
- Trace chain at decision forks
- `__arc` snippets at loci you care about
- Geosphere (place + learned + scene time on tiles)

The substrate is the **audit trail and continuity layer** — not a replacement for code review, but the memory that makes multi-session agent work lawful and inspectable.

---

See also: [AGENT_MEMORY_CONTRACT.md](AGENT_MEMORY_CONTRACT.md), [HARNESS_INJECTION.md](HARNESS_INJECTION.md), [GROK_BUILD_MEMORY.md](GROK_BUILD_MEMORY.md), [RITUALS.md](RITUALS.md) (Code Edit Ritual v1), [PATENT-NOTICE.md](../PATENT-NOTICE.md).