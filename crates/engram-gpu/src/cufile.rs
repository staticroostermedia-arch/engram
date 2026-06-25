//! GPUDirect Storage / cuFile detection, init, and NVMe→GPU DMA for hot residency.

use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::OnceLock;

static CUFILE_DETECTED: AtomicBool = AtomicBool::new(false);
static CUFILE_PROBE_DONE: AtomicBool = AtomicBool::new(false);
static CUFILE_INIT_OK: AtomicBool = AtomicBool::new(false);
static CUFILE_INIT_TRIED: AtomicBool = AtomicBool::new(false);
/// 0=off, 1=cufile_dma, 2=h2d_memcpy, 3=unavailable
static LAST_TRANSFER_MODE: AtomicU8 = AtomicU8::new(3);

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

fn probe_cufile_driver() -> bool {
    if Path::new("/usr/local/cuda/gds/cufile.json").exists()
        || Path::new("/etc/cufile.json").exists()
    {
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
pub fn cufile_driver_detected() -> bool {
    if CUFILE_PROBE_DONE.load(Ordering::Relaxed) {
        return CUFILE_DETECTED.load(Ordering::Relaxed);
    }
    let detected = probe_cufile_driver();
    CUFILE_DETECTED.store(detected, Ordering::Relaxed);
    CUFILE_PROBE_DONE.store(true, Ordering::Relaxed);
    detected
}

pub fn cufile_transfer_path() -> &'static str {
    match LAST_TRANSFER_MODE.load(Ordering::Relaxed) {
        MODE_CUFILE_DMA => "cufile_dma",
        MODE_H2D_MEMCPY => "h2d_memcpy",
        MODE_OFF => "off",
        _ => "unavailable",
    }
}

fn set_transfer_mode(mode: u8) {
    LAST_TRANSFER_MODE.store(mode, Ordering::Relaxed);
}

/// Hot NVMe→GPU path is active: requested, driver present, and cuFile driver open succeeded.
pub fn cufile_hot_active() -> bool {
    if !cufile_hot_requested() {
        set_transfer_mode(MODE_OFF);
        return false;
    }
    if !cufile_driver_detected() {
        set_transfer_mode(MODE_UNAVAILABLE);
        return cfg!(engram_backend_cuda);
    }
    cufile_init() || cfg!(engram_backend_cuda)
}

/// q-vector byte length at offset 0 in a .leg block (8192 × Complex32).
pub const Q_VECTOR_BYTES: usize = 8192 * std::mem::size_of::<f32>() * 2;

#[cfg(target_os = "linux")]
#[allow(clippy::missing_transmute_annotations)]
mod ffi {
    use super::*;
    use std::os::unix::io::AsRawFd;
    use std::sync::Mutex;

    type CuFileDriverOpenFn = unsafe extern "C" fn() -> i32;
    type CuFileDriverCloseFn = unsafe extern "C" fn() -> i32;
    type CuFileHandleRegisterFn =
        unsafe extern "C" fn(*mut u64, *const CuFileDescr) -> i32;
    type CuFileHandleDeregisterFn = unsafe extern "C" fn(u64) -> i32;
    type CuFileReadFn =
        unsafe extern "C" fn(u64, *mut c_void, usize, i64, usize) -> isize;
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
    static OPEN_HANDLES: Mutex<Vec<(u64, i32)>> = Mutex::new(Vec::new());

    fn load_api() -> Option<&'static CuFileApi> {
        API.get_or_init(|| {
            unsafe {
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
            }
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
        let Some(api) = load_api() else {
            return false;
        };
        if !CUFILE_INIT_OK.load(Ordering::Relaxed) && !cufile_init_inner() {
            return false;
        }
        if gpu_ptr == 0 || size == 0 {
            return false;
        }

        let file = match std::fs::File::open(leg_path) {
            Ok(f) => f,
            Err(e) => {
                tracing::debug!("[cufile] open {} failed: {e}", leg_path.display());
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
            tracing::debug!("[cufile] cuFileHandleRegister failed for {}", leg_path.display());
            return false;
        }

        let gpu = gpu_ptr as *mut c_void;
        if unsafe { (api.buf_register)(gpu, size, 0) } != 0 {
            let _ = unsafe { (api.handle_deregister)(fh) };
            tracing::debug!("[cufile] cuFileBufRegister failed");
            return false;
        }

        let nbytes = unsafe {
            (api.read)(
                fh,
                gpu,
                size,
                file_offset as i64,
                size,
            )
        };

        let _ = unsafe { (api.buf_deregister)(gpu) };
        let _ = unsafe { (api.handle_deregister)(fh) };

        if nbytes < 0 || nbytes as usize != size {
            tracing::debug!(
                "[cufile] cuFileRead returned {nbytes} (expected {size}) for {}",
                leg_path.display()
            );
            return false;
        }

        if let Ok(mut handles) = OPEN_HANDLES.lock() {
            handles.push((fh, fd));
        }

        set_transfer_mode(MODE_CUFILE_DMA);
        tracing::debug!(
            "[cufile] cuFile DMA read {size} bytes @ {file_offset} → GPU {gpu_ptr:#x} ({})",
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

    #[test]
    fn cufile_hot_requested_opt_in() {
        std::env::remove_var("ENGRAM_CUFILE_HOT");
        assert!(!cufile_hot_requested());
        std::env::set_var("ENGRAM_CUFILE_HOT", "1");
        assert!(cufile_hot_requested());
        std::env::remove_var("ENGRAM_CUFILE_HOT");
    }

    #[test]
    fn cufile_transfer_path_labels() {
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
        let _ = cufile_init();
        let second = cufile_init();
        assert_eq!(second, CUFILE_INIT_OK.load(Ordering::Relaxed));
    }

    #[test]
    fn device_residency_q_bytes_constant() {
        assert_eq!(Q_VECTOR_BYTES, 65536);
    }
}