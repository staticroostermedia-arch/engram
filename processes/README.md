# Process Sheaf — `processes/`

Declarative TOMLs that define Engram's **process sheaf**: rituals, harnesses, operators, monitors, linguistic calculus. At `mcp_engram_session_start`, the loader walks registered subdirs and registers `[process]` blocks as `process:engram.*` keys with live `requires` / `produces` / `uses_mcp_tool` relations.

## Loader subdirs (sheaf-registered)

The loader in `crates/engram-server/src/mcp.rs` (`load_process_sheaf`) walks:

| Subdir | Role |
|--------|------|
| `ritual/` | Wake, session-end, NREM, code-edit anchors |
| `harness/` | CI / spatial recon / sub-agent launch & relay |
| `operator/` | Momentum query, manifold ops |
| `monitor/` | Subvisor H¹ oversight, manifold health |
| `process/` | Session-end and cross-cutting process defs |
| `linguistic/` | Linguistic calculus + fibered equivalence |

**Requirement:** Each sheaf TOML must have a `[process]` section with a **unique** `name` (e.g. `agent:engram.monitor.sub-agent`). Names map to keys: `agent:engram.*` → `process:engram.*`.

## Workflow-only (not auto-loaded)

| Location | Purpose |
|----------|---------|
| `workflow/` | Human/agent orchestration loops (`[workflow]` only, no `[process]`) |

These files document **how to run** a multi-step arc (wake → execute → trace → handoff). They are **not** registered at session start. Pair them with a monitor subvisor TOML in `monitor/` when H¹ oversight is needed.

### Sub-agent trio (WS-5)

| Role | File | Registered key |
|------|------|----------------|
| **Launch** (orchestrator) | `harness/sub-agent-launch.toml` | `process:engram.harness.sub-agent-launch` |
| **Relay** (sub-agent contract) | `harness/sub-agent-relay.toml` | `process:engram.harness.sub-agent-relay` |
| **Relay steps** (sub-agent playbook) | `workflow/sub_agent_relay_v1.toml` | workflow-only |
| **Monitor** (H¹ while running) | `monitor/sub-agent.subvisor.toml` | `process:engram.monitor.sub-agent` |

See [docs/examples/sub_agent_governance.md](../docs/examples/sub_agent_governance.md), [docs/HARNESS_INJECTION.md](../docs/HARNESS_INJECTION.md), and [docs/SUBSTRATE_WINS_PLAN.md](../docs/SUBSTRATE_WINS_PLAN.md) WS-5 (historical).

## Meta workflows (not sheaf-loaded)

| Location | Purpose |
|----------|---------|
| `meta/` | Advanced operator playbooks for `/loop` scheduling (consciousness loop, self-improvement, NREM consolidation). **Not** scanned by `load_process_sheaf` — human/agent orchestration specs with `[workflow]` sections. See `grok-plugin-engram/commands/engram-loop.md`. |

## Subvisor TOMLs

Files named `*.subvisor.toml` under `monitor/` are full sheaf processes (they include `[process]` + `[subvisor]`). Each must have a distinct `[process].name` — never rely on the subdir fallback (`agent:engram.monitor.unknown`), which collides across files.

## Environment

Set `ENGRAM_PROCESSES_DIR` to override the default (`./processes` relative to cwd).