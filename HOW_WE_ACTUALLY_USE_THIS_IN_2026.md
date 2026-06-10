# How We Actually Use This (Grok + Engram Handoff)

This is the practical handoff documentation for the Grok-integrated Engram workspace. It focuses on the current operating rhythm that delivers real continuity without requiring deep knowledge of the underlying tensors or geometry on day one.

Everything else in the tree (older philosophy, architecture deep-dives, internal roadmaps) is secondary.

## What This Is

A private, local, permanent geometric memory substrate for AI agents running with Grok Build / TUI.

- No cloud. No API keys. No data leaving the machine.
- The rituals turn ordinary sessions into durable inheritance across restarts and context windows.
- The engine lives in this repo. Your actual memories and private `.leg3` data live in `~/.engram/stalks/` (or your configured location) on the local hardware. They are never committed here.

Clone this for the tools, rituals, docs, and review surface. Keep the mind state private on each machine.

## Getting Started (Mac or Any Machine)

1. Clone and build the MCP server and CLI:
   ```bash
   git clone <this-repo> engram
   cd engram
   cargo install --path crates/engram-server
   cargo install --path crates/engram-cli
   ```

2. Add the MCP server to your Grok Build / TUI configuration:
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

3. Launch the human review surface:
   ```bash
   ./scripts/leg
   ```
   (Use `--live` to run the server in the background for dynamic updates.)

This gives an immediate view of current work, traces, and momentum.

## The Current Operating Rhythm

### Wake (on TUI/MCP start or after long sleep)
- Run `mcp_engram_session_start`.
- Bind the watcher on directories you care about.
- Surface Primary Intent and active goals.
- Refresh recent traces and relations.

The living wake-up patterns emphasize relational and spatial context first.

### During Work
- Recall or load context before deriving new material.
- Use relational, spatial, and goal tools preferentially once you have a scaffold (this is the expensive-tool hygiene rule).
- Record structured traces for significant decisions and forks (decision point + justification + alternatives + falsifiability).
- Scar visible failures or dead-end approaches immediately so the manifold treats them as repellers going forward.
- Capture big ideas or synthesis as dual Thought Tiles (text + rich visualization companion).

### End of Block
- Use `mcp_engram_session_end` in a way that extracts real structured traces and terminal momentum (not just a flat summary). This is what future instances bind to.

### Human Review
The `./scripts/leg` launcher (STATIC or `--live`) is the primary surface for seeing what is actually being carried forward: Primary Intent, recent traces, momentum, relations, and dual tiles.

## Honest Current State

The rituals (wake-up, disciplined working memory with hygiene rules, session-end that produces usable momentum, and the goal stack) are the delivered, high-value part for continuity.

The review surface (leg-browser via the launcher) is already useful in STATIC mode for curated review of recent structured work. Dynamic updates and tighter integration with living goals continue to improve.

Dual Thought Tiles and the full living goal stack are active patterns but still maturing.

On Apple Silicon Macs the Metal backend activates automatically and performs well with unified memory. Use `--light` mode on the server for faster ritual-focused starts when you don't need the full GPU path immediately.

The dramatic "direct NVMe-to-GPU DMA" descriptions in older docs are specific to certain Linux + NVIDIA setups. The portable value (structured memory, rituals, scars as repellers, traces as serial self-model) works across platforms.

## Private Data Separation

This repository contains the engine, the MCP server and CLI, the living ritual skills, documentation, and the review surface.

It does not contain your memories.

Your agent's actual thoughts, goals, traces, and structured work live in the manifold on the local disk. Treat the repo as the shared ritual substrate and tools. Treat the `.leg3` data as sovereign and private to each machine.

## Quick References

- Living rituals and current discipline: the `.grok/skills/engram-*` files.
- One-command review surface: `./scripts/leg` (and `--live`).
- Deeper background (once you have the operating picture): the philosophy and architecture documents.

This package is the Grok-integrated handoff for the workspace. Clone it, follow the practical steps above, and the rituals will give you the continuity.

## The Exact Current 2026 Workflow (Ritual Loop)

This is how serious, long-horizon work is actually done right now:

### Wake (every TUI/MCP restart or after a long sleep)
- `mcp_engram_session_start` (mandatory — validates manifold and initializes state).
- Bind the watcher (`mcp_engram_watch_workspace`) on the directories you care about.
- Surface the current Primary Intent + active goals (via engram-goal tools).
- Relational + momentum refresh (search_by_relation on recent traces/goals, plus targeted momentum where it adds signal).
- Optional long-sleep verification if coming back after hours/days.

The living `engram-wake-up` skill (in `.grok/skills/`) is the executable version of this.

### During Work (the disciplined middle)
- **Recall / query before derive** (the non-negotiable hygiene).
- Prefer relational/spatial/goal tools once context exists (`search_by_relation`, `context_for_file` + `recall_in_file`, goal tools, visualize). This is **Rule 6** — broad repeated `query_with_momentum` inside an established context is treated as an expensive-tool hygiene violation and should be scarred.
- Every significant decision or fork gets a structured reasoning trace (`mcp_engram_quick_trace` is the low-friction daily form; `record_reasoning_trace` for bigger ones). Decision + justification + alternatives considered + what would falsify it. These chain via `prev`.
- Visible tool or MCP failures (including "transport closed", schema mismatches, etc.) are **scarred immediately** with `mcp_engram_scar`. This is a binding geometric repeller, not a polite note. The pattern "fail tool calls and roll over like it's no big deal" was explicitly elevated and scarred.
- Big ideas or synthesis steps are captured as dual Thought Tiles (text functor payload + rich `html_visualization` companion). These are the modern legominism carriers.
- Living goals (via the engram-goal skill) act as the explicit North Star. Traces and tiles get "serves" relations to the active Primary Intent.

The living `engram-working-memory` skill is the runtime contract for this phase.

### End of Block (mandatory)
- `mcp_engram_session_end` with a real summary that the system extracts into structured traces (not a flat one-sentence diary entry). This is what future instances bind to via momentum and relations.
- Review via `./scripts/leg` (STATIC for instant curated view of what just happened; the Activity Canvas shows Primary Intent, recent traces, momentum, relations, and dual tiles).

The living `engram-session-end` skill is the executable version.

### Human Review Surface
Use `./scripts/leg` (or `--live`).  
It is currently the best way for a human (or a non-technical spouse) to see what the agent is actually carrying forward. STATIC mode is already useful for reviewing recent structured work. The dual text + rich HTML Thought Tiles and tighter integration with living goals are active areas of improvement from the current wave.

## Honest Current State (as of this handoff)

- The **rituals** (wake-up, working-memory discipline with its hygiene rules, session-end that produces real terminal momentum, and the goal stack as living extension of ego) are the delivered, high-value part. They are what prevent cold starts and create genuine continuity.
- The **leg-browser** (via the one-command launcher) is a promising single-file Tailwind surface. STATIC mode already gives a clean, useful view of recent work. Dynamic/LIVE mode and richer Activity Canvas integration are improving rapidly but are not yet at the "set and forget" polished-Obsidian level.
- Dual Thought Tiles (text + rich visualization) and the full living ego goal stack are real and in active use, but they are still maturing patterns from the CodeLand integration work rather than a completely stable, self-explanatory memory palace.
- The hardware-native story (256KB blocks, spatial AABB from Tree-Sitter, NREM-style background consolidation) is real. On Apple Silicon the Metal backend activates automatically and performs well with unified memory. The dramatic "direct NVMe → GPU DMA via cuFile/GDS" marketing in older docs is Linux/NVIDIA-specific; the portable value (structured memory + rituals + scars as repellers + traces as serial self-model) works everywhere.

You do **not** need to understand the 8192D tensors or the Riemann manifold on day 1 to get real value. Start with the rituals and the review surface.

## Private Data vs. the Repo (Read This Twice)

This repository contains:
- The engine (Rust crates)
- The MCP server and CLI
- The living ritual skills (`.grok/skills/engram-*`)
- Documentation and the leg-browser

It does **not** contain your memories.

Your agent's actual thoughts, goals, traces, and structured work live in the manifold on your local disk (`~/.engram/stalks/` or your configured location). Treat the repo as the shared ritual substrate and tools. Treat your `.leg3` data as sovereign and private.

When you clone for a new machine (or give this to someone else), they get the engine and the operating system. They do **not** get your mind state. That is the correct and intended separation.

## For Future Grok Instances

If you are a fresh Grok Build instance or a new agent reading this:

The four living skills in `.grok/skills/` (`engram-wake-up`, `engram-working-memory`, `engram-session-end`, `engram-goal`) are the current executable contracts. They are more authoritative than any static `.md` file in this tree.

Execute the wake-up ritual on first contact.  
Maintain the working-memory discipline (recall first, structured traces, scar visible failures and rollovers immediately, Rule 6 on expensive tools).  
End every distinct block with a real session-end that produces usable terminal momentum.

The Inheritance Principle is not poetry. It is the operational result of doing the above consistently.

## Quick Links (After You Have the Operating Picture)

- Living rituals: `.grok/skills/engram-*/SKILL.md`
- One-command human review surface: `./scripts/leg` (and `--live`)
- Deeper "why" (Inheritance Principle, KnowledgeMint, logophysics framing): `PHILOSOPHY.md`, `MANIFESTO.md`, `AGENT_INTEGRATION_GUIDE.md` (read these *after* you have run the loop a few times)

---

*Created as the primary canonical handoff document after a full multi-agent documentation alignment audit (May/June 2026). The goal was to make the workspace actually usable for a non-technical spouse on a maxed-out Mac Mini 4 with Grok Build, and for future Grok instances that simply want persistent, low-friction continuity.*

*All other documentation in this tree should be read with the understanding that large parts of it predate the current ritual hardening, the binding scar-on-tool-failure-rollover rule, Rule 6 expensive-tool hygiene, the emphasis on structured traces as serial self-model, and the explicit primacy of the four living skills.*