# Security Policy

## Supported Versions
Only the latest release (v0.4.x) is actively supported with security updates.

## Reporting a Vulnerability
Please do not open a public issue for security vulnerabilities. Instead, email StaticRoosterMedia@gmail.com directly. We will acknowledge receipt within 48 hours and provide a timeline for a patch.

## Geometric Manifold & Ritual Considerations
Engram is a **geometric (non-flat) memory substrate** (HolographicBlocks, VSA, sheaf gluing, spatial AABB, CRS/scar/verify lawfulness). Security extends beyond code vulns:

- **Manifold Integrity**: Use `mcp_engram_verify_manifold_integrity`, `verify_block_lawfulness`, `genesis status`, `spatial_status` as part of any security review or incident response. Low CRS or scar spikes can indicate hallucination/loop or tampering attempts.
- **Ritual Hygiene for Changes**: All substrate edits (crates/, skills/, mcp.rs, store.rs, processes/*.toml) **must** follow Code Edit Ritual (pre: watch_workspace + context_for_file + recall_in_file + record_reasoning_trace; post: delta trace + relate). See docs/RITUALS.md and .github/PULL_REQUEST_TEMPLATE.md.
- **Subvisor / Governance**: monitor.subvisor process (OP_INVERT + H¹) provides oversight for sub-agents and tool graphs to prevent doom loops/stagnation. Scar repetitive patterns immediately.
- **Spatial / Provenance**: AABB + Merkle + provlog in every block; force_spatial_ingest for bootstrap. Tamper-evident by design.
- **Continuation & Handoff**: session_end with COMPRESS + traces ensures lawful state transfer; no flat overwrites.

See [docs/GEOMETRIC_MEMORY.md](docs/GEOMETRIC_MEMORY.md), [docs/RITUALS.md](docs/RITUALS.md), [docs/GITHUB_MVP_PREP_PLAN.md](docs/GITHUB_MVP_PREP_PLAN.md) (prep includes these notes), and processes/ for declarative sheaf.

Vulns affecting invariants (.leg3, CRS gates, allowed transforms, unit hypersphere) or ritual anchors are high priority. Report with reproduction using current build (`target/debug/engram`).

Current build hygiene + engram records (traces/relates to goal:1780419540...) used throughout prep.
