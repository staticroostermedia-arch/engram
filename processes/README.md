# Process Sheaf — `processes/`

Declarative TOMLs that define Engram's **process sheaf**: rituals, harnesses, operators, monitors, linguistic calculus, and meta arcs. At `mcp_engram_session_start`, the loader walks registered subdirs and registers `[process]` blocks as `process:engram.*` keys with live `requires` / `produces` / `uses_mcp_tool` relations.

## Loader subdirs (sheaf-registered)

The loader in `crates/engram-server/src/mcp.rs` (`load_process_sheaf`) walks:

| Subdir | Role |
|--------|------|
| `ritual/` | Wake, session-end, NREM, code-edit anchors |
| `harness/` | CI / spatial recon harnesses |
| `operator/` | Momentum query, manifold ops |
| `monitor/` | Subvisor H¹ oversight, manifold health |
| `process/` | Session-end and cross-cutting process defs |
| `linguistic/` | Linguistic calculus + fibered equivalence |
| `meta/` | Meta arcs with `[process]` (e.g. marketplace prep) |

**Requirement:** Each sheaf TOML must have a `[process]` section with a **unique** `name` (e.g. `agent:engram.monitor.gemma-integration`). Names map to keys: `agent:engram.*` → `process:engram.*`.

## Workflow-only (not auto-loaded)

| Location | Purpose |
|----------|---------|
| `workflow/` | Human/agent orchestration loops (`[workflow]` only, no `[process]`) |
| `meta/*_loop.toml` | Recursive work-loop schemas (`[workflow]` without `[process]`) |

These files document **how to run** a multi-step arc (wake → execute → trace → handoff). They are **not** registered at session start. Pair them with a monitor subvisor TOML in `monitor/` when H¹ oversight is needed.

Example: `workflow/complete-gemma-integration.toml` + `monitor/gemma-integration.subvisor.toml`.

## Subvisor TOMLs

Files named `*.subvisor.toml` under `monitor/` are full sheaf processes (they include `[process]` + `[subvisor]`). Each must have a distinct `[process].name` — never rely on the subdir fallback (`agent:engram.monitor.unknown`), which collides across files.

## Environment

Set `ENGRAM_PROCESSES_DIR` to override the default (`./processes` relative to cwd).