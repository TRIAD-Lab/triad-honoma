use std::ffi::c_void;
use log::{info, warn};

/// Lock a memory range to prevent swapping (mlock)
/// Critical for low-latency consensus state.
pub unsafe fn lock_memory<T>(ptr: *const T, len: usize) {
    let size = len * std::mem::size_of::<T>();
    let addr = ptr as *const c_void;
    
    #[cfg(unix)]
    {
    let ret = unsafe { libc::mlock(addr, size) };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            warn!("Failed to mlock memory at {:p} (size {}): {}", addr, size, err);
        } else {
            info!("Successfully mlocked memory at {:p} (size {})", addr, size);
        }
    }
}

/// Advise the kernel to use Huge Pages for this memory range (madvise MADV_HUGEPAGE)
pub unsafe fn advise_huge_page<T>(ptr: *const T, len: usize) {
    let size = len * std::mem::size_of::<T>();
    let addr = ptr as *mut c_void;

    #[cfg(target_os = "linux")]
    {
        let ret = libc::madvise(addr, size, libc::MADV_HUGEPAGE);
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            warn!("Failed to madvise HUGEPAGE at {:p}: {}", addr, err);
        } else {
            info!("Enabled HUGEPAGE for memory at {:p}", addr);
        }
    }
}

/// Helper to lock a slice
pub fn lock_slice<T>(slice: &[T]) {
    unsafe {
        lock_memory(slice.as_ptr(), slice.len());
        advise_huge_page(slice.as_ptr(), slice.len());
    }
}


