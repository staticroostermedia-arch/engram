//! Single-flight guard for background / on-demand BVH builds.
//!
//! Without dedup, repeated `rebuild_bvh_async` calls clear the index and spawn
//! another full manifold scan — on large stores this stacks 100+ threads and
//! prevents `bvh_ready` from ever committing.

use crate::bvh::BvhManifold;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// Ensures at most one BVH build thread runs per backend instance.
#[derive(Debug)]
pub struct BvhBuildCoordinator {
    in_flight: AtomicBool,
}

impl BvhBuildCoordinator {
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self {
            in_flight: AtomicBool::new(false),
        })
    }

    /// Returns true if the caller won the race and should spawn the build thread.
    pub fn try_start(&self) -> bool {
        self.in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn finish(&self) {
        self.in_flight.store(false, Ordering::Release);
    }

    pub fn is_building(&self) -> bool {
        self.in_flight.load(Ordering::Acquire)
    }
}

/// Spawn one background LBVH build. No-op if a build is already in flight.
pub fn spawn_bvh_build<P: AsRef<Path>>(
    thread_name: &str,
    label: &str,
    path: P,
    bvh: Arc<RwLock<Option<BvhManifold>>>,
    coord: Arc<BvhBuildCoordinator>,
) -> bool {
    if !coord.try_start() {
        eprintln!("[BVH] {label} build skipped — already in progress");
        return false;
    }

    let path = path.as_ref().to_path_buf();
    let name = thread_name.to_string();
    let label = label.to_string();
    let label_err = label.clone();

    let coord_thread = Arc::clone(&coord);
    match std::thread::Builder::new().name(name).spawn(move || {
        let t0 = std::time::Instant::now();
        eprintln!("[BVH] {label} build started…");
        let new_bvh = BvhManifold::build_from_dir(&path);
        if let Ok(mut guard) = bvh.write() {
            let n = new_bvh.as_ref().map_or(0, |b| b.len());
            *guard = new_bvh;
            eprintln!(
                "[BVH] ✓ {label} build complete: {n} concepts in {:.1}s",
                t0.elapsed().as_secs_f32()
            );
        }
        coord_thread.finish();
    }) {
        Ok(_) => true,
        Err(e) => {
            eprintln!("[BVH] {label_err} build thread spawn failed: {e}");
            coord.finish();
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_start_allows_only_one_in_flight() {
        let coord = BvhBuildCoordinator::new_shared();
        assert!(coord.try_start());
        assert!(!coord.try_start());
        coord.finish();
        assert!(coord.try_start());
    }
}
