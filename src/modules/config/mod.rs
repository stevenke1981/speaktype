// config/mod.rs - 設定檔持久化模組
// 職責：儲存與載入使用者設定（情境、模型路徑、熱鍵等）

use crate::modules::error::log_error;
use crate::modules::paths;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::path::PathBuf;

const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 最後選擇的情境
    #[serde(default)]
    pub last_scenario: Option<String>,

    /// 模型名稱
    #[serde(default = "default_model_name")]
    pub model_name: Option<String>,

    /// 模型目錄
    #[serde(default = "default_models_dir")]
    pub models_dir: Option<String>,

    /// 是否啟用 CUDA
    #[serde(default = "default_use_cuda")]
    pub use_cuda: bool,

    /// 錄音相關設定
    #[serde(default)]
    pub recording: RecordingConfig,

    /// 熱鍵設定
    #[serde(default)]
    pub hotkeys: HotkeyConfig,

    /// 文字輸出與暫存設定
    #[serde(default)]
    pub output: OutputConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_scenario: None,
            model_name: Some("large-v3-turbo".to_string()),
            models_dir: Some(default_models_dir().unwrap_or_else(|| "models".to_string())),
            use_cuda: true,
            recording: RecordingConfig::default(),
            hotkeys: HotkeyConfig::default(),
            output: OutputConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    /// 錄音取樣率
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,

    /// 是否啟用簡單 VAD
    #[serde(default)]
    pub enable_vad: bool,

    /// 錄音檔保留天數，0 表示不依天數刪除
    #[serde(default = "default_recording_retention_days")]
    pub retention_days: u32,

    /// 錄音檔總容量上限 MB，0 表示不限制
    #[serde(default = "default_recording_max_total_mb")]
    pub max_total_mb: u64,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            enable_vad: false,
            retention_days: 30,
            max_total_mb: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// 錄音切換熱鍵
    #[serde(default = "default_record_hotkey")]
    pub record_toggle: String,

    /// 是否啟用全域熱鍵
    #[serde(default = "default_global_hotkey_enabled")]
    pub global_hotkey_enabled: bool,

    /// 按住快捷鍵錄音，放開後轉錄
    #[serde(default = "default_hold_to_record")]
    pub hold_to_record: bool,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            record_toggle: "Ctrl+Shift+L".to_string(),
            global_hotkey_enabled: true,
            hold_to_record: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputBufferMode {
    Clipboard,
    #[default]
    Temporary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// 轉錄文字暫存方式：剪貼簿或 App 內部暫存區
    #[serde(default)]
    pub buffer_mode: OutputBufferMode,

    /// 轉錄完成後是否自動注入目前焦點視窗
    #[serde(default = "default_auto_inject_focused_window")]
    pub auto_inject_focused_window: bool,

    /// 使用暫存區注入時，盡量還原原本剪貼簿文字內容
    #[serde(default = "default_restore_clipboard_after_inject")]
    pub restore_clipboard_after_inject: bool,

    /// OpenCC 文字轉換：不轉換、繁體台灣用語、簡體中國大陸用語
    #[serde(default)]
    pub chinese_conversion: ChineseConversionMode,

    /// 轉錄完成後先放在預覽區，由使用者手動送出
    #[serde(default)]
    pub manual_review_before_send: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            buffer_mode: OutputBufferMode::Temporary,
            auto_inject_focused_window: true,
            restore_clipboard_after_inject: true,
            chinese_conversion: ChineseConversionMode::default(),
            manual_review_before_send: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChineseConversionMode {
    #[default]
    Disabled,
    TraditionalTaiwan,
    SimplifiedChina,
}

impl AppConfig {
    /// 載入設定檔（若不存在則回傳預設值）
    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                match fs::read_to_string(&path) {
                    Ok(content) => match toml::from_str::<AppConfig>(&content) {
                        Ok(mut config) => {
                            config.migrate_defaults();
                            println!("[config] 已載入設定檔: {:?}", path);
                            return config;
                        }
                        Err(err) => {
                            log_error("config parse", format!("{}: {}", path.display(), err));
                            backup_invalid_config(&path);
                        }
                    },
                    Err(err) => {
                        log_error("config read", format!("{}: {}", path.display(), err));
                    }
                }
            }
        }
        println!("[config] 使用預設設定");
        Self::default()
    }

    fn migrate_defaults(&mut self) {
        if self.hotkeys.record_toggle.trim().eq_ignore_ascii_case("F2") {
            self.hotkeys.record_toggle = "Ctrl+Shift+L".to_string();
        }
        self.hotkeys.hold_to_record = true;
        if self.get_models_dir() == "models" {
            self.models_dir = default_models_dir();
        }
    }

    /// 儲存設定檔
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path().ok_or("無法取得設定檔路徑")?;

        // 確保目錄存在
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        let temp_path = path.with_extension("toml.tmp");
        fs::write(&temp_path, content).map_err(|e| e.to_string())?;
        fs::rename(&temp_path, &path).map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            e.to_string()
        })?;

        println!("[config] 設定已儲存: {:?}", path);
        Ok(())
    }

    /// 取得設定檔完整路徑
    fn config_path() -> Option<PathBuf> {
        Some(paths::config_dir().join(CONFIG_FILE))
    }

    /// 取得模型目錄
    pub fn get_models_dir(&self) -> String {
        self.models_dir
            .clone()
            .unwrap_or_else(|| "models".to_string())
    }

    /// 取得模型名稱
    pub fn get_model_name(&self) -> String {
        self.model_name
            .clone()
            .unwrap_or_else(|| "large-v3-turbo".to_string())
    }

    /// 取得預期模型檔路徑。whisper.cpp 模型檔通常以 ggml- 開頭。
    pub fn get_model_path(&self) -> PathBuf {
        let models_dir = PathBuf::from(self.get_models_dir());
        let model_name = self.get_model_name();
        let ggml_name = format!("ggml-{}.bin", model_name);
        let legacy_name = format!("{}.bin", model_name);

        let ggml_path = models_dir.join(&ggml_name);
        if ggml_path.exists() {
            return ggml_path;
        }

        let legacy_path = models_dir.join(&legacy_name);
        if legacy_path.exists() {
            return legacy_path;
        }

        ggml_path
    }
}

fn backup_invalid_config(path: &PathBuf) {
    let backup_path = path.with_extension("toml.invalid");
    match File::open(path).and_then(|mut input| {
        let mut output = File::create(&backup_path)?;
        std::io::copy(&mut input, &mut output)?;
        Ok(())
    }) {
        Ok(()) => {
            log_error(
                "config backup",
                format!("invalid config copied to {}", backup_path.display()),
            );
        }
        Err(err) => {
            log_error("config backup", format!("{}: {}", path.display(), err));
        }
    }
}

fn default_model_name() -> Option<String> {
    Some("large-v3-turbo".to_string())
}

fn default_models_dir() -> Option<String> {
    Some(paths::models_dir().display().to_string())
}

fn default_use_cuda() -> bool {
    true
}

fn default_sample_rate() -> u32 {
    16000
}

fn default_recording_retention_days() -> u32 {
    30
}

fn default_recording_max_total_mb() -> u64 {
    4096
}

fn default_record_hotkey() -> String {
    "Ctrl+Shift+L".to_string()
}

fn default_global_hotkey_enabled() -> bool {
    true
}

fn default_hold_to_record() -> bool {
    true
}

fn default_auto_inject_focused_window() -> bool {
    true
}

fn default_restore_clipboard_after_inject() -> bool {
    true
}
