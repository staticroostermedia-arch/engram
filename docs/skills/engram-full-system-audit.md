---
name: engram-full-system-audit
---

# Engram Full-System Audit Skill — Repeatable Subsystem Traversal Protocol

**For agents performing systematic Engram substrate audits.**

Declarative process: `processes/meta/full_system_audit_loop.toml`  
Sheaf harness: `agent:engram.harness.full-system-audit`  
Subvisor: `agent:engram.monitor.full-system-audit`

> **Contract:** [docs/AGENT_MEMORY_CONTRACT.md](../AGENT_MEMORY_CONTRACT.md) — lean 8-tool default; escalate to verify/relations for synthesis.

---

## When to Use

- Periodic substrate health review (quarterly or after major merges).
- Before large refactors — establish baseline findings tiles.
- After harness/MCP changes — validate ritual fidelity across ecosystems.
- Explicit user request: "full-system audit".

---

## Subsystem Manifest (9+ areas)

| # | Subsystem | Representative paths |
|---|-----------|---------------------|
| 1 | engram-core | `crates/engram-core/src/{types,ops,storage,backend,index}.rs` |
| 2 | engram-gpu | `crates/engram-gpu/src/*`, `kernels/*` |
| 3 | engram-server | `mcp.rs`, `store.rs`, `harness_injection.rs`, `session_*` |
| 4 | engram-ast + spatial | `engram-ast/src/lib.rs`, `daemon.rs`, `CODE_ATLAS_CONTINUITY.md` |
| 5 | process sheaf | `processes/**/*.toml`, `load_process_sheaf` in `mcp.rs` |
| 6 | linguistic | `linguistic/*.toml`, `mcp_linguistic_calculus` |
| 7 | integrations + plugin | `integrations/`, `grok-plugin-engram/` |
| 8 | scripts + harness | `scripts/`, `tools/test-harness/` |
| 9 | docs + contracts | `docs/AGENT_MEMORY_CONTRACT.md`, `RITUALS.md`, `skills/` |

---

## Ritual Loop (per phase)

```
1. mcp_engram_session_start(intent="full-system audit phase N: <subsystem>")
2. mcp_engram_ack_wake_queue(executed=true)   # hard gate
3. mcp_engram_recall(query="<subsystem> audit", scope="anchors")
4. mcp_engram_context_for_edit(path="/absolute/path/to/representative/file")
5. mcp_engram_quick_trace(decision=..., why=..., goal_context="goal:engram_mvp_v1",
     process_context="agent:engram.harness.full-system-audit", spatial_context="file.rs:line")
6. At phase boundary: mcp_engram_thought_tile_create(tile_type="research_offload", ...)
7. mcp_engram_session_end(summary=..., prepare_compression=true)  # at block end
```

**Read-only during recon.** Edits allowed only when codifying this process (toml/skill) or minting the autonomous plan tile.

---

## Phase Outputs

Each subsystem pass must produce:
- At least one `trace:*_audit` with `goal_context` + `process_context`
- One `tile:research_offload_*subsystem-audit*` with `human_forward` leading prose
- 3–5 categorized improvement opportunities with impact/feasibility

Synthesis produces:
- Linked `knowledge_graph` or `research_offload` capstone tile
- `verified_sequence` autonomous plan tile (schema: `docs/schemas/verified_sequence_v0.json`)
- `goal_decompose` children under `goal:engram_mvp_v1`

---

## Verification Gates

Before declaring audit complete:

1. `mcp_engram_recall(scope="anchors")` — 9+ distinct subsystem references
2. `mcp_engram_verify_manifold_integrity(min_crs=0.74, sample=50)`
3. `mcp_engram_process_metrics` on `agent:engram.harness.full-system-audit`
4. Git branch with ≥3 commits referencing `trace:*` or `tile:*` in messages
5. Explicit rollback exercised + correction trace

---

## Subvisor Rules

During recon phases the subvisor blocks:
- File writes outside `processes/` and `docs/skills/` (codification scope)
- Repeated grep on same path without new trace
- Sub-agents exceeding ~20 calls

Scar immediately on scope violations.

---

## Rehydration

Future agents: wake → `recall("full_system_audit OR subsystem audit")` → read `verified_sequence` plan tile → execute steps with JIT tool construction per `DEFORMATION_PLAYBOOKS.md`.