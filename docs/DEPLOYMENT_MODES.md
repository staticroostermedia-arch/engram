# Engram Deployment Modes

Engram's memory layer is model-agnostic and IDE-agnostic. The same geometric manifold powers every deployment. Pick the mode that fits your workflow.

**Current Disciplined MCP & Tool Usage (2026 reality — this supersedes older tool lists):**
- Full surface is 50+ Engram MCP tools (goal family, quick_trace, record_reasoning_trace, scar, spatial, verification, etc.).
- **Universal rule:** For *every* MCP server (engram, grok_com_github, etc.): call `search_tool` first to get the live schema, *then* `use_tool`. Never guess parameters.
- **Rule 6 (Expensive Tool Hygiene):** Once context is established, strictly prefer relational/spatial/goal tools (`search_by_relation`, `context_for_file` + `recall_in_file`, `goal_*`, `visualize`) over broad `query_with_momentum`.
- Every significant decision/fork: record via `quick_trace` or `record_reasoning_trace`.
- Visible tool/MCP failures or dead-end approaches: `scar` **immediately** (binding geometric repeller).

---

## Mode 1 — IDE + Cloud Agent
**Simplest setup. Internet required.**

Your AI IDE (Antigravity, Claude Desktop, Cursor, Zed) connects to a cloud reasoning model (Gemini, Claude, GPT-4). Engram runs as an MCP server alongside it and gives the cloud agent persistent geometric memory.

```
User → IDE → Cloud LLM (Gemini / Claude / GPT-4)
                    ↓ MCP tools
               Engram (local memory, 50+ MCP tools — surface evolves; always `search_tool` first for exact current schema)
```

**Setup:** See [`integrations/README.md`](../integrations/README.md). Minimal config:

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

The cloud agent gains access to the full current Engram MCP surface (50+ tools as of 2026, including the complete `goal_*` family, `quick_trace`/`record_reasoning_trace`, `scar`, spatial tools, verification, etc.). 

**Mandatory MCP Calling Discipline (all servers, including engram and grok_com_github):** Always call `search_tool` (with the tool name) **first** to retrieve the exact input schema. Then call `use_tool` using *only* the returned parameter names. NEVER guess or hardcode parameters. This is non-negotiable.

---

## Mode 2 — IDE + Cloud Agent + Local Agent (Hybrid)
**Cloud and local agents share the same manifold simultaneously.**

The cloud agent uses Engram via MCP tools. A local LLM (`monad_nemo` Rust orchestrator) uses Engram via the REST API. Both read and write to the same geometric manifold. Memories from one agent are immediately available to the other.

```
IDE → Cloud LLM → Engram MCP (port: stdio)
                       ↕
         ~/.engram/stalks/   [shared geometric manifold]
                       ↕
  monad_nemo → Local LLM → Engram REST (port: 3456)
```

**Stalk isolation** (recommended):
```toml
# ~/.engram/sheaf.toml
active_stalk = "local-agent"

[[stalks]]
name = "ide-agent"
path = "~/.engram/stalks/ide-agent"

[[stalks]]
name = "local-agent"
path = "~/.engram/stalks/local-agent"
```

Each agent writes to its own stalk. Both stalk recall results are merged and ranked on every query.

---

## Mode 3 — IDE + Local Agent, Fully Offline
**Zero cloud. Zero internet after install. Zero bytes leave the machine.**

The IDE runs locally (Antigravity is an Electron app — it doesn't require cloud). The reasoning model runs locally via `llama-server` or Ollama. Engram provides memory via MCP. Nothing leaves your hardware.

```
User → IDE (local Electron app)
             ↓ points to local LLM endpoint
        Local LLM (Gemma 4 / Llama 3 / any GGUF)
             ↓ MCP tools
        Engram (local memory)
```

**Setup:**
```bash
# 1. Serve your local model
llama-server \
  --model gemma4-26B-A4B-IQ4_XS.gguf \
  --port 11434 \
  --ctx-size 40960

# 2. Start Engram memory
engram mcp

# 3. Point your IDE at the local endpoint
#    Antigravity: Settings → Model → Custom Endpoint → http://localhost:11434
```

**This mode is essential for:**
- 🏢 **Enterprise** — code never leaves the internal network
- 🏥 **Medical / Legal** — data residency and HIPAA/GDPR compliance
- 🔒 **Air-gapped systems** — no internet connection available
- 🛡️ **Privacy-first developers** — proprietary code stays local

**Compatible local models** (any OpenAI-compatible endpoint works):

| Model | Parameters | Notes |
|---|---|---|
| Gemma 4 | 26B / 4B active (MoE) | Native function calling |
| Llama 3.1 | 8B / 70B | Strong tool use |
| Mistral / Mixtral | 7B / 8x7B | Fast on consumer hardware |
| Qwen 2.5 Coder | 7B / 32B | Optimized for code |
| Phi-4 | 14B | Strong reasoning at small scale |
| DeepSeek Coder | 6.7B / 33B | Code-specialized |

Any model served via `llama.cpp` or Ollama with tool/function calling support works identically. Swap the `.gguf` file with no code changes.

---

## Mode 4 — Standalone Local Agent (No IDE, Headless)
**Terminal only. No IDE. No cloud. Fully scriptable.**

`monad_nemo` is a Rust orchestrator that manages the full agentic loop: recall from Engram, inject context into the LLM, parse tool calls, execute them, write new memories back. No IDE required.

```
Terminal
   ↓
nemo_agency (Rust orchestrator)
   ↓         ↓
Engram     Local LLM (llama-server : 11434)
REST API   (Gemma 4 / any OpenAI-compatible model)
```

**Setup:**
```bash
# 1. Start Engram REST server
engram serve --port 3456

# 2. Start local LLM
llama-server --model gemma4-26B-IQ4_XS.gguf --port 11434

# 3. Start the Rust orchestrator
nemo-agency
```

**Use cases:**
- Background autonomous research agents that run while you sleep
- CI/CD pipeline agents with persistent memory across runs
- Scheduled knowledge acquisition loops (cron + nemo-agency)
- Any environment where launching an IDE is impractical

---

## Manifold Compatibility

All four modes read and write the same `.leg` block format. A memory written by a cloud agent in Mode 1 is immediately available to the local agent in Mode 4. ZEDOS tags, CRS scores, Sheaf stalks — everything is preserved across modes.

```
Mode 1 cloud agent remembers something
         ↓ writes to ~/.engram/stalks/ide-agent/
Mode 4 local agent recalls same manifold
         ↑ reads from all stalks via Sheaf fan-out
```

## Model Swapping

The local agent modes (3 and 4) require only one thing from the LLM: an OpenAI-compatible `/v1/chat/completions` endpoint that supports `tools`. To swap models:

```bash
# Stop llama-server, start with a different model
llama-server --model llama3.1-70B-Q4_K_M.gguf --port 11434

# No configuration changes needed anywhere else
```

The `monad_nemo` orchestrator, Engram memory, and the full current MCP surface work identically regardless of which model is behind the endpoint. (Tool surface evolves; always use `search_tool` first for the live schema.)
