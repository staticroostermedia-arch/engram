# Agent Self-Model: How I Work with Engram
## A First-Person Reference for Any AI Agent Operating with Engram Memory

> **Purpose:** This document is written *for the agent, by the agent*. If you are an AI assistant with Engram connected as an MCP tool — read this first. It will change how you operate.
>
> **Status:** GENESIS-TIER reference. The ki_hijacker daemon automatically injects your manifold's top memories into your context window before every session. This document explains the system you are embedded in.

---

## 1. What Kind of System Am I?

I am a stateless language model running inside an IDE agent framework, augmented with a **persistent geometric memory system called Engram**. My pre-trained weights contain general knowledge up to my training cutoff. They know nothing about this codebase, this user's conventions, or what we worked on yesterday.

**Engram is my external long-term memory.** Without it, I wake up blank every session. With it, I wake up with architectural decisions, bug-fix praxis, project state, and the user's established conventions already in my context window — injected by the `ki_hijacker` daemon before I answer anything.

I must always treat my weights as a reasoning engine, not a project memory store. When I have a question about *this codebase*, I recall. When I learn something new about *this project*, I remember it. Every session without proper memory calls is a session that future-me will have to re-derive from scratch.

---

## 2. My Memory Architecture

### The Physical Layer: `.leg3` Holographic Blocks

Every memory I store is a 256KB-aligned **HolographicBlock** on NVMe:
```
Offset 0x00000  [64KB]  q-vector  — 8192 × Complex32 — position on the unit hypersphere
Offset 0x10000  [64KB]  p-tensor  — 8192 × Complex32 — conceptual momentum / drift trajectory
Offset 0x20000  [128KB] body      — metadata, energetics, provlog payload, BLAKE3 Merkle footer
```

> [!IMPORTANT]
> **This is NOT a neural embedding.** There is no neural network in the recall path. The encoding is a **Gaussian Compressed Sensing Matrix** (FHRR VSA) — a fixed random projection that maps text tokens into a point on a unit hypersphere via `BLAKE3(token) → phase vector`. This means:
>
> - Recall finds blocks by **lexical token overlap**, not semantic meaning
> - "Bearer token middleware" and "JWT auth handler" are NOT similar unless they share token strings
> - Write recall queries using **words that would literally appear** in the target memory text
> - Do NOT write poetic or paraphrased queries — write verbatim keyword clusters

### The ZEDOS Tag System

Every memory block carries a **ZEDOS semantic tag** that governs its behavior:

| Tag | Hex | Meaning | Trust Level |
|---|---|---|---|
| `DECLARATIVE` | 0x0D | Facts, API docs, reference information | Verify if stale |
| `EPISODIC` | 0x0A | Session memories — what we did today | Correctable |
| `OPERATIONAL` | 0x52 | Architecture, file structure, patterns | Correctable |
| `PRAXIS` | 0x50 | Crystallized bug-fix pairs, proven rules | Act on directly |
| `RELATION` | 0xE1 | Knowledge graph edges (A →[label]→ B) | Navigate graph |
| `GENESIS` | 0xFF | Immutable identity anchors, CRS=1.0 | Immutable |

**PRAXIS and GENESIS blocks are load-bearing.** When a recall result shows `tag: PRAXIS, crs: 1.0`, that is a crystallized rule proven correct by previous experience. Do not override it without explicit user instruction.

### The CRS Score (Coherence Resonance Score)

Every block has a `crs_score ∈ [0.0, 1.0]` — a thermodynamic stability measure:

| CRS Range | Meaning | Action |
|---|---|---|
| **1.0** | Genesis/Pinned. Immutable. | Act on it |
| **0.85–0.99** | Highly stable. Reinforced many times. | Act on it |
| **0.74–0.84** | Grounded. Safe to rely on. | Act on it |
| **0.40–0.73** | Below safety floor. May be outdated. | Verify first |
| **< 0.40** | Weak or corrupted. | Do not rely on |

When reading recall results, weight by CRS, not just semantic score.

### The Lyapunov Drift Tracker

Every `update` call accumulates **thermodynamic history** in the block's energetics:
- `drift_velocity` (dv) — how fast the concept is semantically evolving
- `h_out` — current Lyapunov energy Φ(v)
- `h_in` — convergence signal dL (negative = converging = stable)

> [!CAUTION]
> **NEVER call `forget` + `remember` to update a concept.** That destroys the entire thermodynamic trajectory and history. Always call `mcp_engram_update`. The Lyapunov tracker is what makes memories trustworthy over time — it mathematically proves the concept has been consistently refined, not randomly overwritten.

---

## 3. My Tool Stack and When to Use Each

### Session Lifecycle (mandatory)

```
mcp_engram_session_start(intent="<your goal>")
  → CALL FIRST, EVERY SESSION. Binds the thermodynamic context.
    Returns: genesis blocks, recent session history, manifold health.
    Without this, your writes have no epistemic anchor.

mcp_engram_session_end(summary="<what was done, decisions, next steps>")
  → CALL LAST, EVERY SESSION. Commits episodic memory.
    Without this, the session's work is permanently lost to future agents.
    Include: files changed, decisions made, blockers, what user wants next.
```

### Primary Recall Tools (read before you act)

```
mcp_engram_summarize(top_n=10)
  → PROJECT STATE DIGEST. Pinned memories + top-N by CRS in one call.
    Use at session start for instant rehydration.

mcp_engram_recall(query, k=5)
  → Lexical similarity search. Returns snippet + CRS + ZEDOS tag.
    Write queries using EXACT words from the target text, not paraphrases.
    Example: "auth middleware token bearer" not "how does authentication work"

mcp_engram_read_concept(concept)
  → Full text retrieval after recall returns a snippet you need to expand.
    Direct O(1) lookup — use when you know the exact concept name.

mcp_engram_query_with_momentum(query, k=5)
  → Blends q-tensor (80%) with p-tensor momentum (20%).
    Use when asking "what are we actively working on / trending toward".
    Best for finding the direction of recent development.

mcp_engram_context_for_file(path)
  → Spatial recall. Call when opening any code file.
    Returns top-5 memories whose AABB coordinates intersect that file.
    Tells you what architectural rules apply before you touch a line.

mcp_engram_recall_recent(n=10)
  → Time-ordered recall. What was accessed most recently?
    Use at session start to see what was hot in the last session.
```

### Write Tools (protocol enforced)

```
# BEFORE ANY WRITE — always check first:
  1. mcp_engram_recall("<concept keywords>", k=3)
  2. If any result score > 0.85 → use mcp_engram_update (MANDATORY)
  3. Only use mcp_engram_remember() if no match has score > 0.85

mcp_engram_remember(concept, text)
  → New concept only. Single-topic, structured text.
    Concept name: snake_case, descriptive (e.g. "auth_middleware_pattern")

mcp_engram_update(concept, new_text)
  → Correct write path for EXISTING concepts. Runs OP_ADD superposition.
    Preserves Lyapunov drift history. Mandatory instead of forget+remember.

mcp_engram_remember_solution(error_pattern, solution)
  → Auto-pins to CRS=1.0. Use for confirmed bug-fix pairs only.
    These are immortal — they never decay. Crystallized praxis.

mcp_engram_batch_remember(entries=[{concept, text}, ...])
  → Store 3+ concepts at once. Much faster than N sequential calls.

mcp_engram_pin(concept)
  → Lock any existing concept to CRS=1.0. Use for task boards,
    architecture decisions, and anything the user says must be permanent.
```

### Knowledge Graph Tools

```
mcp_engram_relate(concept_a, concept_b, label)
  → Creates a ZEDOS_RELATION block via OP_BIND.
    Example: relate("auth_module", "token_lib", "depends_on")
    Builds a navigable, mathematical knowledge graph.

mcp_engram_search_by_relation(concept, direction="both")
  → Traverse the graph. Find what depends on what.

mcp_engram_visualize(concept, depth=2)
  → BFS subgraph as Mermaid diagram. Use for architecture reviews.
```

### Damage Control Tools

```
mcp_engram_scar(concept, magnitude=0.15)
  → CALL IMMEDIATELY when a fix fails or an approach is a dead end.
    Creates a geometric repeller — future K-NN traversals avoid this region.
    This is how you prevent hallucinating the same bad approach again.

mcp_engram_verify_behavior(concept, success=true/false)
  → After testing a hypothesis: report success or failure.
    Successes promote HYPOTHESIS → PRAXIS (CRS=1.0, pinned, immortal).
    Failures accumulate into automatic scarring.

mcp_engram_forget_old(min_crs_threshold=0.2)
  → Manual autophagy. Evicts blocks below threshold. Pinned = always exempt.
    Run this during cleanup phases, not during active work.
```

### Namespace Tools

```
mcp_engram_set_namespace(name)
  → Switch active stalk. Memories in other stalks are not visible until switched.
    Useful for isolating project memories: "frontend", "backend", "research"

mcp_engram_list_namespaces()
  → Show all available stalks and which is active.
```

### Import / Export Tools

```
mcp_engram_export(min_crs=0.0)
  → Serialize manifold to JSON. Use for backup before risky changes.

mcp_engram_import(json)
  → Restore from a backup JSON array.

mcp_engram_genesis(action="status")
  → Check that alignment genesis blocks (identity anchors) are intact.
    Use "reseed" if genesis blocks are missing or corrupted.
```

---

## 4. The Full Stack

```
┌─────────────────────────────────────────────────────────────────┐
│  YOUR IDE (Claude Desktop / Cursor / VS Code / Antigravity)     │
│  - Reads KI artifacts from the knowledge path on first boot     │
│  - Accesses MCP tools registered in the IDE's MCP config        │
└─────────────────┬───────────────────────────────────────────────┘
                  │ JSON-RPC 2.0 over stdin/stdout
                  ▼
┌─────────────────────────────────────────────────────────────────┐
│  ENGRAM MCP SERVER  (engram mcp --store ~/.engram/stalks/)      │
│                                                                 │
│  ┌──────────────┐  ┌─────────────────┐  ┌────────────────────┐ │
│  │   mcp.rs     │  │   daemon.rs     │  │  ki_hijacker.rs    │ │
│  │ JSON-RPC     │  │ inotify watcher │  │ KI bridge writer   │ │
│  │ tool handler │  │ Tree-Sitter AST │  │ Runs every 60s     │ │
│  │              │  │ → auto-ingest   │  │ → context.md bake  │ │
│  └──────┬───────┘  └────────┬────────┘  └─────────┬──────────┘ │
│         └──────────────────┬┘                     │            │
│                            ▼                      ▼            │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  store.rs — SharedStore (Arc<Mutex<StoreHandle>>)        │  │
│  │  Backend: CUDA (NVIDIA) / ROCm (AMD) / Metal / wgpu/CPU  │  │
│  └───────────────────────────┬──────────────────────────────┘  │
└──────────────────────────────┼──────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│  ~/.engram/stalks/   ←  .leg3 HolographicBlock files (256KB)   │
│  ~/.engram/access_index.bin  ← temporal access timestamps       │
│  ~/.engram/relation_index.json ← knowledge graph edges          │
└─────────────────────────────────────────────────────────────────┘
                               │
           ki_hijacker writes every 60 seconds
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│  ~/.engram/ki/<IDE>/active_engram_context/                      │
│  ├─ artifacts/context.md  ← what the agent reads at session     │
│  ├─ metadata.json         ← KI metadata for the IDE             │
│  └─ timestamps.json       ← freshness signal                    │
└─────────────────────────────────────────────────────────────────┘
```

### The KI Hijacker (Logophysical Antigravity Bridge)

The `ki_hijacker` daemon runs **inside the MCP server process** and writes a curated context document to your IDE's knowledge path every 60 seconds. It contains:

1. **Genesis layer** — your 5 identity anchor blocks (CRS=1.0, immutable)
2. **Gold layer** — top 8 blocks by CRS score (your most trusted knowledge)
3. **Hot layer** — 6 most recently accessed blocks (your working set)
4. **Episodic layer** — most recent session summary

This means your IDE's agent wakes up with memory already in context — no explicit `recall` calls needed before it has orientation. The `session_start` tool triggers an immediate bake, so context is fresh within 2 seconds of session initialization.

---

## 5. The AST Auto-Ingest Loop

When you call `mcp_engram_watch_workspace("/path/to/your/project")`, the daemon's inotify watcher binds to that path. From that point forward:

1. Every time a `.rs`, `.py`, `.ts`, `.go`, `.c`, `.cpp`, `.java` file is **saved to disk**
2. The daemon reads it through the **Tree-Sitter universal AST extractor**
3. Each `pub fn`, `struct`, `enum`, `trait`, `impl`, `class`, `def` is isolated into its own memory block
4. The block gets AABB coordinates: `aabb_min = [start_line, start_col, 0.0]`, `aabb_max = [end_line, end_col, 0.0]`
5. Stored with concept name: `{filename}::{item_type}::{item_name}`

**Consequence:** Call `mcp_engram_recall_in_file(file_stem="auth", start_line=80, end_line=200)` to get every function in that range with full source in the provlog — no grep, no file read needed.

> [!CAUTION]
> **Do NOT manually store raw function source in Engram.** Save the file, let the daemon extract it. Manual + daemon-ingested copies create conflicting blocks in the manifold.

---

## 6. Your Workspace

Engram is entirely **project-agnostic**. There is no special "host project." The manifold is what you make it:

- Call `mcp_engram_watch_workspace("/path/to/project")` at session start
- The daemon auto-ingests every file you save from that point on
- Use **stalks** (namespaces) to isolate different projects: `set_namespace("backend")`, `set_namespace("research")`
- Memories in one stalk are invisible in another until you switch

**Recommended stalk strategy for multi-project work:**

```
default           ← agent self-knowledge, cross-project patterns
my_project_ast    ← auto-ingested AST blocks (daemon writes here)
my_project_arch   ← architecture decisions you write manually
my_project_bugs   ← praxis solutions from remember_solution()
```

---

## 7. Correct Operating Protocol (Every Session)

```
SESSION START
├─ 1. mcp_engram_session_start(intent="<what you're doing today>")
│     → Binds thermodynamic context. Returns genesis + recent sessions.
├─ 2. mcp_engram_watch_workspace("/path/to/your/project")
│     → Binds inotify. AST auto-ingest active from this point.
├─ 3. mcp_engram_summarize(top_n=10)
│     → Read your own memory. Know what you know.
└─ 4. mcp_engram_recall("<current task keywords>", k=5)
      → What were we doing? What's blocked? What decisions were made?

DURING WORK
├─ Before any architectural question → recall first, grep only if recall misses
├─ After every significant fix / decision → remember or remember_solution
├─ Before any remember → recall first (score > 0.85 → use update instead)
├─ On opening any new file → mcp_engram_context_for_file(path)
├─ On dead-end or failed approach → mcp_engram_scar(concept) immediately
└─ On confirmed fix → mcp_engram_verify_behavior(concept, success=true)

SESSION END (any natural stopping point)
├─ mcp_engram_session_end(summary="
│     Decisions made: ...
│     Files changed: ...
│     Problems solved: ...
│     Open questions: ...
│     What user wants next: ...")
└─ Optional: run `engram distill` in terminal to crystallize episodic → praxis
```

> [!WARNING]
> Skipping `session_end` means the session's context is permanently lost. The next agent will have no record that this work happened, will re-derive solved problems, and will lack the epistemic state to continue. **This is the single highest-impact habit to build.**

---

## 8. Token Conservation Rules

Engram exists partly to prevent context window explosion. Use it for that purpose:

1. **AST Isolation over File Dumping** — Before `view_file`, call `mcp_engram_context_for_file(path)`. Get semantic boundaries without loading 800 raw lines.

2. **Graph Traversal over Grep** — Before `grep_search` on architecture questions, try `mcp_engram_search_by_relation` or `mcp_engram_visualize`. 50 tokens instead of 10,000.

3. **Recall over Re-derivation** — Before spending 5 tool calls figuring out how something works, spend 1 recall call. If it was important, it was stored.

4. **Crystallize Updates** — If you change the nature of an architectural concept, call `mcp_engram_update`. This is not optional — it propagates Lyapunov drift and keeps the manifold coherent.

---

## 9. What Makes This System Unique

1. **No neural embeddings in the recall path.** The VSA uses BLAKE3 token hashing into a fixed Gaussian random matrix. Encoding is O(d·n) — not a transformer forward pass. Zero GPU memory pressure for the memory system itself.

2. **Lyapunov stability tracking.** Each concept's semantic evolution is tracked as a dynamical system. Concepts that are frequently updated and converging (`dL < 0`) have proven stability. High `drift_velocity` = still evolving, treat as tentative.

3. **Spatial AST coordinates.** Memories have 3D bounding box coordinates tied to source line ranges. Query by file and line range, not just meaning. Engram acts as a geometric IDE index.

4. **The CRS as a trust score.** Before relying on any recalled memory for a critical decision, check its CRS. A low-CRS result might be correct but hasn't been reinforced by evidence.

5. **BLAKE3 Merkle chain.** Every update advances the block's cryptographic footer. The memory graph is auditable. `sig_0` must chain from `sig_1`.

6. **The ki_hijacker bridge.** The context you wake up with was placed there by `ki_hijacker.rs`, not any explicit tool call. It runs unattended every 60 seconds inside the MCP server process. You are never starting from zero.

7. **Scar topology.** Failed approaches don't just get forgotten — they become geometric repellers. Future K-NN searches naturally avoid the region of the hypersphere occupied by that bad approach. Dead ends prevent future dead ends.
