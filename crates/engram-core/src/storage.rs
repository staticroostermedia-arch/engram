//! Storage layer — read/write `.leg` blocks from NVMe.
//!
//! # File naming
//!
//! Blocks are stored as `<concept>.leg` in the manifold directory.
//! The file is exactly BLOCK_SIZE (256KB), 4096-byte aligned for O_DIRECT I/O.

// Cap'n Proto generated bindings for the ProvLog schema (schema/provlog.capnp).
// Generated at build time by build.rs via capnpc.
pub mod provlog_capnp {
    include!(concat!(env!("OUT_DIR"), "/provlog_capnp.rs"));
}

use crate::types::{HolographicBlock, BLOCK_SIZE};
use std::io::{Read, Write};
use std::path::Path;

#[cfg(target_os = "linux")]
use std::os::unix::fs::OpenOptionsExt;

/// Read a `.leg` block from disk using O_DIRECT (bypasses OS page cache).
///
/// The block is heap-allocated (256KB — never stack) and returned as a Box.
pub fn read_block<P: AsRef<Path>>(path: P) -> std::io::Result<Box<HolographicBlock>> {
    let mut file = {
        let mut opts = std::fs::OpenOptions::new();
        opts.read(true);
        #[cfg(target_os = "linux")]
        opts.custom_flags(libc::O_DIRECT);
        opts.open(path.as_ref())?
    };

    // Heap-allocate — 256KB must never go on the stack
    let mut block: Box<HolographicBlock> = unsafe {
        let layout = std::alloc::Layout::new::<HolographicBlock>();
        let ptr = std::alloc::alloc_zeroed(layout) as *mut HolographicBlock;
        if ptr.is_null() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "Failed to allocate HolographicBlock (256KB)",
            ));
        }
        Box::from_raw(ptr)
    };

    let ptr = block.as_mut() as *mut HolographicBlock as *mut u8;
    let slice = unsafe { std::slice::from_raw_parts_mut(ptr, BLOCK_SIZE) };
    file.read_exact(slice)?;
    Ok(block)
}

/// Write a `.leg` block to disk using O_DIRECT.
///
/// Applies Toryx periodic boundary conditions before writing:
/// `q[8191] = q[0]` and `p[8191] = p[0]`.
pub fn write_block<P: AsRef<Path>>(path: P, block: &HolographicBlock) -> std::io::Result<()> {
    let mut file = {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(target_os = "linux")]
        opts.custom_flags(libc::O_DIRECT);
        opts.open(path.as_ref())?
    };

    // Heap-copy to apply Toryx PBC without modifying the caller's block
    let mut boxed: Box<HolographicBlock> = unsafe {
        let layout = std::alloc::Layout::new::<HolographicBlock>();
        let ptr = std::alloc::alloc_zeroed(layout) as *mut HolographicBlock;
        if ptr.is_null() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "Failed to allocate HolographicBlock for write (256KB)",
            ));
        }
        Box::from_raw(ptr)
    };
    unsafe { std::ptr::copy_nonoverlapping(block, &mut *boxed, 1); }

    // Toryx Periodic Boundary Condition: close the standing spiral
    boxed.q[8191] = boxed.q[0];
    boxed.p[8191] = boxed.p[0];

    let ptr = boxed.as_ref() as *const HolographicBlock as *const u8;
    let slice = unsafe { std::slice::from_raw_parts(ptr, BLOCK_SIZE) };
    file.write_all(slice)?;
    file.sync_all()?;
    Ok(())
}

/// Read the ProvLog text from a block's payload field (Capn Proto).
///
/// Falls back to UTF-8 parsing for legacy blocks without Cap'n Proto framing.
pub fn read_provlog(block: &HolographicBlock) -> String {
    let mut slice = &block.payload[..];
    if let Ok(message) = capnp::serialize::read_message(&mut slice, capnp::message::ReaderOptions::new()) {
        if let Ok(plog) = message.get_root::<provlog_capnp::prov_log::Reader>() {
            if let Ok(text) = plog.get_source_text() {
                if let Ok(string) = text.to_string() {
                    return string;
                }
            }
        }
    }

    // Fallback: raw UTF-8 up to first null (for legacy non-unified stalks prior to the Great Reminting)
    let end = block.payload.iter().position(|&b| b == 0).unwrap_or(block.payload.len());
    String::from_utf8_lossy(&block.payload[..end]).into_owned()
}

/// Write ProvLog text into a block's payload field using Cap'n Proto.
pub fn write_provlog(block: &mut HolographicBlock, text: &str) {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut plog = message.init_root::<provlog_capnp::prov_log::Builder>();
        plog.set_source_text(text);
        plog.set_text_data(());
        
        // Export Thermodynamic factors from Engram directly into the Capnp block schema
        plog.set_ego_coherence(block.energetics.crs);
        
        // Map Praxis logic
        if block.zedos_tag == crate::types::ZEDOS_PRAXIS {
            plog.set_is_praxis(true);
        }
    }

    let mut buf = Vec::new();
    capnp::serialize::write_message(&mut buf, &message).unwrap();
    let len = buf.len().min(block.payload.len());
    
    // Wipe previous trailing bytes to avoid Cap'n Proto parse confusion
    block.payload.fill(0);
    block.payload[..len].copy_from_slice(&buf[..len]);
}

// ── Async I/O prototype (speed-up phase 2, 2026-05-28) ─────────────────────
// Non-breaking wrappers using tokio::spawn_blocking around the existing
// O_DIRECT sync implementations. This moves blocking storage work off the
// MCP / ki_hijacker reactor threads without changing the cold-path contract.
//
// Instrumentation: basic timing logged via tracing so we can measure
// event-loop relief during heavy manifold loads or near compression windows.
//
// Future: true io_uring (tokio-uring or custom reactor) while preserving O_DIRECT + alignment.
// This is the Tier 3 complement to device_residency (cuFile/nvidia-fs direct GPU path).
// Goal: non-blocking direct I/O for the hottest paths, feeding GPU buffers with
// minimal CPU involvement. See handoff micro-plan and helper:async_io_design_v1.
// 
// Small autonomous exploration spike start (post-dual-lens baseline):
// - Prototype a basic io_uring submission for 256KB aligned reads/writes.
// - Preserve Toryx PBC contract on writes (CPU copy for checksums may still be needed).
// - Integrate timing/metrics into the existing async wrappers for dual-lens.
// - Synergy: Use as feeder for device_residency GPU buffers.
// Enabled via the "async-io" feature on engram-core (pulled in by engram-server).

// ── DETAILED DESIGN: io_uring Submission Path (Tier 3 item) ─────────────────
// This block records the start of the small io_uring exploration on the
// storage async wrappers. It is intentionally comment-only (prototype outline,
// no functional code changes yet). All edits strictly limited to this file via
// search_replace per task scope.
//
// GOAL
// Provide a non-blocking, kernel-bypass-friendly async I/O path for .leg
// blocks that is a strict superset of the current spawn_blocking + O_DIRECT
// wrappers. The path must be selectable behind a future "io-uring" feature
// (orthogonal to "async-io").
//
// PRESERVED CONTRACTS (non-negotiable)
// - O_DIRECT on open (lib c::O_DIRECT or equivalent) — no page cache pollution.
// - 4096-byte alignment: both file offsets (always multiples of BLOCK_SIZE in
//   the manifold layout) and user buffers. HolographicBlock is
//   #[repr(C, align(4096))] and the existing heap alloc in read_block/write_block
//   already satisfies kernel requirements via Layout + alloc_zeroed.
// - Toryx Periodic Boundary Condition on ALL writes:
//     q[8191] = q[0];
//     p[8191] = p[0];
//   This logical "closure of the standing spiral" is part of the .leg format
//   contract (see write_block and types.rs). In an io_uring path the PBC step
//   remains a small CPU prep phase (copy into aligned buffer + mutate) because
//   it is data-dependent. The outline below re-uses or mirrors the existing
//   copy logic from write_block so callers continue to see identical on-disk
//   results. No weakening of the PBC invariant is acceptable.
//
// MINIMAL PROTOTYPE OUTLINE (tokio-uring or raw io_uring)
// (Sketched for future implementation; remains disabled / behind cfg.)
//
// Under #[cfg(all(feature = "async-io", feature = "io-uring", target_os = "linux"))]:
//
// use tokio_uring::fs::File;  // or raw io_uring::IoUring + opcode::{Read, Write, Fsync}
//
// pub async fn async_read_block_uring<P>(path: P) -> std::io::Result<Box<HolographicBlock>>
// where
//     P: AsRef<std::path::Path> + Send + 'static,
// {
//     let start = std::time::Instant::now();
//     // 1. Open with O_DIRECT (tokio_uring File::open can take custom flags via
//     //    builder or we fall back to std::fs then into uring).
//     let file = /* open with O_DIRECT */ ...;
//
//     // 2. Allocate the exact aligned target (never stack). Re-use the same
//     //    unsafe Box<HolographicBlock> pattern from read_block for zeroed buffer.
//     let mut block: Box<HolographicBlock> = /* alloc_zeroed + from_raw as today */;
//     let buf: &mut [u8] = /* slice of the 256KB */;
//
//     // 3. Submission (tokio-uring style — true uring sqe under the hood):
//     //    let (res, _buf) = file.read_at(buf, 0).await;  // offset 0 for whole block
//     //    or raw:
//     //    let sqe = io_uring::opcode::Read::new(...).build();
//     //    ring.submit_and_wait(...); let cqe = ring.completion().next()...
//
//     // 4. On success: the buffer now contains the on-disk bytes (post-O_DIRECT).
//     //    No extra copy if we read directly into the Box we will return.
//
//     let elapsed = start.elapsed();
//     // 5. Metrics hook (see below)
//     record_uring_timing("read", elapsed, /*...*/);
//
//     Ok(block)
// }
//
// Write variant:
// - Take ownership or &HolographicBlock
// - Allocate fresh aligned Box (as write_block does today)
// - Apply Toryx PBC on the prep copy: boxed.q[8191] = ... (CPU step unavoidable today)
// - Issue uring Write + Fsync (or separate fsync opcode for full contract fidelity)
// - Completion yields the result; timing captured around prep + submit + cqe.
//
// The raw io_uring path would manage its own IoUring instance (or share via
// a dedicated uring reactor thread) and use IORING_OP_READ/WRITE with
// IOSQE_FIXED_FILE + registered buffers for hot paths.
//
// SYNERGY WITH device_residency (GPU buffers directly)
// - This exploration is the explicit Tier 3 complement / feeder for the
//   "device_residency" feature work in engram-gpu/src/backend.rs
//   (register_hot_item_for_device_residency, fetch_block_device_resident).
// - When both features are active and hardware/drivers allow (cuFile,
//   nvidia-fs, GPUDirect Storage, or io_uring + CUDA memory registration):
//     * Read path: uring completion target can be a GPU pointer (sge.addr
//       pointing into device memory registered for DMA) instead of host
//       HolographicBlock. The returned "block" view becomes a device-resident
//       Leg3Pointer with zero host copy.
//     * Hot items promoted via high_priority_cache / promote_tile_to_high_priority
//       can be staged directly NVMe → GPU with the io_uring (or cuFile) path,
//       realizing "minimal CPU involvement".
//     * Write path symmetry: GPU kernels can produce updated q/p tensors that
//       are written back via direct GPU→NVMe without staging through host RAM
//       (PBC closure step could even be a small device kernel in the future).
// - This closes the loop with the CudaBackend device_resident_buffers map and
//   the handoff micro-plan (cuFile/nvidia-fs + io_uring exploration).
// - Result: the hottest manifold items (high-CRS traces, state blocks, Thought
//   Tiles) become truly device-resident for both compute and storage.
//
// TIMING / METRICS HOOKS COMPATIBLE WITH DUAL-LENS SNAPSHOT HELPERS
// - The existing async_*_block already capture std::time::Instant + tracing
//   debug/info for >50ms slow paths. This provides the baseline.
// - io_uring outline extends it with finer-grained, uring-native hooks:
//     - submit_to_cqe_latency
//     - buffer_prep_latency (incl. PBC copy cost)
//     - bytes_transferred, alignment_verified, direct_gpu_bypass flag
//     - uring_cq_overflow or submission_queue_full events (for health)
// - These are designed to be consumable by:
//     * capture_dual_lens_snapshot (engram-server/src/store.rs) which returns
//       (Leg3Pointer, Duration, f32 CRS)
//     * The [DUAL_LENS_SNAPSHOT] instrumentation in ki_hijacker.rs
//     * Future unified hot-path stats alongside AccessIndex / high_priority_cache
//       hit rates.
// - Implementation note: keep the hooks behind cfg(feature = "io-uring") with
//   no-op / passthrough when the feature is absent so the spawn_blocking
//   timing remains bit-compatible for dual-lens before/after comparisons.
// - Snapshot compatibility: any new metrics can be logged with the same
//   structured format or attached as extra fields to the existing timed
//   fetch paths, allowing quantitative measurement of "event-loop relief"
//   and "CPU bypass %" during compression windows (exactly the dual-lens
//   protocol purpose).
//
// RISKS & FUTURE SPIKE NOTES (for lawful continuation)
// - Runtime integration: tokio-uring brings its own current-thread or
//   multi-thread uring driver; must not conflict with the server's primary
//   tokio runtime used by MCP/ki_hijacker.
// - Buffer lifetime: uring requires buffers to outlive the in-flight SQE;
//   the Box<HolographicBlock> model works naturally if we pin it across the
//   await.
// - Feature gating + Cargo: new optional dep behind "io-uring" feature;
//   no change here (scoped edit).
// - Fallback: any uring path must degrade gracefully to the proven
//   spawn_blocking + read_block/write_block implementation.
// - Only Linux (current O_DIRECT is already linux-gated in the sync paths).
//
// This comment block + outline constitutes the formal autonomous start of the
// Tier 3 io_uring item. It is recorded in the living handoff artifacts and
// will be referenced by future agents via momentum recall and ki_hijacker bakes.
// See: helper:async_io_design_v1, helper:current_arc_status_gpu_item2_phase2_handoff_2026-06,
//      device_residency stubs, dual-lens measurement protocol.

#[cfg(feature = "async-io")]
pub async fn async_read_block<P: AsRef<std::path::Path> + Send + 'static>(
    path: P,
) -> std::io::Result<Box<HolographicBlock>> {
    let start = std::time::Instant::now();
    let path_clone = path.as_ref().to_path_buf();
    let inner = tokio::task::spawn_blocking(move || read_block(path_clone))
        .await
        .map_err(std::io::Error::other)??;
    let elapsed = start.elapsed();
    tracing::debug!("[async-io] read_block took {:?} (path: {:?})", elapsed, path.as_ref());
    // Basic instrumentation hook for future metrics (e.g., hot vs cold, compression windows)
    if elapsed > std::time::Duration::from_millis(50) {
        tracing::info!("[async-io] slow read_block {:?} for path {:?}", elapsed, path.as_ref());
    }
    Ok(inner)
}

#[cfg(feature = "async-io")]
pub async fn async_write_block<P: AsRef<std::path::Path> + Send + 'static>(
    path: P,
    block: HolographicBlock,
) -> std::io::Result<()> {
    let start = std::time::Instant::now();
    let path_clone = path.as_ref().to_path_buf();
    tokio::task::spawn_blocking(move || write_block(path_clone, &block))
        .await
        .map_err(std::io::Error::other)??;
    let elapsed = start.elapsed();
    tracing::debug!("[async-io] write_block took {:?} (path: {:?})", elapsed, path.as_ref());
    if elapsed > std::time::Duration::from_millis(50) {
        tracing::info!("[async-io] slow write_block {:?} for path {:?}", elapsed, path.as_ref());
    }
    Ok(())
}

// Usage note: These functions are gated behind the "async-io" feature.
// They are exercised from ki_hijacker for hot-path measurement.
// The O_DIRECT + Toryx PBC contract is fully preserved. The feature is
// enabled automatically when depending on engram-core from engram-server.
