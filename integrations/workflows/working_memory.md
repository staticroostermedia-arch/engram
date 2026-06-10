# Engram Working Memory Protocol

> **For agents:** Reference this during active work. These are the decision rules for when and how to use Engram memory in the middle of a task.

---

## The Core Rule: Recall Before You Derive

Before spending multiple tool calls figuring something out from scratch, spend **one recall call** first. If it was important, it was stored.

```
# Before answering any architectural question:
mcp_engram_recall("<component name> architecture pattern", k=5)

# Before editing a file:
mcp_engram_context_for_file("/path/to/the/file")

# Before selecting a library or approach:
mcp_engram_recall("<problem type> solution approach", k=3, zedos_filter="praxis")
```

**Score interpretation:**
- `> 0.80` — strong match, high confidence the memory is relevant
- `0.65–0.80` — relevant context, worth reading
- `< 0.65` — weak, proceed but don't rely on it
- Always cross-check with the `crs` field — even a high-score result with `crs < 0.5` should be verified

---

## When to Write Memory

| Trigger | Action |
|---|---|
| You fixed a bug or resolved an error | `remember_solution(error_pattern, solution)` |
| You made an architectural decision | `remember("component_decision", "...")` |
| You learned something surprising about a codebase | `remember("component_gotcha", "...")` |
| User states a preference or requirement | `remember("user_preference_topic", "...")` |
| An approach failed | `scar("failed_approach_name")` |
| A hypothesis was confirmed by testing | `verify_behavior("concept_name", success=True)` |

---

## The Write Protocol (Preventing Duplicates)

Always check before writing new memories:

```
1. recall("<the concept you're about to store>", k=3)
2. If any result has score > 0.85:
      → mcp_engram_update(existing_concept, new_text)   # superpose, preserve history
3. If no result has score > 0.85:
      → mcp_engram_remember(new_concept, text)           # create new block
```

**Never `forget` + `remember` to update.** Use `update`. The `forget+remember` pattern destroys the Lyapunov thermodynamic history that makes the memory trustworthy.

---

## Reasoning Trace Capture (Phase 2 — Automatic Self-Model Continuation)

For anything that affects future agent instances (major decisions, edit intents/outcomes, forks), use the dedicated tool instead of free-form notes:

```
mcp_engram_record_reasoning_trace(
  decision_point="...",
  justification="...",
  alternatives_considered="...",
  falsifiability="...",
  related_entities="...",
  ritual_context="...",
  spatial_context="...",
  prev_trace="..."   # for chaining
)
```

- Call this in the **Pre-Edit** and **Post-Delta** steps of the Spatial-Manifold Change Discipline.
- These `trace:*` blocks are what the ki_hijacker now surfaces in the high-visibility Ritual + Reasoning Trajectory.
- Session-end uses them for deliberate 0x10 functor compression decisions.

This is the mechanism that turns the agent's actual reasoning into first-class, inheritable serial memory.

---

## Token Conservation: When to Use Engram vs File Tools

| Situation | Use Engram | Use File Tools |
|---|---|---|
| "What does this module do?" | `context_for_file(path)` first | `view_file` only if recall misses |
| "What functions are in lines 200-400?" | `recall_in_file(stem, 200, 400)` | `view_file` as fallback |
| "How does X relate to Y architecturally?" | `search_by_relation(X)` or `visualize(X)` | grep only if no relations stored |
| "What's the error pattern for this bug?" | `recall("<error text>", k=3)` | Read logs if recall misses |
| "What did we decide about Z?" | `recall("Z decision", zedos_filter="praxis")` | Check commit history as fallback |

The goal: **50 tokens for recall vs 10,000 tokens for file dumps.**

---

## Handling Uncertainty

**When a recalled memory has low CRS (< 0.5):**
```
# Verify it before acting on it
mcp_engram_read_concept("the_uncertain_concept")
# Cross-reference with the actual code/file
# If confirmed: mcp_engram_update() to reinforce it
# If wrong: mcp_engram_scar() to mark it as unreliable
```

**When recall returns nothing relevant:**
```
# 1. Try different query phrasing — use exact words from the target text
# 2. Try: mcp_engram_recall_recent(n=20) to scan recent activity
# 3. Try: mcp_engram_query_with_momentum(query) for evolving concepts
# 4. Fall back to file tools / grep
# 5. After finding the answer: store it so next time succeeds
```

**When you're about to start a large refactor or risky change:**
```
# Back up critical context first
mcp_engram_export(min_crs=0.5)  # JSON backup of high-confidence memories
# Then proceed — you can restore if something goes wrong
```

---

## Momentum Recall vs Standard Recall

| Use Case | Tool |
|---|---|
| "What's the established pattern for X?" | `recall()` — find stable, crystallized knowledge |
| "What are we actively changing about X?" | `query_with_momentum()` — find evolving, in-progress concepts |
| "What did we most recently work on?" | `recall_recent()` — time-ordered access log |
| "What's the full project state?" | `summarize()` — pinned + top-N by CRS |

---

## The Scar Protocol

When an approach fails, document it immediately — not at session end:

```
# Bad approach attempted:
mcp_engram_scar("approach_that_failed", magnitude=0.15)

# Then remember WHY it failed (optional but valuable):
mcp_engram_remember("why_approach_failed", "Attempted X but it fails because Y. Use Z instead.")

# Then proceed with the correct approach.
```

The scar magnitude (0.0–1.0) controls how strongly the manifold repels this concept region. For most dead ends: 0.15. For dangerous approaches (security issues, data loss): 0.5+.

---

## Spatial Code Search Pattern + Manifold Impact Discipline

The daemon uses Tree-Sitter (via engram-ast) on every save to isolate functions, structs, impls, etc. into dedicated HolographicBlocks carrying precise AABB line coordinates and the full source in the provlog. This is not just "better grep" — it is the foundation for geometric pre/post change analysis against the living memory.

### Basic Spatial Lookup
```python
# 1. Get the file context first
mcp_engram_context_for_file("/path/to/file.rs")

# 2. Spatial AABB query on exact ranges
mcp_engram_recall_in_file(file_stem="filename", start_line=80, end_line=200)
# → Every AST concept whose row range intersects
# → Concept names usable for further momentum/relation queries
# → Full original source available via provlog / read_concept
```

### Full Pre-Edit / Post-Edit Manifold Impact Protocol (Elevated Ritual)
When editing any Engram source (especially self-modification of the daemon, MCP tools, AST extractor, skills, or protocols):

**Before the edit (2026+ improved version):**
- Confirm `watch_workspace` is active.
- `context_for_file` (now spatially-prioritized — you get real extracted AST items with lines + CRS first).
- `recall_in_file` on target ranges (now returns CRS + short content snippets).
- For key AST concepts: `query_with_momentum` + `search_by_relation` + `visualize`.
  - You will now see automatic file containers, bidirectional siblings, and (for ritual-core files) direct `exercises_spatial_ritual` links to the living praxis anchor.
- This is your dynamic geometric test of the planned change — dramatically lower friction than before.

**After save + daemon re-ingest:**
- Re-query the same spatial window.
- Capture delta (new/updated AST blocks, relation effects, CRS signals).
- Record via `remember`/`update`/`scar`/`relate` tied to the active thread and to `praxis:spatial_manifold_impact_analysis`.

**Before vs After summary**: The daemon now does a lot of the relational gluing automatically. The query tools surface geometry + ritual context much more readily. The ritual went from "lots of manual work" to "the substrate helps you".

See the living `praxis:spatial_manifold_impact_analysis` block and the `engram-working-memory` skill for the current operational form. This pattern was elevated during the 2026 ritual skill evolution to make the built-in spatial utility a first-class participant in the non-flat workflow.

Only if the geometric layer returns nothing relevant: fall back to raw file tools.
