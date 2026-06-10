# Phase C – P1 Patch Artifact: mint_praxis_block_as_public_executable_protocol

**Source**: Extracted from `conv_task_elevate_praxis_to_operational_protocols.md` (Phase C Execution – P1 Sketch)
**Target**: `/path/to/Documents/CodeLand/crates/monad_praxis/src/architect.rs`
**Date**: 2026-05-26
**Status**: Ready to apply (review + `git apply` or manual)

## Context & Grounding
This patch implements the highest-priority recommendation from the deep vehicle alignment work. It gives the CodeLand synthesis path a clean, opt-in way to produce Praxis blocks that are immediately usable as executable protocols on the public Engram surface (via the 7-point gate and `mcp_engram_invoke_protocol`).

It respects all 8 Non-Negotiable Invariants from `conv_task_leg_primitive_functional_audit.md`.

## Recommended Application
1. Review this file.
2. Apply the helper function in `monad_praxis/src/architect.rs` near the existing `mint_praxis_block`.
3. Add a call site in `commit_praxis_draft` or a high-value synthesis path when the block meets "public executable" criteria (high CRS + rich ProvLog).
4. Later: wire the buffer companion patch so these blocks become visible at runtime.

## Patch (Unified Diff Style)

```diff
diff --git a/crates/monad_praxis/src/architect.rs b/crates/monad_praxis/src/architect.rs
index abc1234..def5678 100644
--- a/crates/monad_praxis/src/architect.rs
+++ b/crates/monad_praxis/src/architect.rs
@@ -543,6 +543,80 @@ impl Architect {
         Ok(())
     }

+    /// Mint a Praxis block that is immediately usable as an executable protocol
+    /// on the public Engram surface (via mcp_engram_invoke_protocol + 7-point gate).
+    ///
+    /// Sets the minimum required contract token ("evidence_update,execute" plus any
+    /// richer deep vocabulary) and prefixes a canonical 32-byte ProtocolHeader (v0.1)
+    /// at the front of the payload, followed by the human ProvLog text.
+    ///
+    /// This is an *additive* path. Existing call sites to mint_praxis_block are untouched.
+    /// Use this variant (or a criteria-driven internal call) when the synthesized content
+    /// is high-CRS, ProvLog-rich, and intended for operational use / long-sleep verification.
+    pub fn mint_praxis_block_as_public_executable_protocol(
+        &self,
+        path: &PathBuf,
+        centroid_q: &[Complex32; DIM],
+        human_provlog: &str,
+        steward_alignment: f32,
+        mean_crs: f32,
+        source_paths: &[PathBuf],
+        protocol_type: u8,             // e.g. 0x01 Decision Procedure
+        dispatch_key: &str,            // Stable short name, e.g. "daily_consolidation_check"
+    ) -> Result<()> {
+        let mut block = Leg3Pointer::mint();
+
+        block.q.copy_from_slice(centroid_q);
+        block.zedos_tag     = ZEDOS_PRAXIS;
+        block.context_state = CONTEXT_STATE_PRAXIS;
+        block.magic         = *b"LEG3";
+        block.spin_state    = 0x01;
+        block.crs_score     = mean_crs.max(0.74);
+        block.decay_factor  = steward_alignment;
+        block.energetics.heat_dissipated = LAW_CONSTANT;
+        block.energetics.crs             = mean_crs.max(0.74);
+        block.energetics.work_verb       = 1.0;
+
+        // Public + Deep Contract
+        let deep = b"evolve,merge,synthesize,demote";
+        let public_exec = b",execute";
+        let mut transforms = [0u8; 64];
+        let mut len = 0;
+        transforms[..deep.len()].copy_from_slice(deep);
+        len += deep.len();
+        if !transforms[..len].windows(public_exec.len()).any(|w| w == public_exec) {
+            transforms[len..len+public_exec.len()].copy_from_slice(public_exec);
+            len += public_exec.len();
+        }
+        block.allowed_transforms[..len].copy_from_slice(&transforms[..len]);
+
+        // Payload: ProtocolHeader (32) + ProvLog text
+        let mut header = [0u8; 32];
+        header[0] = 0x01;                    // version
+        header[1] = protocol_type;
+        header[2] = 0x00;                    // flags
+        let key_bytes = dispatch_key.as_bytes();
+        let key_len = key_bytes.len().min(23);
+        header[8..8+key_len].copy_from_slice(&key_bytes[..key_len]);
+        header[8+key_len] = 0;
+
+        let provlog_bytes = human_provlog.as_bytes();
+        let max_prov = block.payload.len() - 32;
+        let prov_len = provlog_bytes.len().min(max_prov);
+
+        block.payload[..32].copy_from_slice(&header);
+        block.payload[32..32+prov_len].copy_from_slice(&provlog_bytes[..prov_len]);
+
+        // Lineage, sig chain, timestamp (identical to original mint_praxis_block)
+        let path_list = source_paths.iter()
+            .map(|p| p.to_string_lossy())
+            .collect::<Vec<_>>()
+            .join("|");
+        let lineage_hash = blake3::hash(path_list.as_bytes());
+        block.concept_ref.copy_from_slice(lineage_hash.as_bytes());
+
+        let q_bytes = unsafe {
+            std::slice::from_raw_parts(
+                block.q.as_ptr() as *const u8,
+                DIM * std::mem::size_of::<Complex32>(),
+            )
+        };
+        let q_hash = blake3::hash(q_bytes);
+        block.footer.sig_0.copy_from_slice(q_hash.as_bytes());
+        block.footer.sig_1 = block.footer.sig_0;
+        block.last_accessed_timestamp = unix_now();
+
+        monad_storage::staging::direct_write_block(
+            path.to_str().unwrap_or(""),
+            &*block,
+        ).map_err(|e| anyhow!("direct_write_block failed: {}", e))?;
+
+        Ok(())
+    }
+
     /// Read drafting files back into memory, and execute the final minting sequence.
     pub fn commit_praxis_draft(&mut self, md_path_str: &str) -> Result<PathBuf> {
```

## Usage Example (to be added near a synthesis call site)

```rust
// When criteria are met for a public executable protocol
self.mint_praxis_block_as_public_executable_protocol(
    &target_path,
    &centroid_q,
    &rich_human_explanation,
    steward_alignment,
    mean_crs,
    &source_paths,
    0x01,  // Decision Procedure
    "daily_praxis_consolidation_v1",
)?;
```

## Next Integration Step
Apply together with the buffer companion patch so that blocks created with this helper are visible in `PraxisBuffer` and eligible for session context / NREM.

**References**:
- Full context and invariant analysis: `conv_task_elevate_praxis_to_operational_protocols.md` (Phase C section)
- Public ProtocolHeader spec: `praxis_as_protocol_spec.md`
