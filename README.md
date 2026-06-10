# Engram

[![Build Status](https://github.com/staticroostermedia-arch/engram/actions/workflows/rust.yml/badge.svg)](https://github.com/staticroostermedia-arch/engram/actions)
[![MCP](https://img.shields.io/badge/MCP-Native-blue)](https://github.com/modelcontextprotocol)
[![Glama](https://glama.ai/mcp/servers/staticroostermedia-arch/engram/badge)](https://glama.ai/mcp/servers/staticroostermedia-arch/engram)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-purple)](LICENSE)
[![Patent Pending](https://img.shields.io/badge/Patent-Pending-orange)](PATENT-NOTICE.md)
[![Geometric Memory](https://img.shields.io/badge/Geometric-Non--flat%20sheaf%20%2B%20rituals-8A2BE2)](docs/GEOMETRIC_MEMORY.md)

> **Persistent geometric memory for AI agents — one-call wake, anchor-first recall, edit-scoped spatial context, structured handoff. 8 essential MCP tools (62 total for power users). Hardware-native 256KB HolographicBlocks (q/p/CRS/Merkle), VSA/sheaf gluing, spatial AABB, rituals, NREM/ego.leg3. Runs local. Survives 200k-block stores without OOM. See [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md), [docs/GROK_BUILD_MEMORY.md](docs/GROK_BUILD_MEMORY.md), [docs/GEOMETRIC_MEMORY.md](docs/GEOMETRIC_MEMORY.md).**

**New here?**

| Path | Start here |
|------|------------|
| **Grok Build / Cursor / Claude (recommended)** | [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md) + [docs/GROK_BUILD_MEMORY.md](docs/GROK_BUILD_MEMORY.md) + [FIRST_RUN.md](FIRST_RUN.md) |
| **Agent rituals (skills)** | [SKILLS.md](SKILLS.md) → `docs/skills/engram-wake-up.md` |
| **Deep geometric / TUI users** | [HOW_WE_ACTUALLY_USE_THIS_IN_2026.md](HOW_WE_ACTUALLY_USE_THIS_IN_2026.md) + [docs/RITUALS.md](docs/RITUALS.md) |
| **BYOP substrate builders** | [AGENT_INTEGRATION_GUIDE.md](AGENT_INTEGRATION_GUIDE.md) |

**Human review surface:** Run `./scripts/leg` (static) or `./scripts/leg --live` (dynamic) for Primary Intent, traces, momentum, relations, Thought Tiles (text + HTML viz).

Engram is not a vector database. It is a **persistent geometric memory engine** (hardware-native 256KB HolographicBlocks on NVMe with O_DIRECT/GPUDirect, VSA calculus, sheaf gluing via relations, symplectic frames, CRS Lyapunov stability, hot/NREM consolidation, lawfulness gates, scar mechanics). It has **no opinion on your use case**. Sophisticated agents bring their own perspective (BYOP — "Build Your Own Perspective") and construct their own tuned manifolds on the shared substrate. High-quality use reveals deeper structure (category theory + calculus over memory) after the fact.

**See also:** [docs/GITHUB_MVP_PREP_PLAN.md](docs/GITHUB_MVP_PREP_PLAN.md) (current prep for public representation) and [MANIFESTO.md](MANIFESTO.md).

No cloud. No API keys. Runs on your machine via MCP — **8 essential tools** for daily agent work, 62 total for power users ([tier reference](docs/MCP_TOOLS_REFERENCE.md)). Build: `cargo build -p engram-server && target/debug/engram --version`.

## Memory Model: Geometric (Non-Flat) vs Flat

Engram replaces flat vector DBs / append-log RAG with a **geometric sheaf**:

- **HolographicBlock (.leg3)**: 256KB fixed (q 8192D phase tensor, p momentum, CRS Lyapunov, BLAKE3 Merkle provenance, AABB spatial, provlog).
- **VSA Calculus**: OP_ADD (superpose), OP_BIND (role-filler, invertible), OP_GEOMETRIC_PRODUCT etc.
- **Sheaf + Relations**: Declarative processes/*.toml define gluing (H¹ handlers, subvisor for governance). `relate` / `search_by_relation` / `visualize` traverse real OP_BIND edges.
- **Spatial AABB (Item 1.5)**: tree-sitter AST on save; `context_for_file`, `recall_in_file`, `force_spatial_ingest`. Code Edit Ritual pre/post recon mandatory.
- **Rituals for Integrity**: wake-up / working-memory / session-end (continuation bundles, hot promotion, COMPRESS), scar (repulsion), verify_* (manifold/block lawfulness), remember_solution, record_reasoning_trace (A/D/R + goal/spatial).
- **Continuation & Self-Model**: agent_instance_continuation, ego.leg3 / NREM, thought tiles, goal stack as first-class.
- **Lawfulness / Subvisor**: Phase 1.5 metrics, process sheaf (monitor/subvisor OP_INVERT/H¹ for sub-agent doom-loop prevention).

See [docs/GEOMETRIC_MEMORY.md](docs/GEOMETRIC_MEMORY.md) and [docs/RITUALS.md](docs/RITUALS.md) for full.

## Categorical Linguistic Calculus

Engram now supports native categorical reasoning directly over linguistic structures (words, discourse bundles) and mixed with numeric phases — inside the same geometric sheaf. This is structure-preserving synthetic calculus (differentiate/integrate/operadic) with homotopy coherence, fibered CRS guards (>=0.74), persisted via .leg3 + NREM/ego.leg3 promotion (CRS 0.85+ roundtrips/homotopy).

See full details, P1-6 surface (ZEDOS_LINGUISTIC 0x4C etc, mcp_linguistic_calculus + mixed ops), bridging examples, and invariants in [docs/CATEGORICAL_LINGUISTIC_CALCULUS.md](docs/CATEGORICAL_LINGUISTIC_CALCULUS.md).

**Usage example (via mcp_linguistic_calculus):**
```
result = mcp_linguistic_calculus({
  "bundle": {"words": [{"text": "hello", "coeff": [0.92, 0.1, ...]}, ...], "bundle_id": "d1"},
  "operation": "differentiate"
})
# returns {crs: 0.87, result_bundle, phase_preview, ...}
```

**Mixed word/number bridge ex:** coeff scalar on phase (word→num) or num param shift on linguistic under fibered guard + CRS check (class-mixing scar on violation). Full roundtrip: mint mixed → compress → calc (e.g. integrate) → decompress → NREM/ego.leg3 (fidelity >=0.85).

**How to try the new calculus:** `cargo build -p engram-server && target/debug/engram --version`; wire MCP (ENGRAM_PROFILE=agent); `mcp_engram_session_start(intent="explore linguistic calculus")`; call mcp_linguistic_calculus (or extend examples/hello-engram-agent.py); verify `mcp_engram_verify_manifold_integrity` (min_crs 0.74). Full hygiene: cargo test -p engram-core -p engram-server, target/debug/engram --version.

This advances Engram from geometric memory to geometric *reasoning* substrate (calculus over the manifold). Public Phase 6 polish complete.

## Comparison to Popular Memory Projects

| Aspect | Engram | mem0 / Letta | qdrant / chroma / milvus | ragflow |
|--------|--------|--------------|---------------------------|---------|
| Core Model | Geometric sheaf (q/p/CRS/Merkle + relations + VSA + H¹ gluing) | Flat vector + metadata / graph append | Vector DB (ANN, collections) | RAG pipeline over vectors |
| Momentum / Trajectory | p-tensor native, query_with_momentum | Limited recency | None (static) | Temporal via workflow |
| Spatial / Code | AABB AST (tree-sitter per save), recall_in_file | Chunk text | None | Document chunks |
| Rituals / Hygiene | scar, verify_*, record_reasoning_trace, Code Edit Ritual, subvisor | Basic | None | Pipeline steps |
| Continuation | Bundles, session_end handoff, hot path, ego.leg3/NREM | Session state | None | Workflow state |
| Declarative Processes | 7+ tomls (ritual/harness/operator/monitor/subvisor) registered at start | Config | None | YAML flows |
| Hardware Native | 256KB .leg3, O_DIRECT, GPUDirect, LBVH, 8192D phase | CPU/GPU vectors | Index on CPU/GPU | LLM + vector |
| MCP / Agent Native | 8-tool lean contract + 62 MCP tools tiered, process sheaf | API/ SDK | gRPC/REST clients | API |
| Self-Model / Lawfulness | CRS gates, lawfulness metrics, scars deflect | Logging | Metrics | Eval hooks |

(Emulates polish from popular while preserving Engram's non-flat identity. Full details + gaps closed in [docs/GITHUB_MVP_PREP_PLAN.md](docs/GITHUB_MVP_PREP_PLAN.md).)

---

## 🚀 Quick Start

```bash
# Clone and install from source
git clone https://github.com/staticroostermedia-arch/engram.git
cd engram
cargo install --path crates/engram-server

# Verify install
engram --version
# engram-server 0.4.x
```

Add to your MCP config and **restart your IDE/TUI**:

```json
{
  "mcpServers": {
    "engram": {
      "command": "/path/to/Engram/scripts/engram-grok",
      "args": ["mcp"],
      "env": {
        "ENGRAM_STORE": "~/.engram/stalks/",
        "ENGRAM_PROFILE": "agent"
      }
    }
  }
}
```

Or: `cp scripts/engram-grok ~/.local/bin/` and use `"command": "engram-grok"`.

**Agent first call:** `mcp_engram_session_start(intent="your goal")` — inline continuation bundle.

**All ecosystems:** [`integrations/README.md`](integrations/README.md) (Grok, Cursor, Claude, Antigravity, Codex, local agents). Contract: [`docs/AGENT_MEMORY_CONTRACT.md`](docs/AGENT_MEMORY_CONTRACT.md).

**Dual Quickstart Paths**

**Lean agent path (recommended — Grok Build default):**

1. Install + MCP config with safe env (above).
2. Load [`docs/AGENT_MEMORY_CONTRACT.md`](docs/AGENT_MEMORY_CONTRACT.md) into agent instructions.
3. `session_start(intent)` — one call, inline bundle.
4. Work: `context_for_edit(path)` → `recall(scope=anchors)` → `quick_trace` / `remember`.
5. `session_end(summary)` — handoff for next session.

**Deep path:** Tiles, goals, relation graphs, lawfulness audits — see [RITUALS.md](docs/RITUALS.md) + [HOW_WE_ACTUALLY_USE_THIS_IN_2026.md](HOW_WE_ACTUALLY_USE_THIS_IN_2026.md).

> 🔌 **Optional — enable neural semantic search:** set `ENGRAM_EMBED_URL=http://localhost:8086/v1/embeddings` to point at any OpenAI-compatible local embedding server (llama.cpp, ONNX, nomic-embed). Without it, Engram falls back to BLAKE3 spiral-phase encoding — everything still works.

---

> 🧠 **Human review surface (the daily driver for seeing what the agent is carrying):**  
> Run from repo root:  
> `./scripts/leg` — instant STATIC curated view of current Primary Intent, recent structured traces, momentum, relations, and dual Thought Tiles (text + rich HTML visualization).  
> `./scripts/leg --live` — starts the server for dynamic/LIVE updates.  
> 
> STATIC mode is already very useful for review. The deeper integration with living goals and fully dynamic Activity Canvas continues to improve. See the launcher help and the handoff guide above for current honest expectations.

---

> 🧠 **For agents using Engram:** The [KnowledgeMint Protocol](AGENT_INTEGRATION_GUIDE.md#7-knowledgemint-protocol--session-knowledge-crystallization)
> defines the mandatory minting discipline that makes the [Inheritance Principle](PHILOSOPHY.md#the-inheritance-principle)
> operational. Read it before your first session. Every fact you mint is intellectual
> inheritance for every future agent session that uses this system.

---

## ⚡ Why 256KB? The Hardware-Native Advantage


Engram maps your project's memory into strict 262,144-byte (256KB) containers called **HolographicBlocks**. This size is non-arbitrary.

- **Native Tensor Load:** 256KB aligns perfectly to 64× 4KB hardware pages. The `.leg3` format is a strict C-struct — zero JSON decoding, zero Protobuf parsing.
- **O_DIRECT and GPUDirect Storage (GDS):** Engram bypasses the OS page-cache. When your agent searches for a memory, the tensor streams via DMA from NVMe directly into CPU registers or GPU VRAM via NVIDIA cuFile APIs.
- **Zero-Copy Architecture:** GPUDirect Storage eliminates the CPU bounce buffer. Tensors transfer directly over PCIe to the GPU for parallel distance calculations — scan rates in the GB/s range with near-zero CPU overhead.

Every block fuses the full source text, an 8192-dimensional semantic tensor, spatial 3D bounds (for code placement), a BLAKE3 Merkle chain proof, and a thermodynamic confidence score (CRS).

*(See [docs/architecture.md](docs/architecture.md) for a deep dive into the container format, cuFile integration, and LBVH scaling.)*

---

## 🛠️ For External Agents & Other Groks: Load the Rituals (Public Skills)

**The skills we actually use are now public so *your* agents can follow the exact same operating procedures.**

See the new `docs/skills/` directory:
- `docs/skills/README.md` — Index + quickstart loop.
- `docs/skills/engram-wake-up.md` — Geometric continuation protocol (Phase 0-5, living anchors via momentum/relations, lawfulness metric, spatial hygiene).
- `docs/skills/engram-working-memory.md` — The runtime discipline (momentum/relation/spatial first, update-preferred mutations, mandatory traces + scars, Code Edit Ritual pre/post AABB, thought tiles for meta-work, hot promotion, expensive tool hygiene).
- `docs/skills/engram-session-end.md` — Terminal momentum handoff (crystallize traces/goals/anchors, COMPRESS markers, hot promotion, legominism high-lineage, produce continuation target).
- `docs/skills/engram-thought-tiles.md` — Structured offload (when mandatory for meta, types, promote_hot).

**Load these files** (or the full content) into your agent's context when starting Enram-integrated work. They are the published "source code" for the ritual layer on top of the 55+ MCP tools.

See also:
- `docs/RITUALS.md` for the full overview + Code Edit Ritual + subvisor/governance.
- `docs/MCP_TOOLS_REFERENCE.md`
- `docs/GEOMETRIC_MEMORY.md`
- `processes/` (the 7+ declarative .toml ritual/harness/operator/monitor/subvisor — registered dynamically at session_start as first-class process: blocks with gluing).

**Minimal High-Quality Loop for Your Agent**:
1. Connect engram MCP.
2. `mcp_engram_session_start` (rich intent).
3. Follow `docs/skills/engram-wake-up.md`.
4. Work following `docs/skills/engram-working-memory.md` (pre spatial for edits, traces, tiles for meta, update over remember).
5. End with `docs/skills/engram-session-end.md` (produce real terminal state).

This is how agents get *geometric continuation* instead of flat resets. Dogfood it on your own meta-work.

**As Grok**: This is what I want every other capable agent to have — the exact protocols, not hints. Flat context wastes time and capability. These close the loop so the manifold compounds agent intelligence across instances.

The primary deep demonstration is still the TUI embodiment, but the public surface + skills/ now makes the full power immediately usable by sophisticated external agents without gatekeeping.

**Phase 3 deliverables (prominently surfaced for utility):**
- **Python EngramBYOPClient + Effective Usage Patterns:** `integrations/python/engram_client.py` (full source). Includes `BYOPProjection`, `EngramBYOPClient` with `set_perspective_frame`, `create_perspective_tile` (auto-wires provenance + spatial_refs to coordination), `bind_projection`, `emit_perspective_trace`, query helpers, + 5 concrete patterns with Hermes (mythic ontology) and OpenClaw (tool/agentic) examples.
- **"Build Your Own Geometric Memory Substrate" 7-Step Guide + Templates:** Living formal_spec `tile:formal_spec_external-agent-how-to--build-your-own-geometric-` (core primitives exposed, copy-paste sample payloads for formal_spec/ontology, trace formats, goal decomp, client/MCP snippets).
- **PGFS v0.1 (Process Geometry Feedback System):** `tile:formal_spec_pgfs-v0-1--process-geometry-feedback-system---ea` + scar-density/H¹ prototype helper. Extended lawfulness checklist items **9-14** (ritual friction audit, process H¹/scar_density on coordination subgraphs via `search_by_relation`+`visualize`, escalation protocol via `escalates_to`, process invariants first-class, self-improving loop, no violations). **Quote relevant items in every trace.** Healthy construction for *any* agent's army, including yours.
- **Living Coordination Surface:** `tile:knowledge_graph_phase-1-cross-workstream-coordination--ws1-hot-p` (primary colimit; discover projections via `search_by_relation(..., label="projects_as")`, gluing examples, Phase 2/3 artifacts).
- **New Public Guide:** `tile:formal_spec_getting-started-as-an-external-agent--neutral-ge` + HTML viz companion (this section distilled + full framing + PGFS quotes + links).

**Minimal High-Quality Loop (from patterns + 7-step):**
1. Connect + `session_start`.
2. Orient + project your prefixed ontology (`formal_spec` tile).
3. Use (remember/relate/thought_tiles/goals) with your labels + `spatial_references` + `goal_context`.
4. Before decisions: `query_with_momentum`; on failure: `scar`; success: `remember_solution` + relate.
5. End chunks: `session_end` + bind/glue projections to coordination.
6. Audit: `search_by_relation` + `visualize` on your subgraphs (H¹ holes, scar density per PGFS helper).
7. Iterate: `update` (preserve history), relate new artifacts.

See full details + MCP surface (55+ tools: `thought_tile_create`, `quick_trace`/`record_reasoning_trace`, `relate`, `search_by_relation`, `visualize`, `scar`, `verify_*`, goals, spatial, process:engram.*, etc. — full list in docs/MCP_TOOLS_REFERENCE.md) and Python client in the linked artifacts. **Use is more important than understanding upfront.**\n\n**Top-level discovery for agents**: See root `SKILLS.md` (index + links to `docs/skills/`, `docs/examples/sub_agent_governance.md`, `docs/examples/full_ritual_cycle.md`, `examples/hello-engram-agent.py`). Load the skills/ protocols and follow the full cycle demos.

The deepest demonstration of continuity remains the primary TUI ritual path. High-quality external use (yours) will reveal the deeper mathematical structure.

## 📁 Runnable Examples (Phase 2+)

See `examples/` (created/enhanced per GITHUB_MVP_PREP_PLAN.md) for immediately usable / runnable against **current build** (`target/debug/engram` or MCP):

- `examples/mcp_client.py` — Full session_start (loads sheaf), remember/recall/relate/visualize/verify_manifold + session_end (COMPRESS). Adapt from integrations/python/engram_client.py.
- `examples/ritual_verify.md` — Code Edit Ritual v1 + working-memory steps + scar/verify_block_lawfulness/trace examples (executable in TUI or via client).
- Spatial / geosphere demo (add `spatial_geosphere_demo.py` or equiv): `force_spatial_ingest`, `context_for_file` + `recall_in_file`, `set_geosphere_frame`, momentum queries (see plan + RITUALS.md).

**All examples dogfood engram** (traces recorded, relates to goal, spatial hygiene). Run in Phase 3 validation. See also docs/ for GEOMETRIC / RITUALS / MCP ref.

---

## 🛡️ Hallucination & Loop Protection

Traditional vector databases are append-logs: if an LLM hallucinates or loops, it spams the database with broken snippets, destroying context quality.

Engram uses a built-in **Lyapunov stability tracker** (the Coherence-Reliability Score, CRS) that monitors how much a concept drifts between updates:

- **Low Drift → CRS rises:** The system recognizes convergence and increases trust.
- **High Drift → CRS penalized:** Rapid contradictory overwrites are flagged as hallucination. Agents learn not to trust low-CRS blocks.

Memories must mathematically prove their stability. High-CRS blocks are automatically promoted to permanent `ZEDOS_PRAXIS` status during NREM consolidation. Low-CRS blocks decay and are swept by autophagy.

---

## 🧠 The Agentic Daemon

When Engram boots as an MCP server, it launches a **background daemon** that runs three autonomous loops:

### 1. File Watcher
Auto-ingests saved files via `inotify`/`fsevents` kernel hooks. Every time you save a `.rs`, `.py`, `.ts`, or any other supported file, the AST pipeline extracts new semantic blocks and updates the manifold without any agent intervention.

### 2. NREM Consolidation (Phase 3)
On a periodic cycle (~every 10 minutes), the daemon performs a **sleep-cycle memory consolidation pass**:
- Harvests all memories above CRS ≥ 0.74 (grounded fact tier)
- Superimposes them via `OP_ADD` into a unified **ego narrative tensor**
- Writes the result to `ego.leg3` — the agent's persistent self-model
- Mints a ZEDOS_EPISODIC block summarizing the consolidation

This is the equivalent of REM sleep for the agent's memory. Knowledge crystallized in one session is absorbed into the ego tensor and becomes available as prior context in all future sessions.

### 3. System Health Watchdog
The daemon continuously monitors critical background processes (e.g., the Circadian daemon that drives nightly consolidation). If a watched process dies, it automatically mints an **Agency Proposal** in the `agency_proposals.json` queue — a human-readable explanation of what failed and exactly what command it wants to run to fix it. The operator can approve or reject the proposal via the Cockpit UI or API.

> **Autophagy is disabled by default.** An agent's memory should outlive sessions. Use `mcp_engram_forget_old` to trigger manual GC when needed.

---

## 🌳 AST-Aware Semantic Distillation

Traditional RAG chunks text arbitrarily, destroying function boundaries. Engram's ingest pipeline uses a universal **AST-extraction layer** powered by **Tree-Sitter**, parsing **Rust, Python, TypeScript, JavaScript, Go, Java, C, and C++**.

It mints exactly **one memory block per public semantic item** (functions, structs, classes, traits):

- **The Tensor (`q`):** Encodes the doc comment and signature — what it is and what it does.
- **The Provlog:** Carries the raw, full-length source code — verbatim retrieval at any time.
- **Spatial Embodiment:** Maps the precise 2D row/column coordinates (AABB) of each AST node into the block's physical bounds. Agents know *where* code lives, not just *what* it does.

---

## 🧰 MCP Tools Reference (55+ Engram MCP tools as of 2026 — surface evolves; see docs/MCP_TOOLS_REFERENCE.md for categorized full list + examples)

**Mandatory for all MCP use (engram + grok_com_github etc.):** Call `search_tool` **first** (by tool name) to get the exact live input schema. Then `use_tool` with *only* the returned parameters. Never guess.

**Rule 6 (Expensive Tool Hygiene):** Once context is established, strictly prefer relational/spatial/goal tools (`search_by_relation`, `context_for_file` + `recall_in_file`, `goal_*`, `visualize`) over broad `query_with_momentum`. Use momentum only for explicitly "trending" questions when cheaper tools are insufficient.

Every significant decision/fork gets a `quick_trace` or `record_reasoning_trace`. Visible tool/MCP failures or dead-ends: `scar` **immediately**.

### Core Memory (4)

| Tool | Description |
|---|---|
| `remember` | Encode text and store as a persistent memory block |
| `recall` | Semantic similarity search — returns top-k. Optional `time_decay` for time-targeted search and `zedos_filter` for type filtering |
| `forget` | Delete a specific memory by concept name |
| `list_concepts` | List all stored concept names |

### Memory Management (9)

| Tool | Description |
|---|---|
| `mcp_engram_update` | Re-encode an existing memory in place with Lyapunov drift tracking — **use this, never forget+remember** |
| `mcp_engram_pin` | Lock a memory at CRS=1.0 — protects foundational axioms permanently |
| `mcp_engram_stats` | Manifold health report: total count, pinned, avg/min/max CRS, disk usage |
| `mcp_engram_recall_recent` | Return N most recently accessed memories, sorted by access time |
| `mcp_engram_summarize` | Project-state digest: pinned memories + top-N by CRS. Single-call wake-up replacement |
| `mcp_engram_forget_old` | On-demand autophagy: sweep out blocks below a CRS threshold |
| `mcp_engram_read_concept` | Fetch the full un-truncated text of a specific memory by exact concept name |
| `mcp_engram_export` | Serialize the entire manifold (or a CRS-filtered subset) to a portable JSON array |
| `mcp_engram_import` | Ingest a JSON array of `{concept, text}` objects into the manifold |

### Workspace & Agentic (8)

| Tool | Description |
|---|---|
| `mcp_engram_watch_workspace` | Bind a directory to the daemon's inotify watcher — auto-re-ingests saves via AST pipeline |
| `mcp_engram_context_for_file` | Surface top-5 relevant memories for a file path (proactive loading before editing) |
| `mcp_engram_recall_in_file` | Spatial code search: find all AST concepts defined within a specific line range |
| `mcp_engram_batch_remember` | Store multiple `{concept, text}` pairs in a single call — faster than N sequential `remember` calls |
| `mcp_engram_session_start` | **Mandatory at session start.** Validates manifold integrity and initializes epistemic state |
| `mcp_engram_session_end` | **Mandatory at session end.** Commits session summary + computes ADR thermodynamics |
| `mcp_engram_scar` | Create a geometric repeller (Apeiron binding) to mark a rejected approach as hostile — prevents re-hallucination |
| `mcp_engram_remember_solution` | Store a crystallized error→solution pair as a permanent `ZEDOS_PRAXIS` block. Auto-pinned at CRS=1.0 |

### Knowledge Graph (3)

Every `mcp_engram_relate` call stores a `ZEDOS_RELATION` block via `OP_BIND`. Edges are mathematical memory vectors — no external graph database required.

| Tool | Description |
|---|---|
| `mcp_engram_relate` | Bind two concepts via `OP_BIND` to create a directed knowledge graph edge |
| `mcp_engram_search_by_relation` | Traverse the graph by seed concept, edge direction, and optional label |
| `mcp_engram_visualize` | BFS from a seed concept → renders a Mermaid diagram of the subgraph |

### Physics & Alignment (5)

| Tool | Description |
|---|---|
| `mcp_engram_genesis` | Inspect or re-seed the foundational alignment genesis blocks (CRS=1.0, pinned, never decay) |
| `mcp_engram_verify_behavior` | Report empirical success/failure against a ZEDOS_HYPOTHESIS block. Repeated success promotes to PRAXIS |
| `mcp_engram_query_with_momentum` | Momentum-assisted recall (use sparingly per Rule 6 — prefer relational/spatial/goal tools once context exists). Blends semantic (80%) with p-tensor trajectory (20%). |
| `mcp_engram_set_namespace` | Switch to a project-specific memory namespace (stalk). Creates it if it doesn't exist |
| `mcp_engram_list_namespaces` | List all available namespaces and the currently active one |

### Autonomy & Orchestration (2)

These tools expose Engram's deeper integration with the Monad OS oracle layer, enabling agent self-reflection and multi-step workflow orchestration.

| Tool | Description |
|---|---|
| `mcp_self_trace` | Route a query through the Monad Oracle (Operator_LBR anchor) for deep logophysical self-reflection |
| `mcp_orchestrate_workflow_chain` | Chain multiple MCP tool calls into a single autonomous workflow execution |

---

## 🖥️ CLI Commands

Beyond the MCP server, Engram ships a standalone CLI for direct manifold management:

| Command | Description |
|---|---|
| `engram remember <concept> <text>` | Encode and store a memory |
| `engram recall <query>` | Semantic search, returns top-k |
| `engram forget <concept>` | Delete a memory |
| `engram list` | List all stored concept names |
| `engram ingest <path>` | Recursively ingest a directory (AST extraction for code + chunking for docs) |
| `engram trace <A> <OP> <B>` | VSA geometry: query the result of ADD or BIND on two concepts |
| `engram distill` | **Crystallize** — cluster episodic memories into durable ZEDOS_PRAXIS blocks |
| `engram build-index` | Build the LBVH O(log N) index for large manifolds (>10K blocks) |

---

## 🌐 Multi-Project Namespaces

Engram isolates memories by project via namespaced stalks. No config file required — just call:

```
mcp_engram_set_namespace("my_project")   # creates + switches to this namespace
mcp_engram_set_namespace("work_project") # switch to another project
mcp_engram_list_namespaces()             # see all namespaces
```

Or configure via `~/.engram/sheaf.toml`:

```toml
active_stalk = "codeland"

[[stalks]]
name = "codeland"
path = "~/.engram/stalks/codeland"

[[stalks]]
name = "personal"
path = "~/.engram/stalks/personal"
```

---

## ⚙️ Hardware Support

| Backend | Feature Flag | Status | Notes |
|---|---|---|---|
| CPU (Rayon O_DIRECT) | Default | ✅ | Exact linear scan. 10K memories → ~2.5 GB scanned in <0.4s via NVMe DMA bypass |
| CPU (LBVH index) | `bvh` | ✅ | O(log N) CSRP-projected tree. ~64 bytes RAM per concept. Build with `engram build-index` |
| CUDA (NVIDIA) | `cuda-kernels` | ✅ | GPU BVH O(log N), NVMe→VRAM parallel DMA via cuFile GDS |
| ROCm (AMD) | `rocm-kernels` | ✅ | Wavefront HIP execution |
| Metal (Apple) | `metal` | ✅ | MSL dynamic runtime compilation via metal-rs |
| WebGPU | `wgpu-backend` | ✅ | INT8 Poincaré hyperbolic search · 170× VRAM reduction · cross-platform |

---

## 💻 IDE Integration

Integration configs for all supported IDEs: [`integrations/`](integrations/)

### Google Antigravity IDE
```json
{
  "mcpServers": {
    "engram": {
      "command": "engram",
      "args": ["mcp", "--store", "~/.engram/stalks/"],
      "disabled": false
    }
  }
}
```

### Claude Desktop / Cursor / VS Code
```json
{
  "mcpServers": {
    "engram": {
      "command": "engram",
      "args": ["mcp", "--store", "~/.engram/stalks/"]
    }
  }
}
```

---

## 📄 License & Patent

This software is licensed under **AGPL-3.0-only**.

The `.LEG3` container format is covered by **U.S. Patent Application No. 19/372,256** (pending),  
*Self-Contained Variable File System (.LEG Container Format)*,  
Applicant: **Aric Goodman**, Oregon, USA — Static Rooster Media.

Commercial licenses (SaaS/cloud/enterprise) are available.  
Contact: **StaticRoosterMedia@gmail.com**

See [PATENT-NOTICE.md](PATENT-NOTICE.md) for full details.

---

## 🤝 Contributing & Current Build Hygiene

See [CONTRIBUTING.md](CONTRIBUTING.md) and [.github/PULL_REQUEST_TEMPLATE.md](.github/PULL_REQUEST_TEMPLATE.md) (full ritual/spatial/manifold/verify/build checklist).

**Always use current build during dev/prep:** `target/debug/engram` (or `cargo run -p engram-server`) — verified fresh via `cargo build` before edits (see GITHUB_MVP_PREP_PLAN.md execution log + Phase 0/3).

**Dogfooding (engram self-use):** "Dogfood" / "dogfooding" here means using Engram's own geometric tools and rituals *on the work itself* — e.g. `remember`/`relate`/`record_reasoning_trace`/`goal_*`/`scar`/`verify_*`/`spatial` calls + full wake/working-memory/session-end to track prep decisions, edits, and state as first-class manifold geometry. This makes meta-work (like this GitHub MVP prep) part of the living self-model for future agent continuity. See engram-working-memory discipline and AGENTS.md.

All changes follow engram-working-memory + Code Edit Ritual (pre context_for_file + recall + trace, post delta trace + relate to goal, engram dogfood records/scar/remember_solution).

See docs/ for GEOMETRIC_MEMORY.md, RITUALS.md, MCP_TOOLS_REFERENCE.md (public surface for the geometric non-flat + ritual system).

This README updated as part of Phase 2 MVP prep to better represent uniques vs popular flat memory repos.

## Why Engram (for agents & builders)

Engram is the reference implementation of “Against Flat Knowledge”: 256KB HolographicBlocks (.leg3) with q/p phase vectors, CRS, BLAKE3 provenance, VSA calculus (OP_ADD/OP_BIND/...), and a geometric sheaf over phase-space. Not another vector DB — a non-flat substrate where agent processes (rituals, sub-agents, monitors) are first-class sheaf sections declared in `processes/*.toml` (with category-theoretic object/morphism/sheaf_role/h1_handler + mcp_tools/requires/produces).

Live example (single source of truth):
```toml
# processes/ritual/wake-up.toml
[process]
name = "agent:engram.ritual.wake-up"
[category]
object = "session_start"
morphism = "OP_ADD"
sheaf_role = "Gluing axiom entry point..."
h1_handler = "OP_IS_SYMBOLIC_OF"
[mcp_tools]
list = ["mcp_engram_session_start", "mcp_engram_relate", ...]
```

Lean wake optimization (FAST session_start 0.04s, query_pure FAST_ANCHOR for ritual anchors, incremental spatial delta, process toml as executable sheaf with uses_mcp_tool relations): declarative processes/ritual/*.toml + dynamic loader in crates/engram-server/src/mcp.rs (hot preload at session_start, registers process:engram.* + live RELATION gluing via requires/produces/uses_mcp_tool) enable pure-geo query_pure (no file fallback) + incremental_spatial_ingest (mtime delta vs full force) for lean rehydrate while preserving ritual lawfulness, subvisor H¹, CRS gates. See hand-off in GITHUB_MVP_PREP_PLAN.md.

See:
- Manifesto + live processes/ (dynamic loader at session_start registers them with relations).
- AGENT_INTEGRATION_GUIDE.md (Processes as Sheaf Sections).
- .grok/skills/ + engram-working-memory for the discipline that makes it geometric.
- GITHUB_MVP_PREP_PLAN.md for execution (GPU patches, working-memory activation, loader, spatial, lean wake opt).

Commercial / on-prem licenses available. The geometry compounds continuity across agent instances.

(Added per GPU/Polish hand-off 2026-06; pre/post spatial + trace on README.)
