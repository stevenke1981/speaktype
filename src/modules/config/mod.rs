// config/mod.rs - 設定檔持久化模組
// 職責：儲存與載入使用者設定（情境、模型路徑、熱鍵等）

use crate::modules::error::log_error;
use crate::modules::models;
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

    /// Windows 啟動設定
    #[serde(default)]
    pub startup: StartupConfig,

    /// 語音辨識設定（語言、速度）
    #[serde(default)]
    pub transcription: TranscriptionConfig,

    /// 文字送出方式（貼上／Unicode 打字）與各 App 規則
    #[serde(default)]
    pub injection: InjectionConfig,

    /// 選用的 AI 潤稿（本機 Ollama / LM Studio 或 OpenAI 相容 API）
    #[serde(default)]
    pub ai_cleanup: AiCleanupConfig,

    /// 浮動錄音指示器
    #[serde(default)]
    pub indicator: IndicatorConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_scenario: None,
            model_name: Some(models::DEFAULT_MODEL.to_string()),
            models_dir: default_models_dir(),
            use_cuda: true,
            recording: RecordingConfig::default(),
            hotkeys: HotkeyConfig::default(),
            output: OutputConfig::default(),
            startup: StartupConfig::default(),
            transcription: TranscriptionConfig::default(),
            injection: InjectionConfig::default(),
            ai_cleanup: AiCleanupConfig::default(),
            indicator: IndicatorConfig::default(),
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

    /// 指定麥克風名稱；None 使用系統預設
    #[serde(default)]
    pub input_device_name: Option<String>,

    /// 錄音增益倍率
    #[serde(default = "default_recording_gain")]
    pub gain: f32,

    /// 轉錄模式：穩定或快速
    #[serde(default)]
    pub transcription_mode: TranscriptionMode,

    /// 兩次錄音之間保持麥克風開啟（按下快捷鍵即錄，並保留 300ms 預錄）
    #[serde(default = "default_true_rec")]
    pub keep_mic_warm: bool,

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
            input_device_name: None,
            gain: 1.0,
            transcription_mode: TranscriptionMode::default(),
            keep_mic_warm: true,
            retention_days: 30,
            max_total_mb: 4096,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranscriptionMode {
    #[default]
    Stable,
    Fast,
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

    /// 自訂詞庫與專有名詞修正
    #[serde(default)]
    pub vocabulary: VocabularyConfig,

    /// 各情境的輸出規則模板
    #[serde(default)]
    pub rules: OutputRulesConfig,

    /// 是否啟用語音指令模式
    #[serde(default = "default_voice_commands_enabled")]
    pub voice_commands_enabled: bool,
}

fn default_voice_commands_enabled() -> bool {
    true
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            buffer_mode: OutputBufferMode::Temporary,
            auto_inject_focused_window: true,
            restore_clipboard_after_inject: true,
            chinese_conversion: ChineseConversionMode::default(),
            manual_review_before_send: false,
            vocabulary: VocabularyConfig::default(),
            rules: OutputRulesConfig::default(),
            voice_commands_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabularyConfig {
    /// 轉錄後要套用的專有名詞清單
    #[serde(default)]
    pub entries: Vec<VocabularyEntry>,
}

impl Default for VocabularyConfig {
    fn default() -> Self {
        Self {
            entries: vec![VocabularyEntry::blank()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabularyEntry {
    /// 轉錄或簡繁轉換前可能出現的文字
    #[serde(default)]
    pub source: String,

    /// 最終要輸出的正式寫法；空白時沿用 source
    #[serde(default)]
    pub replacement: String,

    /// 保護此詞不被簡繁或用語轉換改動
    #[serde(default = "default_vocabulary_protect")]
    pub protect: bool,
}

impl VocabularyEntry {
    pub fn blank() -> Self {
        Self {
            source: String::new(),
            replacement: String::new(),
            protect: true,
        }
    }

    pub fn is_blank(&self) -> bool {
        self.source.trim().is_empty() && self.replacement.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputRulesConfig {
    #[serde(default = "default_chat_rules")]
    pub chat: ScenarioOutputRules,

    #[serde(default = "default_writing_rules")]
    pub writing: ScenarioOutputRules,

    #[serde(default = "default_code_rules")]
    pub code: ScenarioOutputRules,
}

impl Default for OutputRulesConfig {
    fn default() -> Self {
        Self {
            chat: default_chat_rules(),
            writing: default_writing_rules(),
            code: default_code_rules(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioOutputRules {
    #[serde(default = "default_true")]
    pub auto_punctuation: bool,

    #[serde(default)]
    pub format_paragraphs: bool,

    #[serde(default)]
    pub remove_fillers: bool,

    #[serde(default)]
    pub preserve_code_symbols: bool,

    #[serde(default)]
    pub auto_line_breaks: bool,

    /// 偵測「第一…第二…」「首先…其次…」等口述清單並排成條列
    #[serde(default = "default_true")]
    pub format_lists: bool,

    /// 移除口誤更正（「三點，不對，四點」→「四點」）
    #[serde(default = "default_true")]
    pub resolve_self_corrections: bool,

    /// 中英文之間自動加空白（「用GitHub」→「用 GitHub」）
    #[serde(default = "default_true")]
    pub cjk_spacing: bool,
}

impl ScenarioOutputRules {
    pub fn chat() -> Self {
        Self {
            auto_punctuation: true,
            format_paragraphs: false,
            remove_fillers: true,
            preserve_code_symbols: false,
            auto_line_breaks: false,
            format_lists: true,
            resolve_self_corrections: true,
            cjk_spacing: true,
        }
    }

    pub fn writing() -> Self {
        Self {
            auto_punctuation: true,
            format_paragraphs: true,
            remove_fillers: true,
            preserve_code_symbols: false,
            auto_line_breaks: true,
            format_lists: true,
            resolve_self_corrections: true,
            cjk_spacing: true,
        }
    }

    pub fn code() -> Self {
        Self {
            auto_punctuation: false,
            format_paragraphs: false,
            remove_fillers: false,
            preserve_code_symbols: true,
            auto_line_breaks: false,
            format_lists: false,
            resolve_self_corrections: true,
            cjk_spacing: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupConfig {
    /// 登入 Windows 後自動啟動 SpeakType
    #[serde(default)]
    pub launch_on_startup: bool,

    /// 自動啟動時直接進入系統匣
    #[serde(default = "default_true")]
    pub start_hidden_to_tray: bool,
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            launch_on_startup: false,
            start_hidden_to_tray: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChineseConversionMode {
    Disabled,
    #[default]
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
        if self.models_dir.as_deref().is_some_and(|d| d.trim() == "models" || d.trim().is_empty()) {
            self.models_dir = None;
        }
        if self.hotkeys.record_toggle.trim().eq_ignore_ascii_case("F2") {
            self.hotkeys.record_toggle = "Ctrl+Shift+L".to_string();
        }
        if self
            .hotkeys
            .record_toggle
            .trim()
            .eq_ignore_ascii_case("Ctrl+Shift+Win+L")
        {
            self.hotkeys.record_toggle = "Ctrl+Shift+L".to_string();
        }
        if self.output.vocabulary.entries.is_empty() {
            self.output
                .vocabulary
                .entries
                .push(VocabularyEntry::blank());
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
    /// Models directory. Relative paths used to resolve against the *current
    /// directory*, which is `C:\Windows\System32` when started from the Run key
    /// (autostart) — the model then "disappeared" and was downloaded again.
    /// Now: absolute config value > `<exe dir>\models` (portable installs that
    /// already contain models) > `%LOCALAPPDATA%\SpeakType\models`.
    pub fn get_models_dir(&self) -> String {
        if let Some(dir) = self.models_dir.as_ref().filter(|d| !d.trim().is_empty()) {
            let path = PathBuf::from(dir);
            if path.is_absolute() {
                return dir.clone();
            }
            if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) {
                return exe_dir.join(path).to_string_lossy().into_owned();
            }
        }
        if let Some(exe_models) = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.join("models")))
            .filter(|dir| has_model_files(dir))
        {
            return exe_models.to_string_lossy().into_owned();
        }
        paths::models_dir().to_string_lossy().into_owned()
    }

    /// 取得模型名稱
    pub fn get_model_name(&self) -> String {
        self.model_name
            .clone()
            .unwrap_or_else(|| models::DEFAULT_MODEL.to_string())
    }

    /// 取得預期模型檔路徑。whisper.cpp 模型檔通常以 ggml- 開頭。
    pub fn get_model_path(&self) -> PathBuf {
        let model_name = self.get_model_name();
        let catalog_path =
            PathBuf::from(self.get_models_dir()).join(models::catalog_entry(&model_name).file_name);
        if catalog_path.exists() {
            return catalog_path;
        }

        let models_dir = PathBuf::from(self.get_models_dir());
        let ggml_path = models_dir.join(format!("ggml-{}.bin", model_name));
        if ggml_path.exists() {
            return ggml_path;
        }

        let legacy_path = models_dir.join(format!("{}.bin", model_name));
        if legacy_path.exists() {
            return legacy_path;
        }

        catalog_path
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
    None
}

fn has_model_files(dir: &std::path::Path) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                e.path().extension().is_some_and(|ext| ext == "bin")
            })
        })
        .unwrap_or(false)
}

fn default_true_rec() -> bool {
    true
}

fn default_use_cuda() -> bool {
    true
}

fn default_sample_rate() -> u32 {
    16000
}

fn default_recording_gain() -> f32 {
    1.0
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

fn default_vocabulary_protect() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_chat_rules() -> ScenarioOutputRules {
    ScenarioOutputRules::chat()
}

fn default_writing_rules() -> ScenarioOutputRules {
    ScenarioOutputRules::writing()
}

fn default_code_rules() -> ScenarioOutputRules {
    ScenarioOutputRules::code()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_defaults_are_valid() {
        let config = AppConfig::default();
        assert!(config.use_cuda);
        assert_eq!(config.hotkeys.record_toggle, "Ctrl+Shift+L");
        assert!(config.hotkeys.hold_to_record);
        assert_eq!(config.recording.sample_rate, 16000);
        assert_eq!(config.recording.retention_days, 30);
        assert_eq!(config.recording.max_total_mb, 4096);
        assert!(config.output.auto_inject_focused_window);
        assert_eq!(config.output.chinese_conversion, ChineseConversionMode::TraditionalTaiwan);
        assert_eq!(config.output.buffer_mode, OutputBufferMode::Temporary);
    }

    #[test]
    fn toml_roundtrip() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.use_cuda, config.use_cuda);
        assert_eq!(parsed.hotkeys.record_toggle, config.hotkeys.record_toggle);
        assert_eq!(parsed.recording.gain, config.recording.gain);
        assert_eq!(parsed.output.chinese_conversion, config.output.chinese_conversion);
    }

    #[test]
    fn migrate_defaults_updates_legacy_hotkey() {
        let toml_str = r#"
            [hotkeys]
            record_toggle = "F2"
        "#;
        let mut config: AppConfig = toml::from_str(toml_str).unwrap();
        config.migrate_defaults();
        assert_eq!(config.hotkeys.record_toggle, "Ctrl+Shift+L");
    }

    #[test]
    fn migrate_defaults_fixes_old_win_key() {
        let toml_str = r#"
            [hotkeys]
            record_toggle = "Ctrl+Shift+Win+L"
        "#;
        let mut config: AppConfig = toml::from_str(toml_str).unwrap();
        config.migrate_defaults();
        assert_eq!(config.hotkeys.record_toggle, "Ctrl+Shift+L");
    }

    #[test]
    fn migrate_defaults_keeps_toggle_mode() {
        let toml_str = r#"
            [hotkeys]
            hold_to_record = false
        "#;
        let mut config: AppConfig = toml::from_str(toml_str).unwrap();
        config.migrate_defaults();
        assert!(!config.hotkeys.hold_to_record);
    }

    #[test]
    fn vocabulary_entry_blank_detection() {
        let blank = VocabularyEntry::blank();
        assert!(blank.is_blank());

        let filled = VocabularyEntry {
            source: "API".to_string(),
            replacement: "API".to_string(),
            protect: true,
        };
        assert!(!filled.is_blank());
    }

    #[test]
    fn scenario_output_rules_have_sensible_defaults() {
        let chat = ScenarioOutputRules::chat();
        assert!(chat.auto_punctuation);
        assert!(chat.remove_fillers);

        let code = ScenarioOutputRules::code();
        assert!(code.preserve_code_symbols);
        assert!(!code.auto_punctuation);

        let writing = ScenarioOutputRules::writing();
        assert!(writing.format_paragraphs);
    }
}


// ---------------------------------------------------------------------------
// v0.4 additions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptionConfig {
    /// "auto"（自動偵測，適合中英夾雜）、"zh"、"en"、"ja" ...
    #[serde(default = "default_language")]
    pub language: String,
    /// 依錄音長度縮短 encoder 視窗（短句大幅加速）
    #[serde(default = "default_true")]
    pub fast_encoder: bool,
    /// 執行緒數，0 = 自動
    #[serde(default)]
    pub threads: usize,
    /// 以提示詞引導輸出繁體中文
    #[serde(default = "default_true")]
    pub prefer_traditional_prompt: bool,
    /// 模型載入後先做一次暖機推論
    #[serde(default = "default_true")]
    pub warm_up: bool,
}

fn default_language() -> String {
    "auto".to_string()
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            language: default_language(),
            fast_encoder: true,
            threads: 0,
            prefer_traditional_prompt: true,
            warm_up: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InjectionMethod {
    /// 剪貼簿 + Ctrl+V（最快、最相容，會備份並還原剪貼簿）
    #[default]
    Paste,
    /// 剪貼簿 + Ctrl+Shift+V（部分終端機）
    PasteShift,
    /// SendInput Unicode 逐字輸入（不碰剪貼簿，適合終端機／遠端桌面）
    Unicode,
    /// 只複製到剪貼簿，不自動貼上
    ClipboardOnly,
}

impl InjectionMethod {
    pub fn all() -> [InjectionMethod; 4] {
        [Self::Paste, Self::PasteShift, Self::Unicode, Self::ClipboardOnly]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Paste => "貼上 (Ctrl+V)",
            Self::PasteShift => "貼上 (Ctrl+Shift+V)",
            Self::Unicode => "Unicode 打字",
            Self::ClipboardOnly => "只複製",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppRule {
    /// 程序名稱（不分大小寫，例如 "chrome.exe"、"LINE.exe"）
    pub process: String,
    /// 文字送出方式；未設定時用預設值
    #[serde(default)]
    pub method: Option<InjectionMethod>,
    /// 情境（聊天／寫作／程式碼），決定語氣與格式；未設定時用目前情境
    #[serde(default)]
    pub scenario: Option<String>,
    /// 給 AI 潤稿的額外語氣說明，例如「正式商務 email」
    #[serde(default)]
    pub tone: String,
    /// 換行要送 Shift+Enter（避免聊天軟體直接送出）
    #[serde(default)]
    pub newline_as_shift_enter: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InjectionConfig {
    #[serde(default)]
    pub default_method: InjectionMethod,
    /// 寫入剪貼簿後到按下 Ctrl+V 的等待（毫秒）
    #[serde(default = "default_paste_delay_ms")]
    pub paste_delay_ms: u64,
    /// 貼上後多久還原剪貼簿（毫秒）；太短時慢的 App 會貼到舊內容
    #[serde(default = "default_restore_delay_ms")]
    pub restore_delay_ms: u64,
    /// 放開快捷鍵時記住前景視窗，送出前若焦點改變則切回
    #[serde(default = "default_true")]
    pub refocus_original_window: bool,
    #[serde(default = "default_app_rules")]
    pub app_rules: Vec<AppRule>,
}

fn default_paste_delay_ms() -> u64 {
    15
}

fn default_restore_delay_ms() -> u64 {
    300
}

pub fn default_app_rules() -> Vec<AppRule> {
    let rule = |process: &str, method: Option<InjectionMethod>, scenario: Option<&str>, shift_enter: bool| AppRule {
        process: process.to_string(),
        method,
        scenario: scenario.map(str::to_string),
        tone: String::new(),
        newline_as_shift_enter: shift_enter,
    };
    vec![
        rule("WindowsTerminal.exe", Some(InjectionMethod::Paste), Some("程式碼"), false),
        rule("cmd.exe", Some(InjectionMethod::Unicode), Some("程式碼"), false),
        rule("powershell.exe", Some(InjectionMethod::Unicode), Some("程式碼"), false),
        rule("pwsh.exe", Some(InjectionMethod::Unicode), Some("程式碼"), false),
        rule("mintty.exe", Some(InjectionMethod::PasteShift), Some("程式碼"), false),
        rule("Code.exe", None, Some("程式碼"), false),
        rule("Cursor.exe", None, Some("程式碼"), false),
        rule("mstsc.exe", Some(InjectionMethod::Unicode), None, false),
        rule("LINE.exe", Some(InjectionMethod::Paste), Some("聊天"), true),
        rule("Slack.exe", Some(InjectionMethod::Paste), Some("聊天"), true),
        rule("Discord.exe", Some(InjectionMethod::Paste), Some("聊天"), true),
        rule("Teams.exe", Some(InjectionMethod::Paste), Some("聊天"), true),
        rule("ms-teams.exe", Some(InjectionMethod::Paste), Some("聊天"), true),
        rule("WINWORD.EXE", Some(InjectionMethod::Paste), Some("寫作"), false),
        rule("OUTLOOK.EXE", Some(InjectionMethod::Paste), Some("寫作"), false),
        rule("notepad.exe", Some(InjectionMethod::Paste), None, false),
    ]
}

impl Default for InjectionConfig {
    fn default() -> Self {
        Self {
            default_method: InjectionMethod::Paste,
            paste_delay_ms: default_paste_delay_ms(),
            restore_delay_ms: default_restore_delay_ms(),
            refocus_original_window: true,
            app_rules: default_app_rules(),
        }
    }
}

impl InjectionConfig {
    pub fn rule_for(&self, process: Option<&str>) -> Option<&AppRule> {
        let process = process?.rsplit(['\\', '/']).next()?;
        self.app_rules
            .iter()
            .find(|rule| rule.process.trim().eq_ignore_ascii_case(process))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiCleanupConfig {
    /// 預設關閉；開啟後在本地規則之後送給 LLM 潤稿
    #[serde(default)]
    pub enabled: bool,
    /// OpenAI 相容 chat/completions 端點（Ollama: http://localhost:11434/v1/chat/completions）
    #[serde(default = "default_ai_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_ai_model")]
    pub model: String,
    /// API 金鑰（本機 Ollama 可留空）；也可用環境變數 SPEAKTYPE_AI_API_KEY
    #[serde(default)]
    pub api_key: String,
    /// 逾時（毫秒），逾時就直接送出本地清理結果
    #[serde(default = "default_ai_timeout_ms")]
    pub timeout_ms: u64,
    /// 太短的句子不送 LLM（字元數）
    #[serde(default = "default_ai_min_chars")]
    pub min_chars: usize,
    /// 使用者自訂指示
    #[serde(default)]
    pub custom_instructions: String,
}

fn default_ai_endpoint() -> String {
    "http://localhost:11434/v1/chat/completions".to_string()
}

fn default_ai_model() -> String {
    "qwen2.5:3b".to_string()
}

fn default_ai_timeout_ms() -> u64 {
    5000
}

fn default_ai_min_chars() -> usize {
    8
}

impl Default for AiCleanupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_ai_endpoint(),
            model: default_ai_model(),
            api_key: String::new(),
            timeout_ms: default_ai_timeout_ms(),
            min_chars: default_ai_min_chars(),
            custom_instructions: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndicatorConfig {
    /// 錄音／轉錄時顯示螢幕底部的小浮動指示器
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for IndicatorConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}
