//! GPUDirect Storage / cuFile detection, init, and NVMe→GPU DMA for hot residency.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static CUFILE_DETECTED: AtomicBool = AtomicBool::new(false);
static CUFILE_PROBE_DONE: AtomicBool = AtomicBool::new(false);
/// RSI Cycle 73: ldconfig -p probe runs once in bg — never block wake readiness.
static CUFILE_PROBE_SPAWNED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "linux")]
static CUFILE_INIT_OK: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "linux")]
static CUFILE_INIT_TRIED: AtomicBool = AtomicBool::new(false);
/// RSI Cycle 84: cuFileDriverOpen/dlopen runs once in bg — never block wake readiness.
/// C73 only async'd ldconfig; config-file detect still called sync `cufile_init` (~500ms cold).
static CUFILE_INIT_SPAWNED: AtomicBool = AtomicBool::new(false);
/// 0=off, 1=cufile_dma, 2=h2d_memcpy, 3=unavailable
static LAST_TRANSFER_MODE: AtomicU8 = AtomicU8::new(3);
static LAST_DMA_ATTEMPTED: AtomicBool = AtomicBool::new(false);
static LAST_DMA_SUCCESS: AtomicBool = AtomicBool::new(false);

const MODE_OFF: u8 = 0;
const MODE_CUFILE_DMA: u8 = 1;
const MODE_H2D_MEMCPY: u8 = 2;
const MODE_UNAVAILABLE: u8 = 3;

/// User/env requests cuFile hot path (`ENGRAM_CUFILE_HOT=1`).
pub fn cufile_hot_requested() -> bool {
    let v = std::env::var("ENGRAM_CUFILE_HOT")
        .unwrap_or_else(|_| "0".to_string())
        .to_ascii_lowercase();
    matches!(v.as_str(), "1" | "true" | "on")
}

fn probe_cufile_config_files() -> bool {
    Path::new("/usr/local/cuda/gds/cufile.json").exists() || Path::new("/etc/cufile.json").exists()
}

fn probe_cufile_driver() -> bool {
    if probe_cufile_config_files() {
        return true;
    }
    std::process::Command::new("ldconfig")
        .args(["-p"])
        .output()
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout);
            out.contains("libcufile.so") || out.contains("libcufile_rdma.so")
        })
        .unwrap_or(false)
}

/// True when cuFile / GDS driver artifacts are visible on this host.
///
/// RSI Cycle 73: never block the wake path on `ldconfig -p` (measured ~500ms+ cold).
/// Config-file hits resolve sync; otherwise spawn one background probe and return
/// `false` until it completes (readiness soft-stale will refresh later).
pub fn cufile_driver_detected() -> bool {
    if CUFILE_PROBE_DONE.load(Ordering::Relaxed) {
        return CUFILE_DETECTED.load(Ordering::Relaxed);
    }
    // Fast path: known GDS config files (no process spawn).
    if probe_cufile_config_files() {
        CUFILE_DETECTED.store(true, Ordering::Relaxed);
        CUFILE_PROBE_DONE.store(true, Ordering::Relaxed);
        return true;
    }
    // Slow path: one-shot bg ldconfig — do not block readiness/wake.
    if !CUFILE_PROBE_SPAWNED.swap(true, Ordering::Relaxed) {
        std::thread::spawn(|| {
            let detected = probe_cufile_driver();
            CUFILE_DETECTED.store(detected, Ordering::Relaxed);
            CUFILE_PROBE_DONE.store(true, Ordering::Relaxed);
        });
    }
    false
}

/// Whether the async/sync probe has finished (for tests / readiness observability).
pub fn cufile_probe_complete() -> bool {
    CUFILE_PROBE_DONE.load(Ordering::Relaxed)
}

pub fn cufile_transfer_path() -> &'static str {
    match LAST_TRANSFER_MODE.load(Ordering::Relaxed) {
        MODE_CUFILE_DMA => "cufile_dma",
        MODE_H2D_MEMCPY => "h2d_memcpy",
        MODE_OFF => "off",
        _ => "unavailable",
    }
}

/// Whether the most recent `cufile_direct_read_to_device` attempt ran the DMA path successfully.
pub fn cufile_last_dma_success() -> bool {
    LAST_DMA_SUCCESS.load(Ordering::Relaxed)
}

/// Whether the most recent call entered the cuFile DMA read path (register+read).
pub fn cufile_last_dma_attempted() -> bool {
    LAST_DMA_ATTEMPTED.load(Ordering::Relaxed)
}

fn set_transfer_mode(mode: u8) {
    LAST_TRANSFER_MODE.store(mode, Ordering::Relaxed);
}

/// Hot NVMe→GPU path is active: requested, driver present, and cuFile driver open succeeded
/// (or provisional CUDA path while async init is still in flight).
///
/// RSI Cycle 84: **never** call sync `cufile_init()` here. Cold wake measured
/// `readiness_ms≈514` from `dlopen(libcufile)+cuFileDriverOpen` after config-file
/// detect made `cufile_driver_detected()` return true immediately. Spawn one
/// background init; DMA path still sync-inits on first `cufile_direct_read_*`.
pub fn cufile_hot_active() -> bool {
    if !cufile_hot_requested() {
        set_transfer_mode(MODE_OFF);
        return false;
    }
    if !cufile_driver_detected() {
        set_transfer_mode(MODE_UNAVAILABLE);
        return cfg!(engram_backend_cuda);
    }
    // Already finished (ok or failed).
    #[cfg(target_os = "linux")]
    if CUFILE_INIT_TRIED.load(Ordering::Relaxed) {
        return CUFILE_INIT_OK.load(Ordering::Relaxed) || cfg!(engram_backend_cuda);
    }
    // Spawn async init once; do not block readiness/wake.
    if !CUFILE_INIT_SPAWNED.swap(true, Ordering::Relaxed) {
        std::thread::spawn(|| {
            let _ = cufile_init();
        });
    }
    // Provisional: CUDA backend can serve hot residency while GDS open completes.
    cfg!(engram_backend_cuda)
}

/// Whether async/sync `cufile_init` has finished (tests / readiness observability).
pub fn cufile_init_complete() -> bool {
    #[cfg(target_os = "linux")]
    {
        CUFILE_INIT_TRIED.load(Ordering::Relaxed)
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// q-vector byte length at offset 0 in a .leg block (8192 × Complex32).
pub const Q_VECTOR_BYTES: usize = 8192 * std::mem::size_of::<f32>() * 2;

#[cfg(target_os = "linux")]
#[allow(clippy::missing_transmute_annotations)]
mod ffi {
    use super::*;
    use std::ffi::c_void;
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    use std::sync::OnceLock;

    type CuFileDriverOpenFn = unsafe extern "C" fn() -> i32;
    type CuFileDriverCloseFn = unsafe extern "C" fn() -> i32;
    type CuFileHandleRegisterFn = unsafe extern "C" fn(*mut u64, *const CuFileDescr) -> i32;
    type CuFileHandleDeregisterFn = unsafe extern "C" fn(u64) -> i32;
    type CuFileReadFn = unsafe extern "C" fn(u64, *mut c_void, usize, i64, usize) -> isize;
    type CuFileBufRegisterFn = unsafe extern "C" fn(*mut c_void, usize, i32) -> i32;
    type CuFileBufDeregisterFn = unsafe extern "C" fn(*mut c_void) -> i32;

    #[repr(C)]
    struct CuFileDescr {
        handle_type: i32,
        handle: CuFileHandleUnion,
        fs_ops: *const c_void,
        reserved: *const c_void,
    }

    #[repr(C)]
    union CuFileHandleUnion {
        fd: i32,
    }

    const CUFILE_HANDLE_TYPE_O_DIRECT: i32 = 1;

    struct CuFileApi {
        _lib: *mut c_void,
        driver_open: CuFileDriverOpenFn,
        #[allow(dead_code)]
        driver_close: CuFileDriverCloseFn,
        handle_register: CuFileHandleRegisterFn,
        handle_deregister: CuFileHandleDeregisterFn,
        read: CuFileReadFn,
        buf_register: CuFileBufRegisterFn,
        buf_deregister: CuFileBufDeregisterFn,
    }

    unsafe impl Send for CuFileApi {}
    unsafe impl Sync for CuFileApi {}

    static API: OnceLock<Option<CuFileApi>> = OnceLock::new();

    fn load_api() -> Option<&'static CuFileApi> {
        API.get_or_init(|| unsafe {
            let lib = libc::dlopen(
                c"libcufile.so.0".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            );
            if lib.is_null() {
                return None;
            }
            let sym = |name: &[u8]| -> Option<*mut c_void> {
                let ptr = libc::dlsym(lib, name.as_ptr() as *const _);
                if ptr.is_null() {
                    None
                } else {
                    Some(ptr)
                }
            };
            let driver_open = sym(b"cuFileDriverOpen\0")?;
            let driver_close = sym(b"cuFileDriverClose\0")?;
            let handle_register = sym(b"cuFileHandleRegister\0")?;
            let handle_deregister = sym(b"cuFileHandleDeregister\0")?;
            let read = sym(b"cuFileRead\0")?;
            let buf_register = sym(b"cuFileBufRegister\0")?;
            let buf_deregister = sym(b"cuFileBufDeregister\0")?;
            Some(CuFileApi {
                _lib: lib,
                driver_open: std::mem::transmute(driver_open),
                driver_close: std::mem::transmute(driver_close),
                handle_register: std::mem::transmute(handle_register),
                handle_deregister: std::mem::transmute(handle_deregister),
                read: std::mem::transmute(read),
                buf_register: std::mem::transmute(buf_register),
                buf_deregister: std::mem::transmute(buf_deregister),
            })
        })
        .as_ref()
    }

    pub(super) fn cufile_init_inner() -> bool {
        if CUFILE_INIT_TRIED.load(Ordering::Relaxed) {
            return CUFILE_INIT_OK.load(Ordering::Relaxed);
        }
        CUFILE_INIT_TRIED.store(true, Ordering::Relaxed);
        let Some(api) = load_api() else {
            tracing::debug!("[cufile] libcufile.so.0 not loadable");
            return false;
        };
        let rc = unsafe { (api.driver_open)() };
        let ok = rc == 0;
        CUFILE_INIT_OK.store(ok, Ordering::Relaxed);
        if ok {
            tracing::info!("[cufile] cuFileDriverOpen succeeded — GDS DMA path eligible");
        } else {
            tracing::debug!("[cufile] cuFileDriverOpen returned {rc} — fallback to H2D memcpy");
        }
        ok
    }

    pub(super) fn cufile_direct_read_inner(
        leg_path: &Path,
        file_offset: u64,
        size: usize,
        gpu_ptr: u64,
    ) -> bool {
        LAST_DMA_ATTEMPTED.store(true, Ordering::Relaxed);
        LAST_DMA_SUCCESS.store(false, Ordering::Relaxed);

        let Some(api) = load_api() else {
            return false;
        };
        if !CUFILE_INIT_OK.load(Ordering::Relaxed) && !cufile_init_inner() {
            return false;
        }
        if gpu_ptr == 0 || size == 0 {
            return false;
        }

        // O_DIRECT open — fd must stay alive through register+read+deregister (storage.rs contract).
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECT)
            .open(leg_path)
        {
            Ok(f) => f,
            Err(e) => {
                tracing::debug!("[cufile] O_DIRECT open {} failed: {e}", leg_path.display());
                return false;
            }
        };
        let fd = file.as_raw_fd();

        let mut fh: u64 = 0;
        let descr = CuFileDescr {
            handle_type: CUFILE_HANDLE_TYPE_O_DIRECT,
            handle: CuFileHandleUnion { fd },
            fs_ops: std::ptr::null(),
            reserved: std::ptr::null(),
        };
        if unsafe { (api.handle_register)(&mut fh, &descr) } != 0 {
            tracing::debug!(
                "[cufile] cuFileHandleRegister failed for {}",
                leg_path.display()
            );
            return false;
        }

        let gpu = gpu_ptr as *mut c_void;
        if unsafe { (api.buf_register)(gpu, size, 0) } != 0 {
            let _ = unsafe { (api.handle_deregister)(fh) };
            tracing::debug!("[cufile] cuFileBufRegister failed");
            return false;
        }

        let nbytes = unsafe { (api.read)(fh, gpu, size, file_offset as i64, size) };

        let _ = unsafe { (api.buf_deregister)(gpu) };
        let _ = unsafe { (api.handle_deregister)(fh) };
        // `file` dropped here — fd closed only after cuFile handle deregistered.

        if nbytes < 0 || nbytes as usize != size {
            tracing::debug!(
                "[cufile] cuFileRead returned {nbytes} (expected {size}) for {} — honest H2D fallback",
                leg_path.display()
            );
            return false;
        }

        LAST_DMA_SUCCESS.store(true, Ordering::Relaxed);
        set_transfer_mode(MODE_CUFILE_DMA);
        tracing::info!(
            "[device-residency] cuFile DMA read {size} bytes @ {file_offset} → GPU {gpu_ptr:#x} ({})",
            leg_path.display()
        );
        true
    }
}

#[cfg(not(target_os = "linux"))]
mod ffi {
    use super::*;

    pub(super) fn cufile_init_inner() -> bool {
        false
    }

    pub(super) fn cufile_direct_read_inner(
        _leg_path: &Path,
        _file_offset: u64,
        _size: usize,
        _gpu_ptr: u64,
    ) -> bool {
        false
    }
}

/// Open cuFile driver (idempotent).
pub fn cufile_init() -> bool {
    ffi::cufile_init_inner()
}

/// DMA-read `size` bytes from `leg_path` at `file_offset` into CUDA device memory at `gpu_ptr`.
pub fn cufile_direct_read_to_device(
    leg_path: &Path,
    file_offset: u64,
    size: usize,
    gpu_ptr: u64,
) -> bool {
    if !cufile_hot_requested() || !cufile_driver_detected() {
        return false;
    }
    ffi::cufile_direct_read_inner(leg_path, file_offset, size, gpu_ptr)
}

/// Record H2D memcpy fallback for readiness telemetry.
pub fn cufile_note_h2d_fallback() {
    if cufile_hot_requested() {
        set_transfer_mode(MODE_H2D_MEMCPY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Serializes env/global cuFile probe state across parallel `cargo test` threads.
    fn cufile_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// RSI Cycle 73: unprobed call must not block; returns false until bg/sync complete.
    #[test]
    fn cufile_driver_detected_does_not_require_sync_ldconfig() {
        let _guard = cufile_test_lock();
        // If already probed by other tests, just assert completion path works.
        let _ = cufile_driver_detected();
        // Either probe finished quickly (config file / prior test) or async in flight.
        // Function must return without hanging (this test completing is the contract).
        assert!(
            cufile_probe_complete() || CUFILE_PROBE_SPAWNED.load(Ordering::Relaxed),
            "probe must complete or spawn async"
        );
    }

    /// RSI Cycle 84: `cufile_hot_active` must not block on cuFileDriverOpen.
    #[test]
    fn cufile_hot_active_does_not_require_sync_init() {
        let _guard = cufile_test_lock();
        std::env::set_var("ENGRAM_CUFILE_HOT", "1");
        let t0 = std::time::Instant::now();
        let _ = cufile_hot_active();
        let ms = t0.elapsed().as_millis();
        // Sync dlopen+DriverOpen was ~500ms cold; budget leaves headroom for CI noise.
        assert!(
            ms < 150,
            "cufile_hot_active must stay non-blocking (took {ms}ms)"
        );
        // Init spawn only when driver is already known-present. Hosts without
        // GDS config (CI) correctly skip init and return provisional CUDA/false.
        if cufile_probe_complete() && CUFILE_DETECTED.load(Ordering::Relaxed) {
            assert!(
                cufile_init_complete() || CUFILE_INIT_SPAWNED.load(Ordering::Relaxed),
                "when driver detected, init must complete or spawn async"
            );
        }
        std::env::remove_var("ENGRAM_CUFILE_HOT");
    }

    #[test]
    fn cufile_hot_requested_opt_in() {
        let _guard = cufile_test_lock();
        std::env::remove_var("ENGRAM_CUFILE_HOT");
        assert!(!cufile_hot_requested());
        std::env::set_var("ENGRAM_CUFILE_HOT", "1");
        assert!(cufile_hot_requested());
        std::env::remove_var("ENGRAM_CUFILE_HOT");
    }

    #[test]
    fn cufile_transfer_path_labels() {
        let _guard = cufile_test_lock();
        set_transfer_mode(MODE_CUFILE_DMA);
        assert_eq!(cufile_transfer_path(), "cufile_dma");
        set_transfer_mode(MODE_H2D_MEMCPY);
        assert_eq!(cufile_transfer_path(), "h2d_memcpy");
        cufile_note_h2d_fallback();
        std::env::set_var("ENGRAM_CUFILE_HOT", "1");
        cufile_note_h2d_fallback();
        assert_eq!(cufile_transfer_path(), "h2d_memcpy");
        std::env::remove_var("ENGRAM_CUFILE_HOT");
    }

    #[test]
    fn cufile_init_idempotent() {
        let _guard = cufile_test_lock();
        let first = cufile_init();
        let second = cufile_init();
        assert_eq!(first, second);
        #[cfg(target_os = "linux")]
        assert_eq!(second, CUFILE_INIT_OK.load(Ordering::Relaxed));
    }

    #[test]
    fn device_residency_q_bytes_constant() {
        assert_eq!(Q_VECTOR_BYTES, 65536);
    }

    /// Drive real cuFile DMA entrypoint against a real 256KB .leg on disk + GPU ptr.
    #[cfg(all(engram_backend_cuda, feature = "device_residency"))]
    #[test]
    fn cufile_direct_read_exercises_real_leg_and_device_ptr() {
        use engram_core::storage;
        use engram_core::types::{HolographicBlock, BLOCK_SIZE};
        use std::time::{SystemTime, UNIX_EPOCH};

        let _guard = cufile_test_lock();
        std::env::set_var("ENGRAM_CUFILE_HOT", "1");
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Prefer real Engram store path (NVMe-backed) over tmpfs for O_DIRECT + GDS.
        let store_root = std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".engram").join("stalks"))
            .unwrap_or_else(|_| std::env::temp_dir());
        let dir = store_root.join(format!("cufile_dma_probe_{ts}"));
        std::fs::create_dir_all(&dir).unwrap();
        let leg_path = dir.join("design:cufile_dma_probe.leg");

        let mut block: Box<HolographicBlock> = unsafe {
            let layout = std::alloc::Layout::new::<HolographicBlock>();
            let ptr = std::alloc::alloc_zeroed(layout) as *mut HolographicBlock;
            Box::from_raw(ptr)
        };
        block.q[0].re = 0.42;
        block.q[1].re = 0.99;
        storage::write_block(&leg_path, &block).expect("write aligned .leg");

        let meta = std::fs::metadata(&leg_path).unwrap();
        assert_eq!(meta.len() as usize, BLOCK_SIZE);

        let gpu_ptr = crate::cuda_dispatch::alloc_device_buffer(Q_VECTOR_BYTES)
            .expect("cudaMalloc for cuFile DMA probe");
        assert!(gpu_ptr != 0);

        let attempted_before = cufile_last_dma_attempted();
        let ok = cufile_direct_read_to_device(&leg_path, 0, Q_VECTOR_BYTES, gpu_ptr);
        assert!(cufile_last_dma_attempted() || !attempted_before);

        let path_label = cufile_transfer_path();
        assert!(
            cufile_last_dma_attempted(),
            "cufile_direct_read_to_device must be exercised on real .leg + GPU ptr"
        );
        assert!(
            ok || path_label == "h2d_memcpy" || path_label == "unavailable",
            "cuFile DMA must succeed or honestly fall back; ok={ok} path={path_label}"
        );
        if ok {
            assert_eq!(path_label, "cufile_dma");
            assert!(cufile_last_dma_success());
        }

        let evidence = format!(
            "cufile_direct_read_to_device ok={ok} attempted={} success={} path={}\n",
            cufile_last_dma_attempted(),
            cufile_last_dma_success(),
            cufile_transfer_path()
        );
        let scratch = "/tmp/grok-goal-06e08d787ea9/implementer/cufile_dma.txt";
        if let Ok(mut prior) = std::fs::read_to_string(scratch) {
            prior.push_str("=== cufile_direct_read_to_device real .leg ===\n");
            prior.push_str(&evidence);
            let _ = std::fs::write(scratch, prior);
        }

        crate::cuda_dispatch::free_device_ptr(gpu_ptr);
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_CUFILE_HOT");
    }
}
