# Real Usage Testing Guide – Item 3 Vertical Slice
## Executable Praxis Protocols on Public Engram Surface

**Date**: 2026-05-26
**Related Documents**:
- `conv_task_elevate_praxis_to_operational_protocols.md` (main living task + Execution Package)
- `praxis_as_protocol_spec.md`
- `post_rebuild_formal_logging_checklist.md`

---

## Goal
Once the updated Engram binary is serving the live MCP surface (with `remember_protocol`, `invoke_protocol` + full 7-point gate, and `mcp_engram_invoke_protocol`), execute a focused set of real usage tests that prove the vertical slice works end-to-end against the actual manifold.

Tests must exercise both success paths and the safety mechanisms (scar-on-failure).

All results must be logged back into the Engram manifold as first-class artifacts.

---

## Prerequisites
- [ ] Rebuilt `engram` binary installed and MCP server restarted via the official launcher (see launch sequence in main task document).
- [ ] New tools visible via `search_tool` / MCP surface (`mcp_engram_invoke_protocol` at minimum).
- [ ] The formal logging milestone block has been created (recommended but not strictly required to start basic tests).
- [ ] Access to `mcp_engram_verify_block_lawfulness` and `verify_manifold_integrity` (from Item 1) for pre/post checks.

---

## Test Scenarios

### 1. Basic Protocol Creation (remember_protocol path)
**Objective**: Create a simple executable protocol block using the new surface.

**Steps**:
1. Call `remember_protocol` (or the MCP equivalent) with:
   - A clear `key` (e.g. `test_protocol_simple_decision_v1`)
   - `protocol_type = 1`
   - `allowed_transforms = "evidence_update,execute"`
   - A 32-byte ProtocolHeader with a test dispatch_key
   - Rich human ProvLog text

2. Immediately verify the block:
   - Retrieve it by key
   - Confirm `allowed_transforms` contains "execute"
   - Confirm payload starts with 0x01 (header)
   - Run `get_block_lawfulness_summary` on it

**Success Criteria**:
- Block is created without error
- Contract and header are correctly stored
- No scar on creation

**Log**: Record the key and a short summary as a relation or update to this guide.

### 2. Successful Invocation (Full 7-Point Gate – Happy Path)
**Objective**: Prove the complete verification gate works and dispatch succeeds.

**Steps**:
1. Use a block created in Test 1 (or the formal milestone).
2. Call `mcp_engram_invoke_protocol` (or `invoke_protocol`) with:
   - `dry_run = false`
   - Optional simple args

3. Inspect the `ProtocolInvocationResult`:
   - `status` should be "ok"
   - `verification` should show all 7 points passed
   - `result` should contain dispatch output (even if stub)

4. Optional: Call again with `dry_run = true` and confirm clean dry-run behavior.

**Success Criteria**:
- Gate passes cleanly
- No scar is created
- Result is returned as expected

### 3. Scar-on-Failure Path (Safety Mechanism)
**Objective**: Verify that a block lacking a required gate element is correctly rejected and scarred.

**Steps**:
1. Create (or locate) a ZEDOS_PRAXIS block that is **missing** the "execute" token in `allowed_transforms` (or has low CRS, or empty ProvLog).
2. Attempt to invoke it via `mcp_engram_invoke_protocol`.
3. Confirm:
   - Invocation fails with a clear error (contract violation, lawfulness failure, etc.)
   - The block receives a scar (sig chain advances, contract narrows)
4. Re-check the block with `get_block_lawfulness_summary` to observe the scar effect.

**Success Criteria**:
- Failure is graceful and informative
- Scar behavior matches the primitive rules (no silent failure)
- The system remains stable

### 4. Integration with Formal Milestone Block
**Objective**: Use the actual milestone block minted during formal logging as a live test subject.

**Steps**:
1. After the milestone block exists, invoke it (dry_run first, then real if appropriate).
2. Perform a targeted `verify_manifold_integrity` that includes the milestone.
3. Relate any test observations back to the milestone block and this guide.

### 5. Edge Cases (Stretch)
- Protocol with richer deep contract (`evidence_update,execute,evolve,merge...`)
- Block with `requires_explicit_user_confirmation` flag set in the header
- Multiple rapid invokes of the same protocol (affirmation / use_count tracking if buffer is wired)

---

## Logging & Checkpoint Requirements
Every test run **must** produce internal Engram memory:

- Create or update a `conv:task:real_usage_testing_2026-05-26` (or append to this guide via relations).
- Relate each test block created to this guide and to the parent roadmap arc.
- Record key outcomes, any scars, CRS values, and gate behavior.
- Run a checkpoint ritual at the end of the testing session.

---

## Recommended Order of Execution
1. Basic creation (Test 1)
2. Happy-path invoke (Test 2)
3. Scar-on-failure (Test 3) – do this early to validate safety
4. Milestone integration (Test 4)
5. Edge cases if time/energy allows

---

## Success Criteria for the Full Testing Session
- At least Tests 1–3 completed cleanly.
- All safety mechanisms (gate + scar) demonstrated.
- Results logged inside the manifold with proper ontology links.
- No instability or data loss introduced.

---

**This guide turns the vertical slice from "implemented" into "proven in real use against the live manifold."**

When the updated MCP surface is available, this document + the Execution Package + the Post-Rebuild Checklist together provide a complete, low-friction path from rebuild → formal logging → real usage validation.

**Status**: Draft created. Ready for execution the moment the live surface with the new tools is running. Will be expanded with actual test results and observations once performed.