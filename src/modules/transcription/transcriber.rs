//! Local whisper.cpp transcription.
//!
//! Speed notes (see `docs/PERFORMANCE.md`):
//! * the `WhisperState` (KV cache / compute buffers) is created once and reused,
//! * the encoder window (`audio_ctx`) is shrunk to the clip length instead of the
//!   fixed 30 s window, which is the single biggest CPU win for short dictation,
//! * timestamps are disabled and a single segment is produced,
//! * all physical cores are used (whisper.cpp defaults to 4),
//! * a 1 s warm-up pass runs right after loading so the first real dictation
//!   does not pay for buffer allocation.
use std::path::Path;
use std::sync::Mutex;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

/// Per-call decoding options.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscribeOptions {
    /// Whisper language code ("zh", "en", ...) or "auto" for detection.
    pub language: String,
    /// Prompt used to bias spelling / script (e.g. Traditional Chinese, dictionary terms).
    pub initial_prompt: String,
    /// Shrink the encoder window to the clip length (much faster for short clips).
    pub fast_encoder: bool,
    /// Worker threads; 0 = automatic.
    pub threads: usize,
}

impl Default for TranscribeOptions {
    fn default() -> Self {
        Self {
            language: "auto".to_string(),
            initial_prompt: String::new(),
            fast_encoder: true,
            threads: 0,
        }
    }
}

pub struct Transcriber {
    // Field order matters: the state must be dropped before the context.
    state: Mutex<WhisperState>,
    _ctx: WhisperContext,
    backend: Backend,
}

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

/// Can the GPU backend compiled into this binary run here? Err = reason (zh-TW).
/// User's requested backend: 0 auto, 1 Vulkan, 2 CUDA, 3 CPU (see GpuMode).
static REQUESTED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set_requested_backend(code: u8) {
    REQUESTED.store(code, std::sync::atomic::Ordering::Relaxed);
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

const SAMPLE_RATE: usize = 16_000;
/// Whisper encoder frames per second of audio (1500 frames / 30 s).
const FRAMES_PER_SEC: usize = 50;

impl Transcriber {
    pub fn new(model_path: &Path, prefer_gpu: bool) -> Result<Self, String> {
        let gpu = gpu_runtime_status();
        let backend = choose_backend(prefer_gpu, gpu.clone());
        let use_gpu = backend != Backend::Cpu;
        if let (true, Err(reason)) = (prefer_gpu, &gpu) {
            crate::modules::error::log_error("whisper backend", reason);
        }
        let gpu_device = match backend {
            Backend::Vulkan => selected_vulkan_device().map(|d| d.0).unwrap_or(0),
            _ => 0,
        };
        // Flash attention: supported by ggml's CUDA and Vulkan backends; on the
        // CPU it only slowed things down.
        let params = WhisperContextParameters { use_gpu, flash_attn: use_gpu, gpu_device, ..Default::default() };
        crate::modules::error::log_error(
            "whisper backend",
            format!(
                "backend={} device={:?} (prefer_gpu={prefer_gpu}, build_gpu={:?}, gpu_status={:?}, vulkan_devices={:?}) threads={} system_info={}",
                backend.label(),
                gpu_device_label(backend),
                built_gpu_backend(),
                gpu,
                vulkan_devices(),
                threads_for(backend, 0),
                whisper_rs::print_system_info().trim()
            ),
        );
        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| "模型路徑包含無效 Unicode".to_string())?,
            params,
        )
        .map_err(|e| format!("模型載入失敗: {}", e))?;
        let state = ctx
            .create_state()
            .map_err(|e| format!("建立轉錄狀態失敗: {}", e))?;
        Ok(Self {
            state: Mutex::new(state),
            _ctx: ctx,
            backend,
        })
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    /// Run a short silent pass so the first real dictation is not slower.
    pub fn warm_up(&self) {
        let silence = vec![0.0_f32; SAMPLE_RATE];
        let _ = self.transcribe_with(
            &silence,
            &TranscribeOptions {
                language: "en".into(),
                ..Default::default()
            },
        );
    }

    /// Backwards compatible helper (Chinese, default options).
    pub fn transcribe(&self, audio_samples: &[f32]) -> Result<String, String> {
        self.transcribe_with(audio_samples, &TranscribeOptions::default())
    }

    pub fn transcribe_with(
        &self,
        audio_samples: &[f32],
        options: &TranscribeOptions,
    ) -> Result<String, String> {
        if audio_samples.is_empty() {
            return Ok(String::new());
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let language = options.language.trim();
        if language.is_empty() || language.eq_ignore_ascii_case("auto") {
            params.set_language(Some("auto"));
        } else {
            params.set_language(Some(language));
        }
        params.set_n_threads(threads_for(self.backend, options.threads) as i32);
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_no_timestamps(true);
        params.set_suppress_blank(true);
        params.set_no_speech_thold(0.6);
        let seconds = audio_samples.len() / SAMPLE_RATE;
        if seconds < 30 {
            params.set_single_segment(true);
        }
        if options.fast_encoder {
            if let Some(ctx) = fast_audio_ctx(audio_samples.len()) {
                params.set_audio_ctx(ctx);
            }
        }
        if !options.initial_prompt.trim().is_empty() {
            params.set_initial_prompt(options.initial_prompt.trim());
        }

        state
            .full(params, audio_samples)
            .map_err(|e| format!("轉錄失敗: {}", e))?;

        let mut result = String::new();
        for i in 0..state.full_n_segments() {
            if let Some(segment) = state.get_segment(i) {
                let text = segment.to_str_lossy().map(|s| s.to_string()).unwrap_or_default();
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                if !result.is_empty() {
                    result.push(' ');
                }
                result.push_str(text);
            }
        }
        Ok(result)
    }
}

/// Encoder window for a clip of `samples` 16 kHz samples, with head-room.
/// Returns `None` when the full 30 s window should be used.
pub fn fast_audio_ctx(samples: usize) -> Option<i32> {
    let frames = samples.div_ceil(SAMPLE_RATE / FRAMES_PER_SEC);
    // ~1.3 s of padding and a floor keep short clips from hallucinating.
    let ctx = (frames + 64).max(384);
    if ctx >= 1500 {
        None
    } else {
        Some(ctx.next_multiple_of(64) as i32)
    }
}

/// On a GPU the encoder/decoder run on the device; a few CPU threads are
/// enough for the rest and leave the cores free.
pub fn threads_for(backend: Backend, requested: usize) -> usize {
    let n = resolve_threads(requested);
    if backend == Backend::Cpu || requested > 0 {
        n
    } else {
        n.min(4)
    }
}

pub fn resolve_threads(requested: usize) -> usize {
    if requested > 0 {
        return requested;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8)
}

#[cfg(test)]
mod tests {
    use super::{choose_backend, Backend};

    #[test]
    fn vulkan_device_prefers_discrete_real_gpu() {
        use super::pick_vulkan_device as pick;
        let d = |id: i32, n: &str, v: usize| (id, n.to_string(), v);
        assert_eq!(pick(&[]), None);
        assert_eq!(pick(&[d(0, "llvmpipe (LLVM 17)", 1)]), None);
        assert_eq!(
            pick(&[d(0, "Intel(R) UHD Graphics 770", 2 << 30), d(1, "NVIDIA GeForce RTX 3060 Ti", 8 << 30)]).unwrap().0,
            1
        );
        assert_eq!(
            pick(&[d(0, "Virtual Display Driver", 1 << 30), d(1, "Intel(R) Arc A380", 6 << 30)]).unwrap().0,
            1
        );
        assert_eq!(pick(&[d(0, "AMD Radeon(TM) Graphics [integrated]", 8 << 30), d(1, "AMD Radeon RX 6600", 4 << 30)]).unwrap().0, 1);
        assert_eq!(pick(&[d(0, "Intel(R) UHD Graphics [integrated]", 1 << 30)]).unwrap().0, 0);
    }

    #[test]
    fn gpu_uses_few_threads() {
        assert!(super::threads_for(Backend::Vulkan, 0) <= 4);
        assert_eq!(super::threads_for(Backend::Vulkan, 6), 6);
        assert_eq!(super::threads_for(Backend::Cpu, 0), super::resolve_threads(0));
    }

    #[test]
    fn backend_falls_back_to_cpu() {
        assert_eq!(choose_backend(true, Ok(Backend::Cuda)), Backend::Cuda);
        assert_eq!(choose_backend(true, Ok(Backend::Vulkan)), Backend::Vulkan);
        assert_eq!(choose_backend(true, Err("x".into())), Backend::Cpu);
        assert_eq!(choose_backend(false, Ok(Backend::Cuda)), Backend::Cpu);
        // CPU-only test build reports a reason.
        if super::built_gpu_backend().is_none() {
            assert!(super::gpu_runtime_status().is_err());
        }
    }

    use super::*;

    #[test]
    fn audio_ctx_scales_with_clip() {
        assert_eq!(fast_audio_ctx(SAMPLE_RATE), Some(384));
        assert_eq!(fast_audio_ctx(SAMPLE_RATE * 10), Some(576));
        assert_eq!(fast_audio_ctx(SAMPLE_RATE * 29), None);
    }

    #[test]
    fn threads_auto_is_bounded() {
        let t = resolve_threads(0);
        assert!((1..=8).contains(&t));
        assert_eq!(resolve_threads(3), 3);
    }
}
