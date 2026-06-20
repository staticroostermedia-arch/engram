# Engram

[![Build Status](https://github.com/staticroostermedia-arch/engram/actions/workflows/rust.yml/badge.svg)](https://github.com/staticroostermedia-arch/engram/actions)
[![MCP](https://img.shields.io/badge/MCP-Native-blue)](https://github.com/modelcontextprotocol)
[![Glama](https://glama.ai/mcp/servers/staticroostermedia-arch/engram/badge)](https://glama.ai/mcp/servers/staticroostermedia-arch/engram)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-purple)](LICENSE)
[![Patent Pending](https://img.shields.io/badge/Patent-Pending-orange)](PATENT-NOTICE.md)
[![Geometric Memory](https://img.shields.io/badge/Geometric-Non--flat%20sheaf%20%2B%20rituals-8A2BE2)](docs/GEOMETRIC_MEMORY.md)

**Persistent geometric memory for AI agents.**

Engram is a local, hardware-native memory substrate that gives AI agents coherent, long-term memory with structure-preserving compression, synthetic calculus over both words and numbers, and true continuity across cold shutdowns.

> **Share on X / GitHub:** [docs/images/engram-share-x.png](docs/images/engram-share-x.png) (1280×720). For the repo social preview card: GitHub → **Settings → General → Social preview** → upload that image.

Unlike vector databases or simple logs, Engram uses fixed-size holographic blocks, VSA operations, sheaf gluing, and categorical reasoning to maintain meaning and relationships even after heavy compression and long-running sessions.

It is designed as a drop-in backend for any LLM (Grok, Claude, Llama, etc.) via the Model Context Protocol (MCP) and is fully open for anyone to build on.

### New here?

| You are… | Do this |
|----------|---------|
| **A human** (“should my AI use this?”) | Skim [Quick start](#quick-start) below. If it looks right, tell your agent: *“Clone this repo, follow [FIRST_RUN.md](FIRST_RUN.md) steps 1–4, add the MCP config from FIRST_RUN §2, then use the [8-tool contract](docs/AGENT_MEMORY_CONTRACT.md).”* Optional: `./scripts/leg --live` to review what the agent remembers. |
| **An AI agent** (you were pointed here) | Human must finish [FIRST_RUN.md](FIRST_RUN.md) §1–2 (build + MCP) so you have `mcp_engram_*` tools. Then read [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md) and run the copy-paste loop at the bottom. **First call every session:** `mcp_engram_session_start(intent="…")`. |
| **Curious about the theory** | [docs/GEOMETRIC_MEMORY.md](docs/GEOMETRIC_MEMORY.md) · [MANIFESTO.md](MANIFESTO.md) — after you have a working install. |

**Rituals** = documented MCP habits (wake → trace decisions → handoff) so memory compounds across sessions — not mysticism, just the discipline that beats flat RAG.

Engram is particularly well-suited for:
- Long-running agentic systems
- Games with persistent LLM characters
- Personalized AI companions
- Any application needing coherent, evolving memory beyond simple vector stores

| Start here | Doc |
|------------|-----|
| **New users & agents** | [FIRST_RUN.md](FIRST_RUN.md) |
| **Lean contract (8 tools)** | [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md) |
| **Grok Build / xAI reviewers** | [docs/GROK_BUILD_MEMORY.md](docs/GROK_BUILD_MEMORY.md) |
| **MCP setup (all ecosystems)** | [integrations/README.md](integrations/README.md) |
| **Human review (LEG Browser)** | [docs/LEG_BROWSER.md](docs/LEG_BROWSER.md) |
| **Situated edit memory (code atlas)** | [docs/CODE_ATLAS_CONTINUITY.md](docs/CODE_ATLAS_CONTINUITY.md) |
| **Personal knowledge wiki** | [docs/PERSONAL_KNOWLEDGE_WIKI.md](docs/PERSONAL_KNOWLEDGE_WIKI.md) |
| **Power users (79 tools)** | [docs/TOOL_DECISION_MAP.md](docs/TOOL_DECISION_MAP.md) |
| **Ritual skills** | [SKILLS.md](SKILLS.md) → [docs/skills/](docs/skills/) |
| **Deep mode (after install)** | [AGENT_INTEGRATION_GUIDE.md](AGENT_INTEGRATION_GUIDE.md) |

**Human review (LEG Browser beta):** `./scripts/leg` (static) or `./scripts/leg --live` — see [docs/LEG_BROWSER.md](docs/LEG_BROWSER.md).

---

## Why not flat RAG?

| | Flat vector / markdown | Engram |
|--|------------------------|--------|
| Storage | append-log / chunks | Structured blocks with integrity checks (details: [GEOMETRIC_MEMORY](docs/GEOMETRIC_MEMORY.md)) |
| Wake | cold start every time | `session_start` restores goals, last session, suggested next steps |
| Integrity | none | `verify_*`, scars, lawfulness gates (CRS ≥ 0.74) |
| Code context | RAG chunks | `context_for_edit` — file-scoped memory before you edit |
| Agent discipline | hope the model remembers | Documented rituals + optional governance processes |
| Human mirror | none | LEG Browser beta — see traces, goals, tiles locally |

Full comparison vs mem0/Letta/chroma: see [docs/GROK_BUILD_MEMORY.md](docs/GROK_BUILD_MEMORY.md).

---

## Quick start

```bash
git clone https://github.com/staticroostermedia-arch/engram.git
cd engram
cargo build -p engram-server
target/debug/engram --version   # 0.7.0-beta.2
```

**MCP config** (Grok Build / Cursor — use `scripts/engram-grok`):

```json
{
  "mcpServers": {
    "engram": {
      "command": "/path/to/engram/scripts/engram-grok",
      "args": ["mcp"],
      "env": {
        "ENGRAM_STORE": "~/.engram/stalks/",
        "ENGRAM_PROFILE": "agent"
      }
    }
  }
}
```

Restart your IDE, then:

```
mcp_engram_session_start(intent="your goal")
```

**Lean loop:** `session_start` → `context_for_edit(path)` → `recall(scope=anchors)` → `quick_trace` / `remember` → `session_end(summary)`.

All ecosystems: [integrations/README.md](integrations/README.md). Cursor ambient wake: `./scripts/cursor-engram-preflight.sh`.

---

## LEG Browser (beta)

Local, read-only mirror of agent memory — no cloud, no npm, no account. Your manifold stays in `~/.engram/`; the repo ships tools and the viewer.

```bash
./scripts/leg              # static — instant curated demo, no backend
./scripts/leg --live       # live — engram serve :3456 + viewer :8765
```

**What you get (beta):**

- Wake queue + continuity playbook (same harness agents see at `session_start`)
- Code atlas + evolution timeline at file loci (`__arc` segments, trace chain)
- Presentation stratum (~40–64 distilled nodes, not the full cold manifold)
- Activity feed, traces, goals, thought tiles, relations, geosphere view
- Hygiene controls (demote sprawl, condensation hints, wake/edit-arc debt)

**Beta caveats:** single-file SPA; galaxy view may be slow on 100k+ stores; agent MCP paths stay bounded. Hard-refresh after `index.html` updates. Static mode is a demo snapshot — `--live` shows real MCP work.

Full guide: [docs/LEG_BROWSER.md](docs/LEG_BROWSER.md). Safe serve restart (does not kill MCP): `./scripts/restart-leg-serve.sh`.

![LEG Browser beta — live manifold mirror](./docs/images/leg-browser-beta-live.png)

---

## Memory model (one paragraph)

Fixed **256KB HolographicBlocks** (.leg3): 8192D phase (q), momentum (p), CRS lawfulness, BLAKE3 Merkle, spatial AABB. **VSA calculus** + **sheaf gluing** via `processes/*.toml` (rituals, harness, monitor). **NREM / ego.leg3** for long-horizon continuity. Details: [docs/GEOMETRIC_MEMORY.md](docs/GEOMETRIC_MEMORY.md), [docs/RITUALS.md](docs/RITUALS.md), [docs/HARNESS_INJECTION.md](docs/HARNESS_INJECTION.md).

**Linguistic calculus** (words + numbers in the same sheaf): [docs/CATEGORICAL_LINGUISTIC_CALCULUS.md](docs/CATEGORICAL_LINGUISTIC_CALCULUS.md).

```mermaid
flowchart LR
  W[session_start<br/>harness injection] --> E[edit + trace]
  E --> H[session_end handoff]
  H --> W
```

## What's new ([Unreleased] — see CHANGELOG)

- **Code atlas continuity v2:** situated edit memory at the locus — atlas v2.1, `evolution_at_locus`, hard wake gate, `post_edit_palette`, update coherence. [CODE_ATLAS_CONTINUITY.md](docs/CODE_ATLAS_CONTINUITY.md)
- **Large-store perf:** bounded NREM + relation batching — wake and evolution recon in seconds on ~192k blocks.
- **79 MCP tools** registered; lean default remains **8 essential**.
- **LEG evolution panel:** `./scripts/leg --live` + `GET /api/code-atlas?evolution=1`.

Prior release **v0.7.0-beta.2:** cold-start onboarding, JIT deformation, LEG Browser beta. Full history: [CHANGELOG.md](CHANGELOG.md).

## Categorical Linguistic Calculus

Engram supports native **synthetic calculus over linguistic structures** — including mixed number + word operations — all inside the geometric memory manifold.

Key capabilities:
- Structure-preserving compression and decompression of language while preserving homotopy coherence (meaning up to coherent deformation).
- Synthetic operations: differentiate, integrate, and operadic composition on word bundles.
- Mixed number + word reasoning with clearly defined bridging morphisms and class-mixing guards.
- Full persistence via NREM consolidation and ego.leg3 self-modeling.

### Quick Example
```rust
// Build a linguistic bundle + mixed expression
let bundle = LinguisticDiscourseBundle { ... };
let mixed = op_mixed_linguistic_number_scale(&num_phase, &word);

// Run calculus and store result
let delta = op_linguistic_differentiate(&bundle);
let result = op_linguistic_integrate(&[bundle, delta]);

// Store with full continuity
let _ = Leg3Pointer::mint_linguistic(&result, true); // promotes toward ego.leg3
```

All operations return CRS (Coherence-Reliability Score) and can be verified with `mcp_engram_verify_manifold_integrity`.

---

## Examples

| File | What it does |
|------|----------------|
| [examples/hello-engram-agent.py](examples/hello-engram-agent.py) | Minimal MCP loop |
| [examples/mcp_client.py](examples/mcp_client.py) | Session + recall + relate + verify |
| [examples/ritual_verify.md](examples/ritual_verify.md) | Code Edit Ritual walkthrough |
| [docs/examples/marketplace_demo.md](docs/examples/marketplace_demo.md) | Grok plugin demo |

Build against `target/debug/engram` during development.

---

## MCP tools

**8 essential** for daily work — **79 registered** (75 `mcp_engram_*` + 4 linguistic); full map: [docs/TOOL_DECISION_MAP.md](docs/TOOL_DECISION_MAP.md). Categorized reference: [docs/MCP_TOOLS_REFERENCE.md](docs/MCP_TOOLS_REFERENCE.md). Harness matrix: `tools/test-harness/python/mcp_tool_matrix.py`.

Grok plugin slash commands: [grok-plugin-engram/commands/](grok-plugin-engram/commands/).

---

## Deep dive (linked, not repeated here)

### Users

| Topic | Doc |
|-------|-----|
| LEG Browser (beta) | [docs/LEG_BROWSER.md](docs/LEG_BROWSER.md) |
| Personal knowledge wiki | [docs/PERSONAL_KNOWLEDGE_WIKI.md](docs/PERSONAL_KNOWLEDGE_WIKI.md) |
| Deployment & hardware backends | [docs/DEPLOYMENT_MODES.md](docs/DEPLOYMENT_MODES.md) · [docs/architecture.md](docs/architecture.md) |
| Marketplace submission | [docs/MARKETPLACE_SUBMISSION.md](docs/MARKETPLACE_SUBMISSION.md) |

### Agents

| Topic | Doc |
|-------|-----|
| JIT deformation / RSI | [docs/DEFORMATION_PLAYBOOKS.md](docs/DEFORMATION_PLAYBOOKS.md) |
| Harness injection at wake | [docs/HARNESS_INJECTION.md](docs/HARNESS_INJECTION.md) |
| Ritual overview | [docs/RITUALS.md](docs/RITUALS.md) |
| MCP tools reference (79) | [docs/MCP_TOOLS_REFERENCE.md](docs/MCP_TOOLS_REFERENCE.md) |
| Long-sleep return | [docs/LONG_SLEEP_WAKEUP_PROTOCOL.md](docs/LONG_SLEEP_WAKEUP_PROTOCOL.md) |

### Contributors

| Topic | Doc |
|-------|-----|
| Maintainer workflow | [docs/internal/MAINTAINER_WORKFLOW.md](docs/internal/MAINTAINER_WORKFLOW.md) |
| Harness program (shipped) | [docs/SUBSTRATE_WINS_PLAN.md](docs/SUBSTRATE_WINS_PLAN.md) · [docs/HARNESS_INJECTION.md](docs/HARNESS_INJECTION.md) |
| Process sheaf + sub-agent governance | [processes/README.md](processes/README.md) |
| Contributing | [CONTRIBUTING.md](CONTRIBUTING.md) · [AGENTS.md](AGENTS.md) |

### Theory

| Topic | Doc |
|-------|-----|
| CRS / scars / lawfulness | [docs/GEOMETRIC_MEMORY.md](docs/GEOMETRIC_MEMORY.md) |
| Categorical linguistic calculus | [docs/CATEGORICAL_LINGUISTIC_CALCULUS.md](docs/CATEGORICAL_LINGUISTIC_CALCULUS.md) |
| Philosophy | [MANIFESTO.md](MANIFESTO.md) · [PHILOSOPHY.md](PHILOSOPHY.md) |

**CLI:** `engram remember|recall|forget|list|ingest|trace|distill|build-index`

**Namespaces:** `mcp_engram_set_namespace("project")` or `~/.engram/sheaf.toml`

---

## Contributing

[CONTRIBUTING.md](CONTRIBUTING.md) · [AGENTS.md](AGENTS.md) · PR checklist in [.github/PULL_REQUEST_TEMPLATE.md](.github/PULL_REQUEST_TEMPLATE.md)

Dev build: `cargo build -p engram-server && target/debug/engram --version`

---

## License

**AGPL-3.0-only**. `.leg3` format: U.S. Patent Application No. 19/372,256 (pending). Commercial licenses: StaticRoosterMedia@gmail.com — [PATENT-NOTICE.md](PATENT-NOTICE.md).