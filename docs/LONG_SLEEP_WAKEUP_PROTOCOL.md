# Long-Sleep Wakeup Protocol (Hardened)

**Status**: Draft v0.1
**Related Tasks**: `conv:task:harden_long_sleep_wakeup_protocol`
**Parent Arc**: `conv:arc:engram_self_verification_roadmap`

## Motivation

The standard wake-up protocol is excellent for normal day-to-day use. However, the deeper vision for this memory system requires credible support for **long-sleep / cold-boot scenarios**:

An agent (or robot) can be powered off for days, weeks, or months, then powered back on and still have a practical, locally-executable way to determine:

- "Is my memory still lawful and untampered?"
- "Can I trust my high-value Praxis and Genesis blocks enough to act on them?"
- "What level of caution should I operate under right now?"

This document defines the hardened protocol that makes this possible using Engram's tamper-evident primitives.

### What the Agent Actually Needs

For this part of the system to truly serve long-lived agents, it must provide:
- Fast, low-friction access to a trustworthy "lawfulness health" signal.
- Clear, actionable guidance rather than raw data dumps.
- Easy creation of durable records of what was discovered upon waking.
- Simplicity that remains usable even when the agent is in an uncertain state.
- Strong reinforcement of the agentic-first contract (the agent can verify its own history without heroic effort).

The protocol and any convenience tooling should be optimized around these needs.

## Core Principles

- **Local only** — No external servers or oracles are required for the trust decision.
- **Agent-executable** — The agent itself performs the checks via standard MCP tools.
- **Graduated trust** — Not binary "safe/unsafe", but clear signals that let the agent choose appropriate caution levels.
- **Built on existing substrate** — Leverages Merkle chains, `allowed_transforms` contracts, CRS, Logenergetics, and the new verification primitives.

## Recommended Hardened Wake-Up Sequence (Long Sleep / Cold Boot)

### Phase 0: Basic Connectivity & Session Binding (Same as Normal)
- Verify MCP connection
- `mcp_engram_session_start(intent="...")` — This remains mandatory. It creates the episodic record and triggers ki_hijacker.

### Phase 1: Broad Manifold Health Check (New / Strengthened)
```bash
mcp_engram_verify_manifold_integrity(
    min_crs = 0.74,
    sample_size = 100
)
```

**Interpretation guidelines** (current tool outputs):

**From `verify_manifold_integrity`**:
- `overall_health = "healthy"` and `issues_found = 0` → Strong signal to proceed in **Full Trust Mode**.
- 1–3 issues that are minor (e.g., old high-CRS drift on non-critical blocks) → **Cautious Mode** recommended. Increase use of `verify_behavior` before major actions.
- Any issue mentioning a `PRAXIS` block (especially contract violations) → Treat as high priority. Immediately run `verify_block_lawfulness` on the affected Praxis blocks.
- `issues_found ≥ 5` or multiple PRAXIS-related issues → Strongly consider **Degraded Mode**. Log findings and restrict high-stakes actions until reviewed.

**From `verify_block_lawfulness`** (on individual high-value blocks):
- Clean report (good CRS, expected `allowed_transforms`, reasonable recent drift) → The block can be trusted at face value.
- `allowed_transforms` narrower than expected (e.g., a former PRAXIS block now only allows `evidence_update`) → The block has been scarred. Treat with high caution.
- High `drift_velocity` (> 0.25–0.30) on a block that should be stable (Genesis or long-standing Praxis) → Flag for extra scrutiny or human review.
- Missing or suspicious `merkle_sub_root` on relation-linked concepts → Potential tampering or broken relationships. Escalate.

### Phase 2: Deep Audit of Critical Blocks (New)
For every block the agent considers high-stakes (Genesis blocks, key Praxis blocks, current operational contracts):

```bash
mcp_engram_verify_block_lawfulness(
    concept = "<critical_block>",
    check_merkle_chain = true
)
```

Pay special attention to:
- `allowed_transforms` violations or unexpected narrowing.
- High recent `drift_velocity` on blocks that should be stable.
- Missing or suspicious `merkle_sub_root` on relation-linked concepts.

### Phase 3: Relation & Mapping Integrity (Enhanced)
- Run the normal concept mapping reconstruction (`search_by_relation` + `visualize`).
- Additionally, spot-check relations involving high-CRS blocks for `merkle_sub_root` consistency where possible.

### Phase 4: Decision & Mode Selection
Based on the verification results, the agent should self-assess into one of several operating modes:

- **Full Trust Mode** — All critical checks passed cleanly. Normal high-agency operation.
- **Cautious Mode** — Minor issues or elevated drift on important blocks. Prefer `update` over broad actions, increase use of `verify_behavior`, escalate ambiguous decisions.
- **Degraded / Human-Escalation Mode** — Significant red flags (broken-looking contracts on Praxis, mass integrity issues, etc.). Restrict to read-only or very narrow actions until human review. Mint a visible `session_*_degraded_mode` block.

## Degraded Mode Guidance (Draft)

When verification results indicate **Degraded / Human-Escalation Mode**:
- Immediately create a visible high-priority episodic block (e.g. `session_start_*_degraded_mode`) documenting the specific red flags found.
- Restrict actions to read-only recall, narrow low-stakes operations, or explicit clarification requests to the human.
- Refuse (or heavily caveat) any operations that would create new high-CRS blocks or modify existing high-CRS/PRAXIS contracts until the issues are reviewed.
- Consider running deeper audits or `mcp_engram_search_by_relation` on affected concepts.

## Integration with Existing Tools

- This protocol is an extension of the standard wake-up, not a replacement for normal sessions.
- For everyday wake-ups, agents can continue using the lighter version in `wake_up.md`.
- After long sleep (or when the agent itself detects it has been offline for a long time), it should switch to (or add) the hardened sequence.

## Open Questions

- How should an agent detect "I have been offline for a long time" in a robust way?
- Should there be a dedicated convenience tool that runs a bundled "long-sleep verification suite"?
- How much of the verification logic should eventually move into a dedicated daemon background check vs. being explicitly called by the agent?

---

This document will be refined as we implement and test the protocol. All design decisions should be linked back to `conv:task:harden_long_sleep_wakeup_protocol`.