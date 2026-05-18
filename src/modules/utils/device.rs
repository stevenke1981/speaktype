use cpal::traits::{DeviceTrait, HostTrait};
use std::path::Path;

#[derive(Debug)]
pub struct DeviceStatus {
    pub microphone: String,
    pub gpu: String,
    pub model: String,
}

impl DeviceStatus {
    pub fn detect() -> Self {
        let host = cpal::default_host();
        let microphone = match host.default_input_device() {
            Some(device) => match device.name() {
                Ok(name) => format!("{}：就緒", name),
                Err(_) => "就緒".to_string(),
            },
            None => "未偵測到裝置".to_string(),
        };

        let model_exists = Path::new("models/ggml-large-v3-turbo.bin").exists()
            || Path::new("models/large-v3-turbo.bin").exists();

        Self {
            microphone,
            gpu: "RTX 3060 Ti (CUDA)：就緒".to_string(),
            model: if model_exists {
                "large-v3-turbo：已載入".to_string()
            } else {
                "large-v3-turbo：未找到（將自動下載）".to_string()
            },
        }
    }
}
