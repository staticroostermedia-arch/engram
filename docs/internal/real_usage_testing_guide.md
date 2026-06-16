# Real Usage Testing Guide – Item 3 Vertical Slice

**Audience:** Maintainers validating executable praxis protocols on the live MCP surface.  
**Status:** Draft (2026-05-26). Not part of the public onboarding path.

**Related:**
- [docs/RITUALS.md](../RITUALS.md) (A/D/R trace triad)
- [docs/LAWFULNESS_VERIFICATION_PRIMITIVES.md](../LAWFULNESS_VERIFICATION_PRIMITIVES.md)

---

## Goal

Once the updated Engram binary is serving the live MCP surface (with `remember_protocol`, `invoke_protocol` + full 7-point gate, and `mcp_engram_invoke_protocol`), execute a focused set of real usage tests that prove the vertical slice works end-to-end against the actual manifold.

Tests must exercise both success paths and the safety mechanisms (scar-on-failure).

All results must be logged back into the Engram manifold as first-class artifacts.

---

## Prerequisites

- [ ] Rebuilt `engram` binary installed and MCP server restarted via the official launcher.
- [ ] New tools visible via MCP surface (`mcp_engram_invoke_protocol` at minimum).
- [ ] Access to `mcp_engram_verify_block_lawfulness` and `verify_manifold_integrity` for pre/post checks.

---

## Test Scenarios

### 1. Basic Protocol Creation (remember_protocol path)

**Objective:** Create a simple executable protocol block using the new surface.

**Steps:**

1. Call `remember_protocol` (or the MCP equivalent) with a clear `key`, `protocol_type = 1`, `allowed_transforms = "evidence_update,execute"`, a 32-byte ProtocolHeader, and rich ProvLog text.
2. Retrieve by key; confirm `allowed_transforms` contains `execute`, payload starts with `0x01`, run `get_block_lawfulness_summary`.

**Success:** Block created without error; no scar on creation.

### 2. Successful Invocation (7-point gate — happy path)

**Objective:** Prove the verification gate works and dispatch succeeds.

**Steps:**

1. Use a block from Test 1.
2. Call `mcp_engram_invoke_protocol` with `dry_run = false`.
3. Confirm `status` is ok, all verification points passed, result returned.

### 3. Scar-on-failure path

**Objective:** Verify rejection + scar when a block lacks required gate elements.

**Steps:**

1. Create or locate a block missing `execute` in `allowed_transforms` (or low CRS / empty ProvLog).
2. Attempt invoke; confirm clear error and scar behavior.
3. Re-check with `get_block_lawfulness_summary`.

### 4. Integration with formal milestone block

Invoke the milestone block (dry_run first), run targeted `verify_manifold_integrity`, relate observations to the milestone and this guide.

### 5. Edge cases (stretch)

Richer contracts, `requires_explicit_user_confirmation`, rapid re-invoke / use_count tracking.

---

## Logging requirements

Every test run should produce manifold memory: trace outcomes, relate test blocks to this guide, record scars/CRS/gate behavior, run a checkpoint ritual at session end.

---

## Recommended order

1. Basic creation → 2. Happy-path invoke → 3. Scar-on-failure → 4. Milestone integration → 5. Edge cases

---

## Session success criteria

- Tests 1–3 completed cleanly.
- Gate + scar demonstrated.
- Results logged with proper relations.
- No instability or data loss.