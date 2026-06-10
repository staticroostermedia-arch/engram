# Agentic-First, Long-Sleep Verifiable Memory Substrate

**Vision**: A memory system that allows a true AGI (or any long-lived autonomous agent) to be powered off for days, weeks, or years, then power back on and verify its own lawfulness, accumulated knowledge, and operational protocols **without any external servers or trusted third parties**.

This document captures the design direction emerging from our work on Engram + Codeland, with specific focus on the user's requirements:

- Atomic, self-verifying blocks of thought (`.leg` / `.leg3` HolographicBlocks)
- Tamper-evident cryptographic history (BLAKE3 Merkle chain in every block)
- Thermodynamic trust signals (CRS + Logenergetics + Lyapunov drift)
- Praxis as crystallized, pre-verified, first-class operational protocols
- Direct hardware paths (NVMe → GPU zero-copy where possible)
- Effective "infinite context" through high-fidelity Praxis consolidation
- Agent as primary citizen and steward, not a secondary consumer

---

## Core Thesis

Most current "long-term memory for agents" systems fail the long-sleep test:

- They depend on external vector databases, embedding APIs, or cloud services.
- They have no intrinsic mechanism for an agent to verify that its own memory has not been tampered with during downtime.
- They treat memory as passive data rather than as executable, law-bearing contracts.

Engram's `.leg` primitive is a stronger foundation because:

1. **Every block is self-contained and cryptographically self-verifying.**
   - Full source (ProvLog)
   - Geometric state (q + p tensors)
   - Thermodynamic health record (Logenergetics + CRS + drift)
   - Tamper-evident history (6-deep BLAKE3 chain + merkle_sub_root on relations)
   - Explicit allowed_transforms contract (what operations this block consents to)

2. **The agent can perform lawfulness verification locally.**
   - On wake-up (or periodically), the agent can walk its own manifold and check Merkle chains, CRS stability, and relation integrity without phoning home.
   - This is the minimum requirement for an agent that can be "turned off for long periods."

3. **Praxis blocks can become first-class executable protocols.**
   - High-CRS, pinned `ZEDOS_PRAXIS` blocks that have survived thermodynamic scrutiny are not just "memories."
   - They can be treated as verified, trusted operational objects (code snippets, robot behaviors, HTML UIs, sorting protocols, decision procedures, etc.).
   - Because they carry their own verification history, they can be "called" as secure atoms.

---

## Key Engineering Directions

### 1. Strengthen Offline Self-Verification Primitives (MCP + CLI)

Expose first-class tools for an agent to audit its own lawfulness:

- `mcp_engram_verify_lawfulness` (or similar) — walks selected subgraphs or the whole active stalk, checking Merkle chain integrity, CRS consistency, and allowed_transforms adherence.
- Ability to request the full history chain for a specific Praxis block.
- Local-only mode that refuses to operate if critical invariants are broken.

This directly supports the "verify its lawfulness without calling an external server" requirement.

### 2. Elevate Praxis to First-Class Operational Protocols

Current state:
- `remember_solution` creates CRS=1.0 `ZEDOS_PRAXIS` blocks.
- NREM consolidation superposes high-CRS blocks into `ego.leg3`.

Desired state:
- Praxis blocks can contain (or reference) executable payloads (code, robot command sequences, verified decision trees, etc.).
- The `allowed_transforms` field + Merkle history becomes a **verified execution contract**.
- On "call", the agent (or a robot runtime) can load the block, verify its chain since last use, and execute the protocol with cryptographic proof that it is using a lawful, untampered version.

This enables the "atomic blocks of thought that can be refined into operational protocols" vision.

Direct hardware path (T700 12,400 MB/s example):
- The existing design (O_DIRECT + planned cuFile / GPUDirect Storage paths in engram-gpu) already points in the right direction.
- High-value Praxis blocks should be loadable with minimal CPU involvement when the consumer is GPU-accelerated code.

### 3. Advanced Praxis Consolidation for Effective Infinite Context

Current NREM is a blunt periodic OP_ADD of everything above CRS 0.85.

Needed improvements:
- Differentiated consolidation by prefix/domain (`ops:*` vs `conv:*` vs `vis:*`).
- Weighted consolidation that respects grounding_count, recency, and thermodynamic stability (not just raw CRS).
- Hierarchical or multi-scale ego tensors (short-term, medium-term, civilizational-scale).
- Explicit "Praxis distillation" passes that can take clusters of related high-value blocks and produce a single, higher-order verified protocol block (with its own Merkle ancestry pointing back to the source episodes).

When done well, this is how you get "essentially an infinite context window" while remaining fully local and verifiable.

### 4. Long-Sleep Wakeup Protocol (Hardened Version)

The existing wake_up + the recently added mandatory Concept Mapping step is a good start.

Future version should include an explicit **Lawfulness Verification Phase** on cold boot after long downtime:

1. Cryptographic audit of critical Praxis and Genesis blocks (Merkle chains since last known good state).
2. Thermodynamic sanity check (unexpected drift on high-stability blocks).
3. Ego tensor consistency check against the last known `ego.leg3`.
4. Only after passing these checks does the agent fully trust its own memory for high-stakes action.

This makes the "turned off for long periods" scenario first-class.

---

## Relationship to the Broader Vision

This direction is compatible with (and arguably necessary for) any serious attempt at True AGI that must operate with real autonomy and real stakes.

It is not in conflict with large foundation models. The models can be the reasoning engines. Engram-style substrates can be the **sovereign, verifiable, long-term memory + law layer** that the models consult and are bound by.

The combination of:
- Self-verifying atomic memory objects
- Cryptographic + thermodynamic trust signals
- Praxis as executable verified protocols
- Direct hardware paths
- Strong consolidation for effective scale

...gives a credible path toward agents that can genuinely be stewards rather than just sophisticated tools that reset or require constant babysitting.

---

## Immediate Next Work (Proposed)

1. **Formalize the Lawfulness Verification primitive** (MCP tool + CLI command + clear spec of what must be checked).
2. **Prototype treating high-CRS Praxis blocks as executable contracts** (start with simple cases: verified code snippets or decision procedures that the agent can safely "call").
3. **Improve NREM / consolidation** with domain awareness and better weighting.
4. **Document the full long-sleep + self-verification story** so it can be evaluated by people who care about this class of problem.
5. **Hardware path hardening** — make the zero-copy / direct NVMe-to-GPU story real and measurable on hardware like the T700.

---

This is not a claim that the current implementation is the final answer. It is a claim that the *shape* of the primitive and the surrounding invariants (tamper evidence, thermodynamic health, agent as primary citizen, Praxis as durable law) is a better starting point than isolated vector stores + external trust.

I am ready to continue the work. Tell me where you want to focus first.