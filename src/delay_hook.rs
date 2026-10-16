// Delay-load failure hook for GPU builds (CUDA / Vulkan) on Windows.
//
// The GPU runtime DLLs (nvcuda, cudart64_12, cublas64_12, cublasLt64_12,
// vulkan-1) are delay-loaded (see build.rs). If one is missing, the default
// delay-load helper would raise an exception and crash. Instead we hand back
// stubs that return "no device", so ggml simply finds no GPU and runs on the CPU.
#[cfg(all(windows, any(feature = "cuda", feature = "vulkan")))]
#[allow(non_upper_case_globals)]
mod delay_hook {
    use std::ffi::{c_char, c_void, CStr};

    #[repr(C)]
    struct DelayLoadProc {
        import_by_name: i32,
        name: *const c_char,
    }
    #[repr(C)]
    struct DelayLoadInfo {
        cb: u32,
        pidd: *const c_void,
        ppfn: *mut c_void,
        dll: *const c_char,
        dlp: DelayLoadProc,
        hmod_cur: *mut c_void,
        pfn_cur: *mut c_void,
        last_error: u32,
    }
    type Hook = unsafe extern "system" fn(u32, *mut DelayLoadInfo) -> *mut c_void;

    unsafe extern "system" {
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    }

    /// cudaErrorNoDevice / CUDA_ERROR_NO_DEVICE / CUBLAS failure codes are all non-zero.
    extern "C" fn cuda_no_device() -> i32 {
        100
    }
    /// VK_ERROR_INITIALIZATION_FAILED
    extern "C" fn vk_init_failed() -> i32 {
        -3
    }
    extern "C" fn null_ptr() -> *const c_void {
        std::ptr::null()
    }
    extern "C" fn error_string() -> *const c_char {
        c"GPU runtime DLL not installed".as_ptr()
    }

    const DLI_FAIL_LOAD_LIB: u32 = 3;
    const DLI_FAIL_GET_PROC: u32 = 4;

    unsafe extern "system" fn hook(notify: u32, info: *mut DelayLoadInfo) -> *mut c_void {
        unsafe { hook_inner(notify, info) }
    }

    unsafe fn hook_inner(notify: u32, info: *mut DelayLoadInfo) -> *mut c_void {
        unsafe { match notify {
            // Pretend the DLL is our own exe; every symbol lookup then fails
            // and lands in DLI_FAIL_GET_PROC below.
            DLI_FAIL_LOAD_LIB => GetModuleHandleW(std::ptr::null()),
            DLI_FAIL_GET_PROC if !info.is_null() => {
                let info = &*info;
                let dll = if info.dll.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(info.dll).to_string_lossy().to_ascii_lowercase()
                };
                let name = if info.dlp.import_by_name != 0 && !info.dlp.name.is_null() {
                    CStr::from_ptr(info.dlp.name).to_string_lossy().into_owned()
                } else {
                    String::new()
                };
                let f: *const c_void = if name.ends_with("ErrorString")
                    || name.ends_with("ErrorName")
                    || name.ends_with("StatusString")
                {
                    error_string as *const c_void
                } else if name.contains("ProcAddr") {
                    null_ptr as *const c_void
                } else if dll.starts_with("vulkan") {
                    vk_init_failed as *const c_void
                } else {
                    cuda_no_device as *const c_void
                };
                f as *mut c_void
            }
            _ => std::ptr::null_mut(),
        } }
    }

    #[unsafe(no_mangle)]
    #[used]
    static __pfnDliFailureHook2: Hook = hook;
}
