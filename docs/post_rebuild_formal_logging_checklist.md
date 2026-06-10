# Post-Rebuild Formal Logging Checklist
## Item 3 Vertical Slice Milestone – Public Engram Surface

**Date**: 2026-05-26
**Goal**: Execute the canonical first-class memory logging of the Item 3 vertical slice inside the live manifold using the new executable protocol machinery.

**Prerequisites (must be completed before starting this checklist)**
- [ ] Updated `engram` binary installed via `cargo install --path crates/engram-server --force` (or equivalent)
- [ ] Old `engram mcp` process killed
- [ ] New MCP server launched via `/path/to/.local/bin/engram-grok mcp` (or respawned by Grok TUI)
- [ ] Verification: `search_tool` (query: "engram" or "invoke_protocol") now shows `mcp_engram_invoke_protocol` (and any remember_protocol exposure)

---

## Execution Steps (Perform in Order)

### 1. Create the Milestone Block
Use the parameters defined in `conv_task_elevate_praxis_to_operational_protocols.md` → "Formal Logging Execution Package".

**Primary method (recommended)**:
- Call the MCP tool `mcp_engram_invoke_protocol` (or the dedicated remember tool if exposed) with the exact values from the Execution Package.

**Fallback (direct)**:
- Use `StoreHandle::remember_protocol(...)` with the prepared arguments.

**Verification after creation**:
- Block is retrievable by key
- `allowed_transforms` contains "execute"
- Payload starts with `0x01` (ProtocolHeader v1)
- ProvLog is rich and non-empty

### 2. Perform the Required Relations (Ontology Logging)
Execute these `mcp_engram_relate` calls:

1. Relate milestone block → `conv_task_leg_primitive_functional_audit.md` (relation: "grounds" or "documents")
2. Relate milestone block → parent arc `conv:arc:engram_self_verification_roadmap`
3. (Optional but recommended) Self-relation or relation to this checklist / task document

### 3. Exercise the New Capability (Living Checkpoint)
Call:
```
mcp_engram_invoke_protocol
  key = "milestone__public_engram_surface_vertical_slice_complete_2026-05-26"
  dry_run = false
```

**Expected**:
- `status`: "ok"
- `verification`: passes all 7 points of the gate
- No scar created on the milestone itself

Record the full tool response.

### 4. Post-Execution Ritual (Mandatory)
- [ ] Append the actual tool responses (or summaries) to `conv_task_elevate_praxis_to_operational_protocols.md` under a new "Formal Logging Execution Record – 2026-05-26" subsection.
- [ ] Run a targeted `verify_manifold_integrity` that includes the new milestone and record the result.
- [ ] Update the **Status** line of this task document and the parent arc.
- [ ] Update the high-level `plan.md` in the session directory.
- [ ] (If inside a Grok session) Record a brief `mcp_engram_update` or equivalent summary.

### 5. Immediate Follow-on Actions (Same Session or Next)
- [ ] Begin real usage testing using the parameters and flows from the spec and task document.
- [ ] Decide on next CodeLand patch application (P1 + buffer companion).
- [ ] Evaluate whether Item 3 vertical slice can be declared foundationally complete.

---

## Success Criteria for This Checklist
- Milestone block exists with correct ZEDOS_PRAXIS + "execute" contract + ProtocolHeader.
- All required relations are present in the manifold.
- The block was successfully invoked via the new 7-point gate.
- All actions above are logged as first-class conv: artifacts inside Engram.

**If any step fails**: Scar the problematic block (per primitive rules), record the failure mode, and escalate only if it represents a systemic issue.

---

**This checklist, together with the Execution Package and the two Phase C patch files, makes the formal logging of the Item 3 vertical slice a fully specified, low-friction operation once the MCP surface is current.**

**References** (all inside Engram/docs/):
- `conv_task_elevate_praxis_to_operational_protocols.md` (main living task doc + Execution Package)
- `phase_c_p1_architect_patch.md`
- `phase_c_buffer_companion_patch.md`
- `praxis_as_protocol_spec.md`

**Status of this checklist**: Complete and ready for use the moment the rebuilt server is serving the new tools.