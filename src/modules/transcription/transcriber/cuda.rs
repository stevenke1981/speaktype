/// CUDA runtime DLLs SpeakType's CUDA build loads (downloaded by the installer).
pub const CUDA_RUNTIME_DLLS: [&str; 3] = ["cudart64_12.dll", "cublas64_12.dll", "cublasLt64_12.dll"];

/// CUDA runtime DLLs missing next to the exe (or on PATH).
pub fn missing_cuda_runtime_dlls() -> Vec<&'static str> {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf()));
    let path_dirs: Vec<std::path::PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    CUDA_RUNTIME_DLLS
        .iter()
        .copied()
        .filter(|dll| {
            !exe_dir.iter().chain(path_dirs.iter()).any(|d| d.join(dll).is_file())
        })
        .collect()
}

/// NVIDIA driver present (nvcuda.dll / libcuda)? Without it a CUDA build must
/// run on the CPU.
pub fn nvidia_driver_present() -> bool {
    #[cfg(windows)]
    {
        let sys = std::env::var_os("SystemRoot").map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"));
        sys.join("System32").join("nvcuda.dll").exists()
    }
    #[cfg(not(windows))]
    {
        std::path::Path::new("/proc/driver/nvidia/version").exists()
    }
}
