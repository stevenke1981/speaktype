// history/mod.rs - 歷史紀錄模組
// 職責：儲存辨識結果、時間戳、情境

use crate::modules::error::log_error;
use crate::modules::paths;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const HISTORY_FILE: &str = "history.json";

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RecognitionRecord {
    pub timestamp: DateTime<Local>,
    pub text: String,
    pub scenario: String,
    pub duration_sec: f32,
    /// Target application (process name), if known.
    #[serde(default)]
    pub app: Option<String>,
    /// Hotkey release → text inserted, in milliseconds.
    #[serde(default)]
    pub latency_ms: Option<f64>,
    /// Whisper output before cleanup.
    #[serde(default)]
    pub raw_text: Option<String>,
}

pub struct HistoryManager {
    records: Vec<RecognitionRecord>,
    max_records: usize,
}

impl HistoryManager {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            max_records: 200,
        }
    }

    pub fn load() -> Self {
        let Some(path) = Self::history_path() else {
            return Self::new();
        };

        if !path.exists() {
            return Self::new();
        }

        match fs::read_to_string(&path)
            .map_err(|err| err.to_string())
            .and_then(|content| {
                serde_json::from_str::<Vec<RecognitionRecord>>(&content)
                    .map_err(|err| err.to_string())
            }) {
            Ok(records) => {
                let mut manager = Self::new();
                manager.records = records;
                manager.records.truncate(manager.max_records);
                manager
            }
            Err(err) => {
                log_error("history load", format!("{}: {}", path.display(), err));
                Self::new()
            }
        }
    }

    pub fn add_record(&mut self, text: String, scenario: String, duration_sec: f32) {
        self.add_entry(RecognitionRecord {
            timestamp: Local::now(),
            text,
            scenario,
            duration_sec,
            app: None,
            latency_ms: None,
            raw_text: None,
        });
    }

    pub fn add_entry(&mut self, record: RecognitionRecord) {
        self.records.insert(0, record);

        // 超過上限就刪除最舊的
        if self.records.len() > self.max_records {
            self.records.truncate(self.max_records);
        }

        self.save();
    }

    pub fn records(&self) -> &[RecognitionRecord] {
        &self.records
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.save();
    }

    pub fn history_path() -> Option<PathBuf> {
        Some(paths::data_dir().join(HISTORY_FILE))
    }

    fn save(&self) {
        let Some(path) = Self::history_path() else {
            return;
        };

        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                log_error("history save", format!("{}: {}", parent.display(), err));
                return;
            }
        }

        // Serialise here, write on a background thread (keeps the UI responsive).
        let content = match serde_json::to_string(&self.records) {
            Ok(content) => content,
            Err(err) => return log_error("history save", err),
        };
        std::thread::spawn(move || {
            let tmp = path.with_extension("json.tmp");
            let result = fs::write(&tmp, content).and_then(|_| fs::rename(&tmp, &path));
            if let Err(err) = result {
                log_error("history save", format!("{}: {}", path.display(), err));
            }
        });
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new()
    }
}
