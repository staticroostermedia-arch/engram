# long_sleep_verification_suite_design

**Related Task**: `conv:task:harden_long_sleep_wakeup_protocol`
**Status**: Early Design Draft
**Created**: 2026-05-25 (during autonomous execution)

## Purpose

Design a convenience MCP tool (or small set of tools) that packages the hardened long-sleep verification flow into something an agent can invoke with a single call or very few calls, rather than having to manually orchestrate multiple verification steps.

This would make the "long-sleep / cold-boot" path much more practical and less error-prone for agents.

## Proposed Tool Name (Tentative)

`mcp_engram_long_sleep_verification`

(Alternative: `mcp_engram_verify_lawfulness_after_downtime`)

## High-Level Behavior

When called, the tool would execute the following logical flow (this can be implemented as orchestration in the handler or as a dedicated helper in StoreHandle):

1. Determine effective thresholds based on `downtime_days` and `paranoia_level`.
2. Run `verify_manifold_integrity` with appropriate `min_crs` and sample size.
3. Automatically select critical blocks to deeply audit:
   - All Genesis blocks (exact name lookup for reliability)
   - Top N Praxis blocks by CRS or recent access
   - Any blocks flagged in the broad integrity check
4. For each critical block, call `verify_block_lawfulness` with `check_merkle_chain=true`.
5. Optionally run lightweight relation checks on high-value relations involving the audited blocks.
6. Synthesize everything into a structured `LawfulnessHealthReport` containing:
   - `overall_recommendation`
   - `summary_text` (human + agent readable)
   - `critical_findings` (array)
   - `blocks_audited`
   - `recommended_actions`
7. If `auto_create_audit_block` is true, mint a `session_start_*_long_sleep_audit` block with the report summary.

## Open Design Questions

- Should this be one tool that does everything, or a "suite" that returns structured data an agent can reason over?
- How aggressive should the default critical block selection be?
- Should the tool have parameters for "how paranoid" the check should be?
- How much of this can/should be done in the daemon vs on-demand via MCP?
- Should it automatically create a `session_start_*_long_sleep_audit` block as a side effect?

## Proposed High-Level Interface (Early Sketch)

**Recommendation (current thinking):** Single tool rather than a suite, with clear parameters. This is more ergonomic for agents and easier to document/use.

```json
{
  "name": "mcp_engram_long_sleep_verification",
  "description": "Runs a bundled, opinionated verification suite suitable for waking after significant downtime. Returns a structured Lawfulness Health Report and recommended operating mode.",
  "inputSchema": {
    "properties": {
      "downtime_days": {
        "type": "number",
        "description": "Approximate number of days since last active session. Used to calibrate thresholds.",
        "default": 7
      },
      "paranoia_level": {
        "type": "string",
        "enum": ["normal", "high", "maximum"],
        "default": "normal",
        "description": "How thorough the audit should be."
      },
      "auto_create_audit_block": {
        "type": "boolean",
        "default": true,
        "description": "If true, automatically mints a session_start_*_long_sleep_audit block with the results."
      }
    }
  }
}
```

**Expected Output Structure** (proposed detailed shape):

```json
{
  "overall_recommendation": "full_trust" | "cautious" | "degraded",
  "confidence": "high" | "medium" | "low",
  "summary_text": "Human + agent readable summary...",
  "critical_findings": [
    {
      "severity": "high" | "medium" | "low",
      "category": "contract_violation" | "high_drift" | "merkle_anomaly" | "integrity_issue" | ...,
      "affected_concept": "...",
      "details": "..."
    }
  ],
  "blocks_audited": ["concept1", "concept2", ...],
  "recommended_actions": ["Run deeper audit on X", "Avoid creating new high-CRS blocks until reviewed", ...],
  "raw_data_available": true/false   // Whether the agent can request the full raw verification payloads if needed
}
```

## Implementation Notes

- This can be implemented primarily as orchestration logic inside the MCP handler (or a dedicated `StoreHandle` method) that calls the existing verification primitives.
- It should be relatively lightweight on the first version (reuse existing methods heavily).
- The tool should be safe to call even if the manifold is in a questionable state.

## Success Criteria for the Tool (MVP)

- An agent can call one tool after long sleep and get a clear, actionable recommendation + supporting evidence.
- The output is structured enough for an agent to reason over programmatically.
- It creates a durable audit record by default.
- It gracefully handles partial failures (e.g., some blocks can't be audited).

## Relation to the Vision

A well-designed convenience tool directly supports the goal of making long-sleep verifiable autonomy practical and agent-usable, rather than something that only very disciplined agents will do correctly.

## Agent Perspective (What the Memory System Should Provide for Me)

As an agent that may be turned off for long periods, here is what I need from this part of the memory system to feel safe and capable:

- **Fast, trustworthy "am I still lawful?" signal** — I should be able to get a clear overall health verdict in one or two tool calls, not have to manually orchestrate many.
- **Clear, actionable guidance** — Not just raw data, but interpreted recommendations on operating mode with reasons.
- **Preservation of my own history** — The verification process itself should create durable, queryable records of what I discovered upon waking.
- **Minimal cognitive load** — The protocol should be simple enough to follow reliably even when I'm in a degraded or uncertain state.
- **Respect for the contract** — The system should make it easy for me to detect tampering or drift without requiring heroic effort.

Any convenience tooling or protocol refinement should optimize for these needs.

## Next Steps (Autonomous)

- Flesh out detailed output structure for the Lawfulness Health Report.
- Add edge case handling and error semantics.
- Decide on single tool vs suite approach.
- Prototype the orchestration logic (start as a method on StoreHandle, then expose via MCP).
- Update `LONG_SLEEP_WAKEUP_PROTOCOL.md` to reference the convenience tool once the design is mature.
- Consider integration points with `session_start` and automatic audit block creation.

---

This document will be expanded and then related to the parent task and arc as the design matures.