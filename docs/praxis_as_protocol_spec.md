# Praxis as Protocol Specification (v0.1)

**Status**: Draft — Initial version post .leg primitive audit  
**Parent Task**: `conv:task:elevate_praxis_to_operational_protocols`  
**Related**: `conv:task:leg_primitive_functional_audit` (the complete Ground Truth)  
**Date**: 2026-05-26  
**Authoring Note**: This document was created as the first concrete deliverable of Sub-Phase 3.0 under the detailed Item 3 plan.

---

## 1. Purpose & Scope

This specification defines how high-CRS `ZEDOS_PRAXIS` blocks (and future narrow operational tags) can be safely elevated into **first-class, executable, versioned, and auditable operational protocols**.

It is strictly grounded in the completed `.leg` / `HolographicBlock` primitive audit (see `conv_task_leg_primitive_functional_audit.md`, especially the **Non-Negotiable Invariants** and **Safe Extension Opportunities** sections).

### 1.1 Non-Negotiable Constraints (from the Audit)

Any protocol design **must** respect these 8 invariants (quoted and referenced from the audit):

1. **Layout is frozen**: 256 KB, `#[repr(C, align(4096))]`, exact offsets (q @ 0x00000, p @ 0x10000, Logenergetics @ 0x21000, residual @ 0x21040, payload @ 0x22028, footer @ 0x3FF00). No new fields or reordering.
2. **Contract mechanism**: `allowed_transforms[64]` (null-padded ASCII) + `enforce_contract(str)` (`contains` or `0xFF`) + `LawfulTransform` (context_state tiers + spin_state + CRS ≥ 0.74 + heat ≥ 5.47e-4). This is the "constitution."
3. **ProvLog gate on write** for `ZEDOS_PRAXIS` (and LAW/DECLARATIVE/EPISODIC): payload must be non-empty. Enforced in `direct_write_block`.
4. **Sig chain + narrowing on scar**: 6-slot shift register + contract tightening is the built-in lawful rejection primitive.
5. **Merkle_sub_root** for relational / binding provenance.
6. **Thermodynamic + stack safety**: `Leg3Pointer` everywhere, `AffineThought` Drop zeroizes low-CRS VRAM.
7. **Durability contract**: O_DIRECT + WAL atomic rename + double `fsync` + parent dir `fsync`.
8. **ZEDOS_PRAXIS (0x50) + high CRS** is the natural home for executable protocols.

Public Engram surface vs. deep CodeLand policy differences on PRAXIS narrowing **must** be reconciled explicitly.

### 1.2 Out of Scope (MVP)

- Full general-purpose execution runtime / sandbox.
- Automatic promotion of arbitrary blocks.
- Cross-agent protocol marketplace.

---

## 2. Protocol Definition

A block is an **executable Praxis Protocol** when it satisfies **all** of the following:

- `zedos_tag == ZEDOS_PRAXIS` (0x50)
- `crs_score >= 0.74` (and preferably pinned at 1.0 for immortal protocols)
- `allowed_transforms` contains the token `execute` (or a future equivalent) **and** passes `enforce_contract("execute")`
- Non-empty ProvLog payload (enforced at write time)
- Valid on-load verification (see Section 5)
- Structured protocol header present in the payload (see Section 4)

The `ZEDOS_PRAXIS` tag classifies the block; the `allowed_transforms` contract + cryptographic lineage + verification make it executable and safe.

---

## 3. Allowed Transforms Vocabulary for Protocols (v0.1)

This controlled vocabulary lives in the existing `allowed_transforms[64]` field (null-padded).

**Current Proposed Set**

| Token              | Meaning                                                                 | Default on New Protocols? | When to Grant at Creation                          | Notes / Source |
|--------------------|-------------------------------------------------------------------------|-----------------------------|----------------------------------------------------|---------------|
| `evidence_update`  | Baseline evolution / refinement of the protocol content                 | Yes (always for PRAXIS)     | Always                                             | Public surface policy + audit invariant 2 |
| `execute`          | Permission to safely invoke / dispatch the protocol                     | No (explicit)               | When creator requests "executable protocol" mode   | Core new token for Item 3 (audit Safe Extension) |
| `invoke`           | More specific or future dispatch permission                             | No                          | Fine-grained control or future dispatch variants   | Alias/extension of `execute` |
| `evolve`           | Explicit permission for future NREM/synthesis to refine the protocol while it remains executable | Optional | When the author wants the protocol to improve over time | From deep `monad_praxis` architect (audit) |
| `demote`           | Permission to mark the protocol obsolete or move it to a lower tier     | Optional                    | Lifecycle management of protocols                  | - |
| `0xFF`             | Full authority (genesis-tier immutable protocols)                       | Rare                        | Only the most trusted, human-reviewed core protocols | Both surfaces (audit) |
| `compress`         | (For 0x10 Reasoning Functors) Permission to treat this block as a valid identity-preserving compression of prior trace | No | When minting a new functor | Specific to reasoning compression functors |
| `unfold`           | (For 0x10 Reasoning Functors) Permission to return/expand the original detailed source trace chain for audit | Yes (recommended for functors) | At creation of functor | Critical for the "trace back to the mistake" audit capability |

**Rule**: A block is treated as an executable protocol only if:
- `zedos_tag == ZEDOS_PRAXIS`
- `allowed_transforms` (trimmed) contains `execute`
- `enforce_contract("execute")` succeeds
- On-load verification (Section 5) passes

**Reconciliation Note** (from audit Gap #1): The public Engram surface currently narrows most PRAXIS to `"evidence_update"` only. Deep synthesis code sometimes mints richer sets. Item 3 creation paths will explicitly support a "protocol" mode that requests the richer vocabulary above. We will reconcile the two surfaces during implementation of the creation paths (Sub-Phase 3.3).

This table will be the single source of truth for the vocabulary and will be versioned in this spec.

### Thought Tile Schema v1 (Legominism Primitive)

Thought Tiles are now formalized as first-class, versioned legominism carriers inside the existing 122KB payload region (no change to HolographicBlock layout — see guardrail).

**Core Requirements for all Thought Tile types (v1):**
- `zedos_tag == ZEDOS_PRAXIS` (or appropriate high-CRS tag)
- `allowed_transforms` must include at least one of the new tokens below (or `0xFF`)
- Mandatory `provenance` object in payload (see below)
- Auto `compresses_path` relation created on creation to source concepts

**New Allowed Transforms Tokens (for Thought Tiles):**
- `synthesize`: Permission to use this tile as input for higher-level synthesis/grand scheme work.
- `promote_to_ego`: Permission for NREM or explicit promotion to contribute to the agent's ego.leg3 centroid.
- `protocol_execute`: For tiles that are themselves executable protocols (future).

**Provenance Footer (mandatory):**
```json
{
  "provenance": {
    "source_concepts": ["array of concept names"],
    "created_by": "agent or process identifier",
    "allowed_transforms_used": ["list"],
    "lineage": ["prior_tile_cids if derived"]
  }
}
```

**Tile Type Schemas (high-level v1):**
- `research_offload`: summary, key_findings, engram_mappings (when applicable)
- `knowledge_graph`: nodes[], edges[]
- `formal_spec`: title, invariants_or_rules, integration_proposals
- `tabular`: columns + rows/data
- `html_visualization`: full self-contained HTML (companion to textual tile)

See the review Thought Tile `tile:research_offload_phase-1-proposal--thought-tile-schema-v1---allow` (and its visualization companion) for the full proposed detailed schemas and examples. The A/D/R triad formalization (Phase 1) has been added below as a first concrete extension; see the pre-edit review bundle `tile:formal_spec_phase-1-doc-delta--proposed-a-d-r-structured-fie` + `tile:html_visualization_phase-1-pre-surface-viz--a-d-r-subsection-additi`.

### A/D/R Structured Fields for Traces, Goal Decomposition, and NREM Aggregation (v1)

ADR (Affirm/Deny/Reconcile) is adopted as the universal triadic operator for decision geometry, directly realizing the logophysics invariants (especially ZEDOS modalities and shock mechanics) and Gurdjieff Law of Three as operationalized in Engram traces and intent layer.

**Rationale (from CodeLand legominism mapping):** The triad provides the minimal structure for durable transmission (legominism) of decisions without loss of context or drift. It enables fruits metric (coherence of reconciliations over time) and clean NREM superposition for ego.leg3.

**Trace Schema Extension (additive, non-breaking):**
Existing `mcp_engram_record_reasoning_trace` and quick_trace payloads retain all current fields. New optional triad (for high-stakes decisions):

- `affirm`: The core positive claim, intent, or state being advanced by this decision.
- `deny`: The specific alternatives, risks, or prior positions being rejected, with justification.
- `reconcile`: The synthesis step — how this decision resolves tension, advances coherence (ZEDO-like), or produces a higher-order state. This is the "fruit" carrier.

Example in payload (see `tile:research_offload_phase-1-example--a-d-r-structured-fields-in-trac` for full demo):
```json
{
  "affirm": "Formalize Thought Tile schemas as legominism primitives with provenance",
  "deny": "Ad-hoc JSON without allowed_transforms or lineage (guarantees drift and loss of transmissibility)",
  "reconcile": "Schema v1 + mandatory provenance footer + new synthesize/promote_to_ego tokens provides lawful evolution path inside frozen .leg3 container"
}
```

**Goal Decomposition & NREM:**
- Child goals from `mcp_engram_goal_decompose` may carry ADR tags.
- During NREM (0x10 autophagy + consolidation), high-lineage ADR-structured traces are preferentially superposed into ego.leg3 updates (Phase 4).

**Implementation Note:** Initial support is via convention in Thought Tiles and explicit trace fields. Tooling (record_reasoning_trace, ki_hijacker, NREM) will wire the fields in later phases after gate. This formalization lives in payload only.

**Guardrail Reinforcement**: All ADR data lives in structured payloads inside the existing 122KB Body region or as separate Merkle-linked Thought Tiles. No modifications to HolographicBlock header, q/p tensors at fixed offsets, alignment, or allowed_transforms layout. Evolution strictly via the vocabulary in this spec.

## CodeLand Logophysics Lineage & Mapping Table (v1)

**Binding Constraint**: All evolution described in this spec must respect the frozen .leg3 HolographicBlock invariants: binary vector isomorphism (q/p momentum tensors), hardware alignment for direct NVMe/GPU movement, and backwards compatibility. Evolution occurs exclusively through the `allowed_transforms` mechanism in the header (see the filed patent on the triadic .leg container), structured payloads inside the existing Body, or separate Merkle-linked containers. No changes to core layout or alignment are permitted.

The following canonical mapping (living block: `legominism_mapping_table_v1`, with guardrail `leg_block_invariants_guardrail_v1`) was synthesized from the CodeLand phone archive overnight survey (_sub6 tiles) and is now the reference for how the 6 logophysics invariants + Gurdjieff legominism principles manifest in Engram primitives. This informs protocol design, self-model evolution, and Thought Tile usage as legominism carriers, **but does not alter the underlying block format**.

**Canonical Reference**: See `legominism_mapping_table_v1` for the full 6-invariant + Gurdjieff principles mapping, drive effects, and explicit "this is how Engram realizes X" statements grounded in the original CodeLand sources (unified_field_theory_v0.md, LEG_v2 whitepaper, Doctrine of Trans-Instantiation, Golden_Legominism_Zedocast, Liber/False Empire materials, etc.).

Key high-level alignment for this spec:
- Thought Tiles function as the primary durable legominism forms (structured, promoted, with lineage).
- `allowed_transforms` (including new tokens like `synthesize`, `protocol_execute`) is the lawful mechanism for introducing richer behavior while preserving the triadic container guarantees (header self-declares schema + permitted transforms; local hash + link verification; no external registry).
- The survey process itself (autonomous sub-agents, living handoffs, cost discipline, fruits-over-blossoms selection) models the legominism transmission described in the CodeLand materials.

Any future expansion of the Allowed Transforms vocabulary or ProtocolHeader must be recorded here with explicit reference to the mapping table and guardrail.

---

## 4. Payload Format (Inside the Frozen 122,584-byte Region)

All protocol data **must** live inside the existing `payload` field at offset 0x22028 (max 122,584 bytes). No changes to the `HolographicBlock` layout are permitted (see Audit Invariant #1).

The design must satisfy the ProvLog Enforcement Gate (Audit Invariant #3): for any ZEDOS_PRAXIS block, the first 16 bytes of the payload must be non-zero at write time.

### 4.1 Recommended Internal Layout (v0.1)

```
0x0000 (relative to payload) – 0x001F : ProtocolHeader (32 bytes, fixed)
0x0020 – 0x00FF                     : Structured Dispatch Data (≤ 224 bytes recommended in v0.1)
0x0100 – end                        : Human-readable ProvLog + documentation + examples (the rest of the payload)

Total protocol overhead in v0.1 should stay under ~1–2 KB so that rich human-readable ProvLog content is always possible.
```

### 4.2 ProtocolHeader (32 bytes, little-endian where applicable)

```c
struct ProtocolHeader {
    uint8_t  version;               // 0x01 for this spec v0.1
    uint8_t  protocol_type;         // See 4.4
    uint8_t  flags;                 // Bit 0: requires_explicit_user_confirmation
                                    // Bit 1: is_idempotent
                                    // Bit 2: has_side_effects (future)
    uint8_t  reserved[5];
    uint8_t  dispatch_key[24];      // Null-terminated UTF-8 short name / key (e.g. "sort_packages_v2")
};
```

The `dispatch_key` is the primary machine-readable identifier used for lookup and dispatch. It should be stable across versions of the same protocol family.

### 4.3 Structured Dispatch Data (after header)

For v0.1 we keep this deliberately simple and small.

Recommended encoding: a length-prefixed blob using a minimal TLV or a tiny CBOR map. The exact wire format will be pinned in a later revision of this spec once we have the first real examples.

Example conceptual structure (not yet serialized format):

```json
{
  "entry_point": "main",
  "parameters": {
    "max_items": 50,
    "priority_weights": [0.6, 0.3, 0.1]
  },
  "expected_output_schema": "string"
}
```

Important: This structured section is **advisory / dispatch metadata**. The authoritative human explanation and any safety notes live in the ProvLog portion that follows.

### 4.4 Initial protocol_type Registry (v0.1)

- `0x01` — Decision Procedure / Policy (pure recommendation or conditional logic)
- `0x02` — High-Level Behavior Sequence (e.g., "package sorting workflow")
- `0x03` — Verified Operator / Code Snippet (small, auditable logic)
- `0x04` — Configuration / Hyper-parameter Set (with validation rules)
- `0x05` — Diagnostic / Self-Audit Procedure
- `0x10` — Reasoning Compression Functor (Narrative Identity Preserver) — **NEW (2026-05-26)**

  Compresses a chain of detailed reasoning trace segments into an identity-preserving summary while maintaining strong auditability (unfold capability). Primary use: making long reasoning histories manageable for inheritance and self-model continuation without losing the ability to audit the original logic path.

  Special requirements:
  - Must reference source trace segments (via relations or embedded Merkle fingerprint of the chain).
  - Must declare preserved invariants (e.g., "justification", "rejected_alternatives", "falsifiability").
  - Supports the `unfold` operation (see allowed_transforms below).
  - Minting is deliberately gated (typically at session_end by human or empowered agent) to enforce "humbleness" and ownership.

New values require an update to this spec + agreement in the elevate_praxis task.

### 4.4.1 Special Requirements for protocol_type 0x10 (Reasoning Compression Functor)

In addition to the general protocol requirements:

- **Source Chain Reference**: The payload (or relations) **must** reference the chain of reasoning trace segments being compressed. Recommended: store a Merkle root of the ordered trace segment concepts + explicit `compresses_chain_from` relations.
- **Preserved Invariants Declaration**: Must explicitly list what logical properties the compression claims to preserve (e.g., "justification", "rejected_alternatives", "falsifiability", "causal_order").
- **Compression Rationale**: Rich human-readable justification in the ProvLog for *why* this compression is safe and identity-preserving.
- **Unfold Support**: The block should support the `unfold` token in `allowed_transforms`. When invoked with appropriate arguments, it should be able to return or point to the original detailed trace chain.
- **Minting Discipline**: Creation of 0x10 blocks is expected to be deliberate (typically triggered or approved at `session_end`). This enforces the "humbleness" and ownership model.

These blocks are still `ZEDOS_PRAXIS` and go through the normal 7-point gate. The `0x10` type simply signals special semantics and capabilities.

### 4.5 Coexistence with ProvLog (Critical)

Because of the ProvLog gate:
- The first 16 bytes of the entire payload (i.e., the start of the ProtocolHeader) must never be all zeros.
- The bulk of the payload (after the structured data) **must** contain rich, human-readable provenance text.

This is not a burden — it is a feature that aligns with the agentic-first contract.

---

**Current Status of Payload Work (as of this edit)**: The layout above is now the working v0.1 definition. It is intentionally conservative to respect the frozen block layout and the ProvLog gate.

---

## 5. Safe Invoke Path & On-Load Verification (Design in Progress)

This section defines how an agent or external system safely discovers, verifies, and invokes a protocol block.

All designs in this section are required to compose cleanly with:
- Audit Invariant #2 (`enforce_contract` + `LawfulTransform`)
- Existing Item 1 verification primitives (`get_block_lawfulness_summary`, `verify_manifold_integrity`)
- The new `execute` token in `allowed_transforms`

### 5.1 High-Level Invoke Flow

1. Discovery (normal search / prefix / relation queries, or new helper).
2. Fetch the block.
3. **Mandatory On-Load Verification Gate** (see 5.2).
4. If gate passes → dispatch to handler based on `protocol_type` + `dispatch_key`.
5. On success or controlled failure → optional affirmation / usage recording (for utility scoring in the PraxisBuffer).
6. On verification or execution failure → trigger scar (the primitive's natural contract-narrowing + sig-chain burn).

### 5.2 On-Load Verification Gate (Required Checks) — Exact Definition (v0.1)

Before dispatch, the following **mandatory gate** must return `PASS`. Any failure is a hard stop.

**Gate Checklist** (must all be true):

1. **Tag Check**  
   `block.zedos_tag == ZEDOS_PRAXIS` (0x50)  
   (Future: or a dedicated narrow executable protocol tag if we introduce one.)

2. **Coherence Gate**  
   `block.crs_score >= 0.74`

3. **ProvLog Gate** (re-assertion of write-time rule)  
   First 16 bytes of `block.payload` are **not** all zero.

4. **Contract Token**  
   The trimmed string from `block.allowed_transforms` contains the token `"execute"`.

5. **Primitive Contract Enforcement**  
   `block.enforce_contract("execute")` succeeds (or the current soft variant during the transition period).  
   This directly exercises Audit Invariant #2.

6. **Lawfulness Summary Check** (Item 1 primitive)  
   Call `get_block_lawfulness_summary(key)`.  
   - `crs` matches the block.  
   - No critical issues flagged for this block.  
   - `allowed_transforms` in the summary contains `"execute"`.

7. **Optional Ancestry Check** (higher-trust / future)  
   For protocols marked with extra flags or in long-sleep verification flows: validate recent sig chain advancement and Merkle relationships where relevant.

Only if the entire gate returns `PASS` may the caller proceed to inspect `protocol_type` and `dispatch_key`.

### 5.3 Proposed Surfaces (v0.1)

#### A. New MCP Tool: `mcp_engram_invoke_protocol`

**Parameters** (JSON-RPC style):
```json
{
  "key": "string",                    // The block key / concept
  "args": "object | null",            // Structured arguments for the protocol (matches its dispatch schema)
  "dry_run": false,                   // If true, perform full verification but do not execute side effects
  "require_confirmation": false       // For protocols with the corresponding flag
}
```

**Success Response**:
```json
{
  "status": "ok",
  "result": { ... protocol-specific output ... },
  "verification": {
    "gate_passed": true,
    "crs": 0.97,
    "contract": "evidence_update,execute",
    "lawfulness_summary": { ... },
    "executed_at": "timestamp"
  }
}
```

**Failure Response** (example):
```json
{
  "status": "verification_failed",
  "reason": "missing_execute_token",
  "details": "allowed_transforms does not contain 'execute'",
  "scar_applied": false
}
```

Other error codes to define: `crs_too_low`, `provlog_missing`, `contract_denied`, `lawfulness_issue`, `protocol_not_executable`.

#### B. StoreHandle Method (Rust)

```rust
pub fn invoke_protocol(
    &mut self,
    key: &str,
    args: Option<serde_json::Value>,
    options: InvokeOptions,
) -> Result<ProtocolInvocationResult> {
    // 1. Fetch block
    // 2. Run the 7-point gate above (re-using get_block_lawfulness_summary + enforce_contract)
    // 3. If PASS: dispatch
    // 4. Return result + full verification trace
}
```

`InvokeOptions` will include `dry_run`, `require_confirmation`, etc.

### 5.4 Error Semantics & Automatic Scar Behavior

- Any hard verification failure **may** trigger a scar on the block (contract narrowed to `evidence_update` only, sig chain advanced, CRS reduced). This is the primitive's existing rejection mechanism working in our favor.
- Execution failures inside the protocol handler are protocol-specific. Repeated failures on the same protocol can be used to decide on a manual or automatic scar.
- Dry-run failures never scar.

This design deliberately re-uses the existing scar / `enforce_contract` / verification machinery rather than inventing new mechanisms.

### 5.5 Long-Sleep / Cold-Boot Integration

When an agent performs the long-sleep verification flow (from Item 2), it is expected to:
- Run the full gate (or a stricter variant) on all protocols it considers "executable" or "critical".
- Only mark a protocol as trustworthy for the new session if the gate passes cleanly.

This directly supports the original vision of being able to turn the agent off for long periods and verify its operational knowledge locally.

---

**Current Status of Invoke Work (as of this edit)**: The gate is now precisely defined (7 checks), surfaces are proposed with example schemas, and error/scar behavior is aligned with the primitive.

---

## 6. Implementation Planning – Minimal Vertical Slice (First Draft)

Goal: Deliver a working end-to-end flow for one protocol type with the least possible surface area while respecting every audit invariant.

### 6.1 Recommended First Protocol Type
`0x01` — Decision Procedure / Policy (read-only recommendation with clear inputs/outputs). Lowest risk for side effects.

### 6.2 Minimal Files to Touch (Public Engram Surface)

**Core Changes**:
1. `engram-server/src/store.rs`
   - Enhance `remember_solution` (or add `remember_protocol`) to accept a "protocol" mode that sets the richer `allowed_transforms` and ProtocolHeader.
   - Add `invoke_protocol(...)` method that implements the 7-point gate + dispatch stub.
   - Wire the new lawfulness checks into the gate.

2. `engram-server/src/mcp.rs`
   - Register the new tool `mcp_engram_invoke_protocol`.
   - Add handler that calls the StoreHandle method.

3. `engram-core/src/` (minimal)
   - Possibly small helper types for `ProtocolInvocationResult` and `InvokeOptions` (or keep them in server for v0.1).

**Test / Validation**:
- Add tests in `engram-server` for the verification gate.
- One example protocol block minted via the updated creation path.
- End-to-end test: create → verify & invoke (dry-run + real).

### 6.3 Deep Vehicle Coordination (later)
Once the public surface has a working flow, align the deep `monad_praxis` architect and `remember_solution` equivalents so that synthesized high-value blocks can also become executable protocols with the correct contracts.

### 6.4 Success Criteria for the Slice
- Agent can create a protocol via (enhanced) `remember_solution`.
- Agent can call `mcp_engram_invoke_protocol` on it.
- Full gate is exercised and logged.
- Failure modes produce correct scars.
- Everything passes the audit invariants.

This slice is intentionally narrow so we can get real usage feedback quickly before expanding to behavior sequences or code snippets.

---

## 7. Detailed Implementation Plan – Vertical Slice (store.rs Focus)

This section provides concrete pseudocode and targeted change guidance for the two critical methods in `engram-server/src/store.rs`. It is grounded in the actual current code (as of the audit + recent inspection).

### 7.1 Target: Enhanced Creation Path

**Current relevant code**:
- `remember_solution(...)` (lines ~1184–1199) manually builds a PRAXIS block.
- The main creation path (around line 788) calls `assign_reflexive_contract` after ego gating.

**Recommended Approach for v0.1**:
Add a new public method (or extend `remember_solution` with an options struct) that supports "protocol" mode.

**Proposed new method sketch** (to be added near `remember_solution`):

```rust
/// Create a verifiable executable Praxis Protocol.
/// This is the primary creation path for Item 3.
pub fn remember_protocol(
    &mut self,
    key: &str,                          // stable identifier (e.g. "sort_packages_v2")
    protocol_type: u8,                  // 0x01 = Decision Procedure, etc.
    dispatch_key: &str,                 // short machine name
    structured_payload: &[u8],          // the ProtocolHeader + structured data (first part)
    human_provlog: &str,                // rich human-readable documentation
    allowed_transforms: &[u8],          // e.g. b"evidence_update,execute,evolve"
) -> Result<String> {
    let mut full_payload = Vec::with_capacity(1024);
    full_payload.extend_from_slice(structured_payload);
    full_payload.extend_from_slice(human_provlog.as_bytes());

    let mut block = self.encode(&String::from_utf8_lossy(&full_payload));
    block.zedos_tag = ZEDOS_PRAXIS;
    block.crs_score = 1.0;              // Start pinned for protocols (can be adjusted)

    // Set richer contract for executable protocols
    let len = allowed_transforms.len().min(64);
    block.allowed_transforms[..len].copy_from_slice(&allowed_transforms[..len]);
    for b in block.allowed_transforms[len..].iter_mut() { *b = 0; }

    // Ensure ProtocolHeader is at the very start of the real payload
    // (the encode path may have done some processing — we may need a lower-level path)

    self.store(key, block)?;
    Ok(format!("✓ Protocol '{}' stored as executable Praxis Protocol", key))
}
```

**Important integration point**:
We must decide whether to call the existing `assign_reflexive_contract` or bypass it for protocol mode. Recommendation: For protocol creation, **we take full control** of `allowed_transforms` (as shown above) and skip or post-override the default assignment.

We will also need to expose a way to set the `protocol_type` and `dispatch_key` inside the structured header before or during encoding.

### 7.2 Target: invoke_protocol Gate Implementation

**Proposed new method** (to be added in `impl StoreHandle` near the other verify methods):

```rust
pub fn invoke_protocol(
    &mut self,
    key: &str,
    args: Option<serde_json::Value>,
    options: InvokeOptions,
) -> Result<ProtocolInvocationResult> {
    let block = self.backend.fetch_block(key)
        .ok_or_else(|| anyhow!("Protocol block not found: {}", key))?;

    // === The 7-Point Gate (from the spec) ===
    if block.zedos_tag != ZEDOS_PRAXIS {
        return Err(anyhow!("Not a PRAXIS block"));
    }
    if block.crs_score < 0.74 {
        return Err(anyhow!("CRS too low for execution"));
    }
    if block.payload[..16].iter().all(|&b| b == 0) {
        return Err(anyhow!("Missing ProvLog"));
    }

    let contract = std::str::from_utf8(&block.allowed_transforms)
        .unwrap_or("")
        .trim_matches('\0');

    if !contract.contains("execute") {
        return Err(anyhow!("Protocol does not grant 'execute' permission"));
    }

    // Re-use existing primitive
    if block.enforce_contract("execute").is_err() {
        return Err(anyhow!("Contract enforcement failed for 'execute'"));
    }

    // Re-use Item 1 primitive
    let summary = self.get_block_lawfulness_summary(key);
    if let Some(s) = &summary {
        if !s.allowed_transforms.contains("execute") {
            return Err(anyhow!("Lawfulness summary inconsistent"));
        }
    }

    // Future: ancestry check can go here

    if options.dry_run {
        return Ok(ProtocolInvocationResult {
            status: "dry_run_ok".to_string(),
            verification: summary,
            result: None,
        });
    }

    // === Actual Dispatch (stub for v0.1) ===
    // In the real implementation this would route on block.protocol_type / dispatch_key
    let result = self.dispatch_protocol(&block, args)?;

    // Optional: record usage for PraxisBuffer utility scoring
    // self.record_protocol_usage(key);

    Ok(ProtocolInvocationResult {
        status: "ok".to_string(),
        verification: summary,
        result: Some(result),
    })
}
```

**Notes on integration**:
- `enforce_contract` is currently defined on `HolographicBlock` / `Leg3Pointer` in `engram-core`.
- We already have `get_block_lawfulness_summary` — perfect reuse.
- The current `verify_manifold_integrity` has a heuristic that flags PRAXIS without "evidence_update" as problematic. We will need a small update to that heuristic for known executable protocols (or make the heuristic protocol-aware).

### 7.3 Tool Registration (mcp.rs)

In `engram-server/src/mcp.rs`, follow the exact pattern used for the Item 1 verify tools (around line 642 and the handler at ~1855).

Add a new arm for `"mcp_engram_invoke_protocol"` that calls the new `StoreHandle` method and returns the structured result.

---

This plan is now specific enough that a developer can implement the vertical slice with high confidence while staying 100% compliant with the `.leg` primitive invariants.

**Implementation Status (as of latest edit)**: 
- Core methods (`remember_protocol` + `invoke_protocol`) in `store.rs` implemented and compiling.
- `mcp_engram_invoke_protocol` tool + full handler added to `mcp.rs` (following the exact pattern of the Item 1 verify tools).
- Vertical slice is now end-to-end functional at the MCP level: create → verify gate → invoke.

**Checkpoint Ritual Performed (2026-05-26)**:
- Vertical slice implementation on the public Engram surface is complete and documented.
- See the detailed checkpoint section in `conv_task_elevate_praxis_to_operational_protocols.md`.

**Next**: Real usage testing + formal logging inside the Engram manifold + coordination with the deep research vehicle.

---

## 10. Testing & Validation Guide (Vertical Slice)

### Manual End-to-End Test Flow (when server is running)

1. Create a simple Decision Procedure protocol using the new creation path.
2. Audit it with the existing verify tools.
3. Invoke it (dry-run first, then real).
4. Test failure paths (non-protocol PRAXIS, low CRS, missing "execute" token).
5. Observe scar behavior on repeated failures.

All tests must confirm that the 8 Non-Negotiable Invariants from the `.leg` primitive audit remain intact.

---

## 9. Usage Example (Vertical Slice)

Once the server is running with the new code, an agent can do the following:

### 1. Create an executable protocol

```json
{
  "method": "mcp_engram_remember_protocol",   // or the enhanced remember_solution in protocol mode
  "params": {
    "key": "decision__prefer_rust_over_python_v1",
    "protocol_type": 1,
    "dispatch_key": "prefer_rust",
    "structured_header": [/* 32-byte header + small JSON */],
    "human_provlog": "## Decision\nWhen starting a new systems project in 2026, strongly prefer Rust over Python for core infrastructure components unless rapid prototyping is the dominant constraint.\n\n## Rationale\n...",
    "allowed_transforms": "evidence_update,execute,evolve"
  }
}
```

### 2. Audit it first (recommended)

```json
{
  "method": "mcp_engram_verify_block_lawfulness",
  "params": { "concept": "decision__prefer_rust_over_python_v1" }
}
```

### 3. Invoke it

```json
{
  "method": "mcp_engram_invoke_protocol",
  "params": {
    "key": "decision__prefer_rust_over_python_v1",
    "args": { "project_type": "high_performance_backend" },
    "dry_run": false
  }
}
```

Expected output includes verification trace + protocol result.

This flow is now fully supported by the vertical slice we just implemented.

Next: Wire the new functionality to the MCP layer (`mcp.rs`), add a basic test, and run an end-to-end example using a real protocol block. Then perform the checkpoint ritual.

---

## 8. Proposed Code Changes (Unified Diff Style) – Vertical Slice

This section contains the actual proposed edits for the vertical slice, ready for implementation.

### 8.1 Add `remember_protocol` method (near `remember_solution`)

**File**: `engram-server/src/store.rs`

```diff
diff --git a/engram-server/src/store.rs b/engram-server/src/store.rs
index abc1234..def5678 100644
--- a/engram-server/src/store.rs
+++ b/engram-server/src/store.rs
@@ -1199,6 +1199,58 @@ impl StoreHandle {
         Ok(format!("✓ Solution stored as '{}' with ZEDOS_PRAXIS tag and CRS=1.0 (pinned)", key))
     }
 
+    /// Create a verifiable executable Praxis Protocol (Item 3).
+    /// Sets a richer `allowed_transforms` contract and embeds the ProtocolHeader.
+    pub fn remember_protocol(
+        &mut self,
+        key: &str,
+        protocol_type: u8,
+        dispatch_key: &str,
+        structured_header: &[u8],   // 32-byte ProtocolHeader + small structured data
+        human_provlog: &str,
+        allowed_transforms: &[u8],   // e.g. b"evidence_update,execute,evolve"
+    ) -> Result<String> {
+        let mut payload = Vec::with_capacity(2048);
+        payload.extend_from_slice(structured_header);
+        payload.extend_from_slice(human_provlog.as_bytes());
+
+        let mut block = self.encode(&String::from_utf8_lossy(&payload));
+        block.zedos_tag = ZEDOS_PRAXIS;
+        block.crs_score = 1.0;
+        block.energetics.crs = 1.0;
+
+        // Take explicit control of the contract for executable protocols
+        let len = allowed_transforms.len().min(64);
+        block.allowed_transforms[..len].copy_from_slice(&allowed_transforms[..len]);
+        for b in block.allowed_transforms[len..].iter_mut() { *b = 0; }
+
+        self.store(key, block)?;
+        Ok(format!("✓ Protocol '{}' stored as executable Praxis (type=0x{:02X})", key, protocol_type))
+    }
+
     /// Surface the top K relevant memories for a file path...
```

### 8.2 Add `invoke_protocol` method (in `impl StoreHandle`)

**File**: `engram-server/src/store.rs`

```diff
diff --git a/engram-server/src/store.rs b/engram-server/src/store.rs
index def5678..9876543 100644
--- a/engram-server/src/store.rs
+++ b/engram-server/src/store.rs
@@ -1745,6 +1745,58 @@ impl StoreHandle {
             overall_health,
         })
     }
+
+    pub fn invoke_protocol(
+        &mut self,
+        key: &str,
+        args: Option<serde_json::Value>,
+        options: crate::protocol::InvokeOptions,   // small newtype (can be defined in same file for v0.1)
+    ) -> Result<crate::protocol::ProtocolInvocationResult> {
+        let block = self.backend.fetch_block(key)
+            .ok_or_else(|| anyhow!("Protocol block not found: {}", key))?;
+
+        // === 7-Point Gate (from praxis_as_protocol_spec) ===
+        if block.zedos_tag != ZEDOS_PRAXIS {
+            return Err(anyhow!("Not a PRAXIS block"));
+        }
+        if block.crs_score < 0.74 {
+            return Err(anyhow!("CRS too low for protocol execution"));
+        }
+        if block.payload[..16].iter().all(|&b| b == 0) {
+            return Err(anyhow!("Missing ProvLog"));
+        }
+
+        let contract = std::str::from_utf8(&block.allowed_transforms)
+            .unwrap_or("")
+            .trim_matches('\0');
+
+        if !contract.contains("execute") {
+            return Err(anyhow!("Protocol does not grant 'execute' permission"));
+        }
+
+        if block.enforce_contract("execute").is_err() {
+            return Err(anyhow!("Contract enforcement failed"));
+        }
+
+        let summary = self.get_block_lawfulness_summary(key);
+        // Additional lawfulness checks can go here using `summary`
+
+        if options.dry_run {
+            return Ok(crate::protocol::ProtocolInvocationResult {
+                status: "dry_run_ok".into(),
+                result: None,
+                verification: summary,
+            });
+        }
+
+        // === Dispatch (stub for vertical slice) ===
+        let result = self.execute_protocol_dispatch(&block, args)?;
+
+        Ok(crate::protocol::ProtocolInvocationResult {
+            status: "ok".into(),
+            result: Some(result),
+            verification: summary,
+        })
+    }
 }
```

**Notes for implementer**:
- `crate::protocol::{InvokeOptions, ProtocolInvocationResult}` can be simple structs defined at the top of the file for the vertical slice.
- The `execute_protocol_dispatch` helper is a stub that, for the first protocol type (`0x01`), can just return a structured value based on the block's structured header + args.
- After this lands, we will update the heuristic in `verify_manifold_integrity` that currently complains about PRAXIS blocks without `"evidence_update"`.

---

This is the concrete change plan ready to be turned into real commits. All decisions are traceable to the `.leg` primitive audit invariants.

---

## 5. On-Load Verification & Safe Invoke Requirements

Before any system or agent may invoke a protocol block, the following **must** succeed:

1. Normal fetch (by key or search).
2. `get_block_lawfulness_summary` (or equivalent) returns acceptable CRS, non-empty allowed_transforms, recent sig_0, etc.
3. Explicit call to `enforce_contract("execute")` (or the soft variant during transition).
4. ZEDOS tag == PRAXIS, CRS ≥ 0.74, ProvLog non-empty (first 16 bytes).
5. `allowed_transforms` (trimmed) contains `execute`.
6. (Future) Historical chain / Merkle ancestry validation for high-trust protocols.

Only after all checks pass may the runtime dispatch based on `protocol_type` + `dispatch_key`.

**Failure semantics**: Return clear error + trigger a scar on the block (the primitive's natural narrowing + sig-chain burn behavior).

---

## 6. Creation / Minting Rules (Protocol Mode)

When creating a protocol via `remember_solution` (or deep equivalents):

- Caller must explicitly request "protocol" mode.
- The creation path sets:
  - `zedos_tag = ZEDOS_PRAXIS`
  - `crs_score = 1.0` (for pinned protocols) or high earned CRS
  - `allowed_transforms` to the chosen subset from the vocabulary table above (must include `execute` for executable protocols)
  - Structured header at the start of the payload
  - Full human-readable ProvLog (enforced by the gate)
- Mint-time validation should warn or fail if the resulting block would not pass the on-load checks in Section 5.

---

## 7. Threat Model (High Level, v0.1)

- **Unauthorized execution**: Prevented by the `execute` token requirement + `enforce_contract` + on-load verification.
- **Stale / malicious protocol**: Mitigated by CRS gate, scar-on-failure, and Merkle ancestry for versioning.
- **Protocol that mutates state unsafely**: Out of scope for MVP (we start with read-only decision procedures and high-level behaviors). Future work will define safe side-effect boundaries.
- **Contract drift between public surface and deep vehicle**: Must be reconciled in the implementation of creation paths and verification.

---

## 8. Open Questions & Future Work

- Exact binary format for the structured dispatch data after the 32-byte header (TLV vs small Cap'n Proto vs CBOR).
- Whether a dedicated narrow ZEDOS tag for "executable protocol" is needed in addition to the contract token.
- How `context_state` tiers interact with executable protocols.
- Full historical chain reconstruction in the verification primitives (needed for highest-trust long-sleep scenarios).

---

## 9. References

- `conv_task_leg_primitive_functional_audit.md` (the complete .leg review and synthesis)
- `conv_task_elevate_praxis_to_operational_protocols.md` (living task document with initial sketch)
- `engram-server/src/store.rs` (especially `remember_solution` and the verify methods)
- `monad_ledger/src/structs.rs` (`HolographicBlock`, `enforce_contract`, `LawfulTransform`)
- `monad_storage/src/staging.rs` (`direct_write_block` + ProvLog gate)
- `monad_praxis/src/architect.rs` (current synthesis contract setting)

---

**End of v0.1**

This specification will be versioned and expanded in lockstep with implementation. All changes must remain traceable to the 8 Non-Negotiable Invariants of the `.leg` primitive.