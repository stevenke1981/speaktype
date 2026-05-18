// history/mod.rs - 歷史紀錄模組
// 職責：儲存辨識結果、時間戳、情境

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RecognitionRecord {
    pub timestamp: DateTime<Local>,
    pub text: String,
    pub scenario: String,
    pub duration_sec: f32,
}

pub struct HistoryManager {
    records: Vec<RecognitionRecord>,
    max_records: usize,
}

impl HistoryManager {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            max_records: 20,
        }
    }

    pub fn add_record(&mut self, text: String, scenario: String, duration_sec: f32) {
        let record = RecognitionRecord {
            timestamp: Local::now(),
            text,
            scenario,
            duration_sec,
        };

        self.records.insert(0, record);

        // 超過上限就刪除最舊的
        if self.records.len() > self.max_records {
            self.records.truncate(self.max_records);
        }
    }

    pub fn records(&self) -> &[RecognitionRecord] {
        &self.records
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new()
    }
}
