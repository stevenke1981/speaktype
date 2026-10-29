/// Vulkan loader (vulkan-1.dll, installed by GPU drivers) present?
pub fn vulkan_loader_present() -> bool {
    #[cfg(windows)]
    {
        let sys = std::env::var_os("SystemRoot").map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"));
        sys.join("System32").join("vulkan-1.dll").exists()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// GPU devices ggml can see, numbered like whisper's `gpu_device`
/// (GPU + iGPU devices in registry order): (index, name, total VRAM bytes).
/// Uses the backend registry, whose Vulkan init catches driver exceptions
/// (the direct `ggml_backend_vk_*` calls abort when no Vulkan device exists).
pub fn vulkan_devices() -> Vec<(i32, String, usize)> {
    static DEVICES: std::sync::OnceLock<Vec<(i32, String, usize)>> = std::sync::OnceLock::new();
    DEVICES
        .get_or_init(|| {
            #[cfg(feature = "vulkan")]
            {
                if vulkan_loader_present() {
                    return ggml_gpu_devices();
                }
            }
            Vec::new()
        })
        .clone()
}

#[cfg(feature = "vulkan")]
fn ggml_gpu_devices() -> Vec<(i32, String, usize)> {
    use whisper_rs_sys as sys;
    let text = |p: *const std::os::raw::c_char| {
        if p.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    let mut out = Vec::new();
    let n = unsafe { sys::ggml_backend_dev_count() };
    for i in 0..n {
        let dev = unsafe { sys::ggml_backend_dev_get(i) };
        if dev.is_null() {
            continue;
        }
        let ty = unsafe { sys::ggml_backend_dev_type(dev) };
        if ty != sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_GPU
            && ty != sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_IGPU
        {
            continue;
        }
        let mut name = text(unsafe { sys::ggml_backend_dev_description(dev) });
        if name.is_empty() {
            name = text(unsafe { sys::ggml_backend_dev_name(dev) });
        }
        if ty == sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_IGPU {
            name.push_str(" [integrated]");
        }
        let (mut free, mut total) = (0usize, 0usize);
        unsafe { sys::ggml_backend_dev_memory(dev, &mut free, &mut total) };
        out.push((out.len() as i32, name, total));
    }
    out
}

/// Prefer a discrete NVIDIA/AMD GPU, then Intel/other real GPUs; never a
/// software or virtual adapter (llvmpipe, SwiftShader, "Virtual Display Driver").
pub fn pick_vulkan_device(devices: &[(i32, String, usize)]) -> Option<(i32, String)> {
    devices
        .iter()
        .filter_map(|(id, name, vram)| {
            let l = name.to_ascii_lowercase();
            if ["llvmpipe", "lavapipe", "swiftshader", "microsoft basic", "virtual", "software"]
                .iter()
                .any(|bad| l.contains(bad))
            {
                return None;
            }
            let rank = if l.contains("[integrated]") {
                0
            } else if ["nvidia", "geforce", "rtx", "quadro", "amd", "radeon"].iter().any(|v| l.contains(v)) {
                2
            } else {
                1
            };
            Some((rank, *vram, *id, name.clone()))
        })
        .max_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)).then(b.2.cmp(&a.2)))
        .map(|(_, _, id, name)| (id, name))
}

/// GPU device the Vulkan build would use, if any.
pub fn selected_vulkan_device() -> Option<(i32, String)> {
    pick_vulkan_device(&vulkan_devices())
}
