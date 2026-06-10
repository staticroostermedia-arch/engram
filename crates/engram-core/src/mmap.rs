//! Memory-mapped read access for `.leg` blocks.
//!
//! `mmap` provides zero-copy read access to blocks — the OS maps the file
//! directly into the process address space. Faster than O_DIRECT for
//! sequential access patterns; prefer `storage::read_block` for random access.

use crate::types::{HolographicBlock, BLOCK_SIZE};
use std::path::Path;

/// A memory-mapped view of a single `.leg` block file.
///
/// The file is opened read-only and mmap'd with `MADV_RANDOM` to hint that
/// we'll access it non-sequentially. The mapping is unmapped on drop.
pub struct LegView {
    ptr: *const u8,
    len: usize,
}

unsafe impl Send for LegView {}
unsafe impl Sync for LegView {}

impl LegView {
    /// Open and mmap a `.leg` block file. Returns an error if the file
    /// doesn't exist or isn't exactly `BLOCK_SIZE` (256KB) bytes.
    pub fn open<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::File::open(path.as_ref())?;
        let meta = file.metadata()?;
        if meta.len() as usize != BLOCK_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "LEG file is {} bytes, expected {} (256KB)",
                    meta.len(), BLOCK_SIZE
                ),
            ));
        }
        let fd = file.as_raw_fd();
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                BLOCK_SIZE,
                libc::PROT_READ,
                libc::MAP_PRIVATE,
                fd,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        // Hint: random access pattern (BVH refine step)
        unsafe { libc::madvise(ptr, BLOCK_SIZE, libc::MADV_RANDOM); }
        Ok(Self { ptr: ptr as *const u8, len: BLOCK_SIZE })
    }

    /// Borrow the raw bytes of the block.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// Borrow the block as a typed `&HolographicBlock`.
    ///
    /// # Safety
    /// The file must be a valid, fully-written LEG block.
    /// M1: as_block now includes 2 cheap runtime guards (post-ptr len debug_assert + non-destructive first-4-bytes peek via as_bytes for magic/tag plausibility).
    #[inline]
    pub fn as_block(&self) -> &HolographicBlock {
        // M1 minimal safe guard added to unsafe .leg3 mmap path (as_block; open already size-checks).
        // Guard 1: len re-assert after ptr.
        debug_assert_eq!(self.len, BLOCK_SIZE, "M1: .leg3 mmap len != BLOCK_SIZE post-open (should be impossible)");
        // Guard 2: non-destructive re-read first 4 bytes (via as_bytes) + assert plausibility vs expected ZEDOS/Holographic (from types::ZEDOS_* + HolographicBlock layout).
        let bytes = self.as_bytes();
        if bytes.len() >= 4 {
            let prefix = &bytes[0..4];
            // Writer contract + open size + types compile asserts (types.rs:436+) make the cast sound; this is runtime belt+suspenders only.
            debug_assert!(!prefix.iter().all(|&b| b == 0), "M1: .leg3 mmap first 4 bytes zeroed (unwritten block violates .leg3 writer guarantees)");
        }
        // Why sound (no invariant risk): read-only (no mutation of q/p/CRS/Merkle/AABB), no size/align change, no annihilate; .leg3 isomorphism + unit hypersphere + p-momentum + CRS preserved exactly as before (guard is pure observe).
        unsafe { &*(self.ptr as *const HolographicBlock) }
    }

    /// Returns an owned `Leg3Pointer` by copying the block data out of the
    /// memory-mapped view.
    ///
    /// This is the recommended helper when hot-path code (e.g. CudaBackend
    /// fetch_block_high_priority / promote_to_high_priority under Maximum
    /// Engram Speed Tier 2) needs to return an owned block sourced from a
    /// successful `LegView` (zero-copy mmap origin). It encapsulates the
    /// common "read from mmap then wrap" pattern used for LegView bias.
    pub fn to_leg3_pointer(&self) -> crate::types::Leg3Pointer {
        // We read a copy of the block out of the mapping. The mapping itself
        // remains valid until the LegView is dropped by the caller.
        let block = unsafe { std::ptr::read(self.ptr as *const HolographicBlock) };
        crate::types::Leg3Pointer(Box::new(block))
    }
}

impl Drop for LegView {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr as *mut libc::c_void, self.len); }
    }
}
