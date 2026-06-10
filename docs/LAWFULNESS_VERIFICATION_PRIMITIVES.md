# Lawfulness Verification Primitives for Engram

**Status**: Design Specification (v0.1)  
**Date**: 2026-05-25  
**Priority**: Highest (Foundation for long-sleep agent autonomy)

## 1. Motivation & Requirements

The core promise of this memory system for "True AGI" style agents is:

> An agent must be able to be powered off for arbitrary periods and, upon waking, credibly verify that its own accumulated knowledge, decisions, and operational protocols remain lawful and untampered — using only local resources and the intrinsic properties of the substrate.

**CodeLand Lineage Note (v1)**: The 6 logophysics invariants + Gurdjieff legominism principles (see living block `legominism_mapping_table_v1`) provide the deep mechanical source for what "lawful" means in this substrate. The Liber/False Empire experiment was explicitly testing attention mechanics, durable forms (legominisms), metabolism via A/D/R, and conscience shocks — precisely the dynamics these verification primitives must protect. All verification must respect the frozen .leg3 block invariants (binary vector isomorphism, hardware alignment for direct movement, backwards compatibility — see `leg_block_invariants_guardrail_v1`). Evolution of verification logic itself must use Allowed Transforms.

Current state:
- The `.leg` format already provides excellent tamper-evidence via the 6-deep BLAKE3 chain (`sig_0`–`sig_5`) + `merkle_sub_root` on relations.
- `allowed_transforms` provides reflexive contracts on blocks (especially strong on `ZEDOS_PRAXIS`).
- `verify_hypothesis` exists but is narrow (only for `ZEDOS_HYPOTHESIS` promotion).
- Enforcement of contracts is currently soft (warning logs only).
- There is no high-level, agent-friendly way to ask "Is my memory still lawful?"

This spec defines the missing primitives.

## 2. Goals

1. **Local, offline verifiability** — No external servers required.
2. **Agent-usable via MCP** — First-class tools an agent can call during wake-up or periodically.
3. **Practical for long-sleep scenarios** — An agent coming back after days/weeks/months can run a reasonable audit in seconds to minutes.
4. **Composable with existing mechanisms** — Builds on `fetch_block`, the Merkle footer, `allowed_transforms`, CRS, and ZEDOS tags.
5. **Safe by default** — Verification should be cheap to run and hard to abuse for DoS.

## 3. Proposed MCP Tools (Public Surface)

### 3.1 `mcp_engram_verify_block_lawfulness`

**Purpose**: Deep audit of a single high-value block (especially Praxis or Genesis).

**Input**:
- `concept`: string (required)
- `check_merkle_chain`: bool (default true)
- `check_contract`: bool (default true)
- `max_history_depth`: int (default 6, the full chain)

**Behavior**:
- Fetches the block.
- If `check_merkle_chain`: walks the `sig_0` → `sig_5` chain and reports any obvious anomalies (note: full historical reconstruction may require keeping more history or a separate log).
- If `check_contract`: verifies that recent operations respected `allowed_transforms`.
- Returns structured result: `lawful: bool`, `issues: []`, `crs`, `zedos_tag`, `last_update_age_seconds`, `summary`.

**When an agent should call it**:
- On cold boot after long downtime for any block with `crs >= 0.85` or tagged `PRAXIS`.
- Before acting on a critical operational protocol.

### 3.2 `mcp_engram_verify_manifold_integrity`

**Purpose**: Higher-level "am I still sane?" check on the active stalk or whole manifold.

**Input**:
- `scope`: "active_stalk" | "all_stalks" (default active)
- `min_crs`: float (default 0.74)
- `sample_size`: int (default 100) — for performance on very large manifolds
- `include_relation_integrity`: bool

**Behavior**:
- Samples high-value blocks.
- Checks for gross inconsistencies (sudden mass CRS drops, broken recent chains if detectable, etc.).
- Reports overall health score + specific red flags.
- Can be relatively cheap (uses existing indexes + sampling).

**When to call**:
- Mandatory part of a hardened long-sleep wake-up protocol.
- Periodically by background daemons.

### 3.3 `mcp_engram_audit_praxis_contracts` (Future / Stretch)

Specialized audit focused on `ZEDOS_PRAXIS` blocks — the most important "lawful operational protocols."

## 4. Storage / Backend Additions

We need to extend the system so verification logic lives in the right layer.

### 4.1 New methods on `VsaBackend` trait (or a new `VerifiableBackend` supertrait)

```rust
fn get_block_footer(&self, concept: &str) -> Option<LegFooter>;
fn get_allowed_transforms(&self, concept: &str) -> Option<String>;
fn verify_merkle_chain_integrity(&self, concept: &str, depth: usize) -> Result<ChainVerificationResult>;
```

Note: Full historical Merkle verification is hard without storing every previous version of every block. We may start with:
- "Recent chain sanity" (last N updates)
- Reliance on the fact that every update/scar/relation already advances the chain at write time.

### 4.2 `StoreHandle` methods

```rust
pub fn verify_block_lawfulness(&self, concept: &str, options: VerificationOptions) -> Result<LawfulnessReport>;
pub fn verify_manifold_integrity(&self, options: ManifoldVerificationOptions) -> Result<ManifoldHealthReport>;
```

Implementation can start in the `Single` / `Sheaf` / CPU path and be backend-agnostic where possible (most verification is structural, not compute-heavy).

### 4.3 Soft → Hardening Path for Contracts

Current `enforce_contract_soft` only logs. For high-stakes Praxis blocks we may want:
- Configurable strict mode (refuse operations that violate contract).
- Or at minimum, surface violations clearly in verification reports.

## 5. Long-Sleep Wakeup Integration (High Level)

A future hardened wake-up protocol could look like:

```text
SESSION START (after long sleep)
1. mcp_engram_session_start(...)
2. mcp_engram_verify_manifold_integrity(scope="active", min_crs=0.74)
3. For critical Praxis/Genesis blocks:
      mcp_engram_verify_block_lawfulness(concept, deep=true)
4. If any red flags → enter degraded / human-review mode
5. Proceed with normal rehydration + concept mapping reconstruction
```

This makes "verify its lawfulness without calling an external server" a concrete, first-class part of agent operation.

## 6. Open Questions & Risks

- How much history do we need to keep for meaningful chain verification over months/years?
- Performance characteristics on 100k+ block manifolds.
- How to represent "this Praxis block is a verified executable protocol" in a machine-actionable way (future work with item 3).
- Threat model: sophisticated attacker with physical access vs. casual corruption.

## 7. Next Immediate Steps

1. Finalize this spec with feedback.
2. Add `verify_block_lawfulness` and `verify_manifold_integrity` to `StoreHandle` + one backend (start with CPU path).
3. Add the two new MCP tools to `mcp.rs` (tool definition + handler).
4. Wire a minimal version into the wake-up guidance / AGENT_INTEGRATION_GUIDE.
5. Test on real data (the current ~149k block manifold).

---

This document will be the living spec for Item #1 in the roadmap.

Once we have working primitives here, the rest of the vision (long-sleep protocols, Praxis as executable objects, etc.) becomes much more credible.