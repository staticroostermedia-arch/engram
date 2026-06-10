# Phase C – Buffer Companion Patch Artifact

**Source**: Extracted from `conv_task_elevate_praxis_to_operational_protocols.md` (Phase C Execution – Buffer Companion Sketch)
**Target**: `/path/to/Documents/CodeLand/crates/monad_praxis/src/buffer.rs`
**Date**: 2026-05-26
**Status**: Ready to apply (review + manual edit)

## Context & Grounding
This is the critical runtime visibility companion to the P1 creation hook. Without it, even perfectly formed public executable protocol blocks created by the architect (or future NREM promotion) will be invisible to the PraxisBuffer, session context injection, utility scoring, affirmation tracking, and NREM eligibility decisions.

The changes are deliberately minimal and zero-cost for ordinary (non-protocol) PRAXIS entries.

## Patch Summary

### 1. Extend `PraxisEntry` struct

Add the following fields after the existing ones (around line 38):

```rust
    /// --- Protocol / Executable Protocol metadata (Item 3 alignment) ---
    /// Populated only for blocks that carry the public executable protocol markers.
    /// Zero-cost (Option / bool) when absent — no overhead for ordinary PRAXIS entries.
    pub is_public_executable_protocol: bool,
    pub protocol_type: Option<u8>,
    pub dispatch_key: Option<String>,
    pub protocol_header_present: bool,
```

Update the `recompute_utility` impl if needed (no change required for v0.1).

### 2. Update `load_praxis_entry` (the key function at ~lines 373-399)

Insert protocol detection logic right after the ZEDOS_PRAXIS tag check and before constructing the `PraxisEntry`.

Replace the `Some(PraxisEntry { ... })` construction with the extended version.

### 3. Add convenience method (optional but recommended)

```rust
impl PraxisEntry {
    pub fn as_executable_protocol(&self) -> Option<(u8, &str)> {
        if self.is_public_executable_protocol {
            self.protocol_type.zip(self.dispatch_key.as_deref())
        } else {
            None
        }
    }
}
```

## Detailed Diff (Unified)

```diff
diff --git a/crates/monad_praxis/src/buffer.rs b/crates/monad_praxis/src/buffer.rs
index 9876543..3210cba 100644
--- a/crates/monad_praxis/src/buffer.rs
+++ b/crates/monad_praxis/src/buffer.rs
@@ -29,6 +29,15 @@ pub struct PraxisEntry {
     pub crs: f32,
     /// Flagged for re-synthesis by M-NOL
     pub needs_resynthesis: bool,
+
+    // --- Item 3: Public Executable Protocol visibility (Phase C) ---
+    pub is_public_executable_protocol: bool,
+    pub protocol_type: Option<u8>,
+    pub dispatch_key: Option<String>,
+    pub protocol_header_present: bool,
 }

 impl PraxisEntry {
@@ -373,6 +382,30 @@ fn load_praxis_entry(path: &Path) -> Option<PraxisEntry> {
     if block.zedos_tag != monad_ledger::structs::ZEDOS_PRAXIS {
         return None;
     }
+
+    // --- Protocol detection (lightweight, only runs for PRAXIS blocks) ---
+    let mut is_pub_proto = false;
+    let mut proto_type = None;
+    let mut disp_key = None;
+    let mut header_present = false;
+
+    let transforms = std::str::from_utf8(&block.allowed_transforms)
+        .unwrap_or("")
+        .trim_end_matches('\0');
+
+    if transforms.contains("execute") && block.payload[0] == 0x01 {
+        header_present = true;
+        is_pub_proto = true;
+        proto_type = Some(block.payload[1]);
+
+        let key_end = block.payload[8..32].iter().position(|&b| b == 0).unwrap_or(24);
+        if let Ok(k) = std::str::from_utf8(&block.payload[8..8+key_end]) {
+            disp_key = Some(k.to_string());
+        }
+    }

     // Extract provlog text (existing logic — header is skipped naturally for protocol blocks
     // because we start at first 0; future refinement can start after 32 when header_present)
@@ -389,6 +422,11 @@ fn load_praxis_entry(path: &Path) -> Option<PraxisEntry> {
         use_count:         0,
         crs:               block.crs_score,
         needs_resynthesis: false,
+
+        is_public_executable_protocol: is_pub_proto,
+        protocol_type: proto_type,
+        dispatch_key: disp_key,
+        protocol_header_present: header_present,
     })
 }
```

## Integration Notes
- This change makes every block minted via the P1 `mint_praxis_block_as_public_executable_protocol` helper (or the public `remember_protocol` surface) immediately visible inside `PraxisBuffer::vram_tier` / `ram_tier`.
- Consumers of the buffer (session context builder, `record_affirm`, rebalance, NREM eligibility) can now call `.as_executable_protocol()` to decide whether to surface the block as an invocable protocol.
- No behavior change for the ~149k existing blocks that do not carry the "execute" token + v0.1 header.

## Recommended Follow-up
1. Apply this patch.
2. Add a small test or logging line that prints protocol blocks when they appear in the buffer.
3. Wire a basic "executable protocol" injection path into the session context (future expansion item).

**Full design rationale and invariant analysis**: See `conv_task_elevate_praxis_to_operational_protocols.md` (Phase C Buffer Companion section).

**Companion artifact**: `phase_c_p1_architect_patch.md`
