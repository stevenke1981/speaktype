use cpal::traits::{DeviceTrait, HostTrait};
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

fn detect_gpu(use_cuda: bool) -> String {
    let Some(name) = query_video_controller_name() else {
        return if use_cuda {
            "未偵測到 GPU；CUDA 可能無法使用".to_string()
        } else {
            "未啟用 CUDA".to_string()
        };
    };

    if !use_cuda {
        return format!("{name}：已偵測（CUDA 未啟用）");
    }

    if name.to_ascii_lowercase().contains("nvidia") {
        format!("{name} (CUDA)：就緒")
    } else {
        format!("{name}：已偵測，但未確認 CUDA 支援")
    }
}

fn query_video_controller_name() -> Option<String> {
    query_video_controller_with_wmic().or_else(query_video_controller_with_powershell)
}

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
        .find(|name| {
            let lower = name.to_ascii_lowercase();
            !lower.contains("basic display") && !lower.contains("remote display")
        })
        .map(ToOwned::to_owned)
}

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
        .find(|name| {
            let lower = name.to_ascii_lowercase();
            !lower.contains("basic display") && !lower.contains("remote display")
        })
        .map(ToOwned::to_owned)
}
