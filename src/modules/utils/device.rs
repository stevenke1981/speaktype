use cpal::traits::{DeviceTrait, HostTrait};
#[cfg(windows)]
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

#[derive(Debug)]
pub struct DeviceStatus {
    pub microphone: String,
    pub gpu: String,
    pub model: String,
}

impl DeviceStatus {
    pub fn detect(model: String, use_cuda: bool) -> Self {
        let host = cpal::default_host();
        let microphone = match host.default_input_device() {
            Some(device) => match device.name() {
                Ok(name) => format!("{}：就緒", name),
                Err(_) => "就緒".to_string(),
            },
            None => "未偵測到裝置".to_string(),
        };

        Self {
            microphone,
            gpu: detect_gpu(use_cuda),
            model,
        }
    }
}

static GPU_NAME: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Query the GPU name on a background thread. `wmic`/PowerShell take 0.3–2 s
/// (and `wmic` no longer exists on Windows 11 24H2), so this must never run on
/// the UI thread during startup.
pub fn prefetch_gpu_name() {
    std::thread::spawn(|| {
        let _ = GPU_NAME.get_or_init(query_video_controller_name);
    });
}

fn detect_gpu(use_cuda: bool) -> String {
    let Some(cached) = GPU_NAME.get() else {
        return "偵測中…".to_string();
    };
    let Some(name) = cached.clone() else {
        return if use_cuda {
            "未偵測到 GPU；CUDA 可能無法使用".to_string()
        } else {
            "已選擇 CPU".to_string()
        };
    };

    if !use_cuda {
        return format!("{name}：已偵測（已選擇 CPU）");
    }

    match crate::modules::transcription::transcriber::gpu_runtime_status() {
        Ok(backend) => format!("{name}：{} 可用", backend.label()),
        Err(reason) => format!("{name}：{reason}"),
    }
}

#[cfg(windows)]
fn query_video_controller_name() -> Option<String> {
    query_video_controller_with_wmic().or_else(query_video_controller_with_powershell)
}

/// Prefer a real NVIDIA/AMD/Intel adapter over virtual/basic display drivers.
pub fn pick_gpu_name<S: AsRef<str>>(names: &[S]) -> Option<String> {
    let names: Vec<&str> = names.iter().map(|n| n.as_ref().trim()).filter(|n| !n.is_empty()).collect();
    fn real(n: &str) -> bool {
        let l = n.to_ascii_lowercase();
        !l.contains("basic display") && !l.contains("remote display") && !l.contains("virtual")
    }
    for vendor in ["nvidia", "amd", "radeon", "intel"] {
        if let Some(n) = names.iter().find(|n| real(n) && n.to_ascii_lowercase().contains(vendor)) {
            return Some(n.to_string());
        }
    }
    names.iter().find(|n| real(n)).or(names.first()).map(|n| n.to_string())
}

#[cfg(windows)]
trait PickGpu {
    fn pipe_pick(self) -> Option<String>;
}
#[cfg(windows)]
impl PickGpu for Vec<&str> {
    fn pipe_pick(self) -> Option<String> {
        pick_gpu_name(&self)
    }
}

#[cfg(not(windows))]
fn query_video_controller_name() -> Option<String> {
    None
}

#[cfg(windows)]
fn query_video_controller_with_wmic() -> Option<String> {
    let mut command = Command::new("wmic");
    command.args(["path", "win32_VideoController", "get", "name", "/value"]);

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Name="))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .pipe_pick()
}

#[cfg(windows)]
fn query_video_controller_with_powershell() -> Option<String> {
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-Command",
        "(Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name) -join \"`n\"",
    ]);

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .pipe_pick()
}

#[cfg(test)]
mod gpu_pick_tests {
    #[test]
    fn prefers_nvidia_over_virtual_display() {
        let names = ["Virtual Display Driver", "NVIDIA GeForce RTX 3060 Ti"];
        assert_eq!(super::pick_gpu_name(&names).as_deref(), Some("NVIDIA GeForce RTX 3060 Ti"));
        assert_eq!(super::pick_gpu_name(&["Intel(R) UHD Graphics"]).as_deref(), Some("Intel(R) UHD Graphics"));
        assert_eq!(super::pick_gpu_name::<&str>(&[]), None);
    }
}
