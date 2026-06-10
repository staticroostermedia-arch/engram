# Reasoning Functors as Extension of Praxis Architecture

**Date**: 2026-05-26
**Status**: Design Sketch v0.1
**Related**:
- `conv:task:elevate_praxis_to_operational_protocols`
- `docs/praxis_as_protocol_spec.md`
- `conv_task_agent_self_model_reasoning_trace_continuation.md` (Items 1+2)
- `docs/conv_task_spatial_impact_ritual_hardening.md`

## 1. Core Question

How should **functor-style compression of reasoning traces** (identity-preserving collapse of serial justification chains) relate to the existing `ZEDOS_PRAXIS` and executable protocol system?

## 2. Existing Praxis Strengths (Relevant to This Idea)

From the `.leg` audit and `praxis_as_protocol_spec.md`:

- Strong 7-point verification gate (`invoke_protocol`)
- `allowed_transforms` contract + `enforce_contract`
- ProvLog + Merkle_sub_root for provenance
- CRS + thermodynamic scoring (penalizes unstable compression)
- Explicit human/agent ownership possible via flags (`requires_explicit_user_confirmation`)
- Already uses "Categorical Functor" language in AGENT_INTEGRATION_GUIDE for KnowledgeMint

Praxis is the natural home for **crystallized, auditable, executable knowledge**.

## 3. Key Distinction

| Aspect                      | Current Praxis (error→solution)          | Reasoning Trace Functor / Compression                  |
|----------------------------|-------------------------------------------|-------------------------------------------------------|
| Primary Content            | Operational rule ("when X error, do Y")   | Identity-preserving summary of a reasoning chain      |
| Goal                       | Reusable behavior                         | Auditability + humbleness + narrative continuity      |
| Compression Role           | Implicit (the solution is the compression) | Explicit (functor that collapses detailed trace while preserving key invariants) |
| Audit Need                 | High (must verify the rule still holds)   | Very High (must be able to unfold back to faulty step)|
| Natural Minting Point      | After verified success                    | At session_end (human or empowered agent decision)    |

They are **siblings in the same family**, not identical.

## 4. Recommended Integration Approach

**Do not create a completely separate ZEDOS_FUNCTOR tag** (at least not initially).

Instead:

### 4.1 Extend the `protocol_type` Registry

Add to the existing registry in `praxis_as_protocol_spec.md`:

- `0x10` — Reasoning Compression Functor (Narrative Identity Preserver)
  - Compresses a chain of reasoning trace segments.
  - Must store Merkle fingerprint (or list of `prev_in_trace` links) of the source chain.
  - Must declare what invariants it preserves.
  - Supports "unfold" operation for audit.

### 4.2 New `allowed_transforms` Tokens (optional but recommended)

- `compress` — Permission to treat this block as a valid compression of prior trace.
- `unfold` — Permission to return the detailed source chain for audit.
- `evolve_compression` — Permission for NREM or later sessions to refine the compression while preserving identity.

### 4.3 Payload Structure for Reasoning Functors

Inside the normal ProvLog-rich payload, after the standard `ProtocolHeader`:

```json
{
  "functor": {
    "source_trace_segments": ["trace_123", "trace_124", ...],
    "merkle_root_of_chain": "hex",
    "preserved_invariants": ["causal_justification", "rejected_alternatives", "falsifiability"],
    "compression_rationale": "Human + agent justification for why this compression is safe and identity-preserving",
    "unfold_contract": "evidence_update,unfold"
  }
}
```

The rich ProvLog text before/after this section explains the actual reasoning in human terms.

### 4.4 Verification Gate Extension

The existing 7-point gate in `invoke_protocol` still applies.

Add one optional Praxis-specific check for functor blocks:

- If `protocol_type == 0x10`, the lawfulness summary or a new helper should be able to validate that the referenced source trace segments still exist and their Merkle fingerprint matches (or at least that the chain has not been tampered since compression).

## 5. Session_End as the Natural Minting Gate

Strong agreement with the user's point:

- `session_end` is the correct, human/agent-owned compression point.
- At session_end, the agent (or human) reviews the accumulated reasoning segments from the session.
- They explicitly decide which chains are stable enough to compress into a Reasoning Functor block.
- This block can be minted as `ZEDOS_PRAXIS` (with the new `0x10` type) and related back to the detailed trace via `compresses_chain_from`.
- This creates strong ownership and "humbleness" — compression is a deliberate, auditable act, not automatic.

This aligns perfectly with the "empowered agent or human-in-the-loop" model.

## 6. Benefits of This Integration

- Reuses all existing Praxis machinery (verification, invocation, scar narrowing, NREM compatibility, allowed_transforms).
- Makes the "cost of lying" even higher: a false compression would have to survive both the detailed trace scrutiny *and* the functor verification gate.
- Enables clean layering: detailed trace (for deep audit) + functor (for normal operation and inheritance).
- Directly supports the user's vision of tracing back to the exact mistaken step.

## 7. Open Questions / Next Work

- Should there be a lightweight "trace segment" block type separate from full Praxis, with only certain high-value ones promoted to full Reasoning Functor Praxis?
- How should NREM treat functor blocks vs raw trace segments?
- What does the "unfold" operation actually return to the agent (raw previous segments, or a reconstructed narrative)?

## 8. Recommendation

Integrate Reasoning Compression Functors **as a specialized protocol_type inside the existing Praxis system**, not as a parallel mechanism.

This is the most coherent, least-fragmenting path that respects the strong primitive audit work already done.

## 9. Broader Payload Vision (2026-05-26 Addition)

The .leg3 block is a general-purpose verified container. Beyond text memory and reasoning traces, blocks can carry:

- HTML thought tiles for WebGUI visualization of the agent's mind
- Functored / structured sets of tool calls (with schemas)
- Other modalities (images, audio descriptors, binary data) with cryptographic provenance

The raw 122kB payload region + existing ProvLog Cap'n Proto schema (modality union including rawMimeType, codeSignature, tripleJson, etc.) already provide latent support for this. Combined with the NVMe → GPU direct path (GPUDirect Storage), this makes Engram unusually strong as a local backend memory system for advanced TUIs and future multi-modal agents.

Reasoning functors can themselves be payloads that describe compressed tool chains or visualization states. This dramatically increases the surface area for what a "GrokBuild" public release can credibly demonstrate.

---

**Next Immediate Steps** (if approved):
- Extend `praxis_as_protocol_spec.md` with the `0x10` type and functor vocabulary.
- Update the conv:task for elevating Praxis to reference this design.
- Evolve `session_end` skill + handler to support explicit "mint compression" decisions.
- Seed the first real example from the spatial ritual work.