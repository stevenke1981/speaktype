use super::cuda::{missing_cuda_runtime_dlls, nvidia_driver_present};
use super::vulkan::{selected_vulkan_device, vulkan_devices, vulkan_loader_present};

/// Inference backend actually used for a loaded model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    Cuda,
    Vulkan,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Backend::Cpu => "CPU",
            Backend::Cuda => "CUDA",
            Backend::Vulkan => "Vulkan",
        }
    }
}

/// True when this binary was built with CUDA support.
pub const fn built_with_cuda() -> bool {
    cfg!(feature = "cuda")
}

/// True when this binary was built with Vulkan support.
pub const fn built_with_vulkan() -> bool {
    cfg!(feature = "vulkan")
}

/// GPU backend compiled into this binary, if any.
pub const fn built_gpu_backend() -> Option<Backend> {
    if built_with_cuda() {
        Some(Backend::Cuda)
    } else if built_with_vulkan() {
        Some(Backend::Vulkan)
    } else {
        None
    }
}

/// Can the GPU backend compiled into this binary run here? Err = reason (zh-TW).
/// User's requested backend: 0 auto, 1 Vulkan, 2 CUDA, 3 CPU (see GpuMode).
static REQUESTED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set_requested_backend(code: u8) {
    REQUESTED.store(code, std::sync::atomic::Ordering::Relaxed);
}

/// Can the GPU backend compiled into this binary run here? Err = reason (zh-TW).
pub fn gpu_runtime_status() -> Result<Backend, String> {
    let requested = REQUESTED.load(std::sync::atomic::Ordering::Relaxed);
    let build = built_gpu_backend();
    match (requested, build) {
        (3, _) => return Err("已選擇 CPU".to_string()),
        (1, Some(Backend::Cuda)) => return Err("已選擇 Vulkan，但這是 CUDA 版，改用 CPU；請安裝 Vulkan 版".to_string()),
        (2, Some(Backend::Vulkan)) => return Err("已選擇 CUDA，但這是 Vulkan 版，改用 CPU；請安裝 CUDA 版".to_string()),
        _ => {}
    }
    match build {
        None => Err("此版本為 CPU 版；要用 GPU 請安裝 Vulkan 版（建議）或 CUDA 版".to_string()),
        Some(Backend::Cuda) => {
            if !nvidia_driver_present() {
                return Err("找不到 NVIDIA 驅動程式 (nvcuda.dll)，改用 CPU".to_string());
            }
            let missing = missing_cuda_runtime_dlls();
            if missing.is_empty() {
                Ok(Backend::Cuda)
            } else {
                Err(format!("缺少 CUDA 元件 {}，改用 CPU；可在「設定 → 辨識硬體」下載", missing.join(", ")))
            }
        }
        Some(b) => {
            if !vulkan_loader_present() {
                return Err("找不到 Vulkan 驅動程式 (vulkan-1.dll)，改用 CPU；請更新顯示卡驅動".to_string());
            }
            if selected_vulkan_device().is_some() {
                Ok(b)
            } else {
                let seen: Vec<String> = vulkan_devices().into_iter().map(|d| d.1).collect();
                Err(format!("找不到可用的 Vulkan GPU（偵測到：{}），改用 CPU", if seen.is_empty() { "無".to_string() } else { seen.join("、") }))
            }
        }
    }
}

/// Human-readable device the GPU backend uses ("" for CPU).
pub fn gpu_device_label(backend: Backend) -> String {
    match backend {
        Backend::Vulkan => selected_vulkan_device().map(|d| d.1).unwrap_or_default(),
        Backend::Cuda => crate::modules::utils::device::cached_gpu_name().unwrap_or_default(),
        Backend::Cpu => String::new(),
    }
}

/// Decide the backend from the user preference, the build and the driver.
pub fn choose_backend(prefer_gpu: bool, gpu: Result<Backend, String>) -> Backend {
    match gpu {
        Ok(b) if prefer_gpu => b,
        _ => Backend::Cpu,
    }
}

/// Backend that a model load with this preference would use right now.
pub fn current_backend(prefer_gpu: bool) -> Backend {
    choose_backend(prefer_gpu, gpu_runtime_status())
}
