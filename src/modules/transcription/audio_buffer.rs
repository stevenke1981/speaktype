/// 簡單的音訊緩衝區工具
/// 負責把 cpal 錄到的音訊轉成 Whisper 需要的格式（16kHz, mono, f32）
pub struct AudioBuffer {
    samples: Vec<f32>,
    sample_rate: u32,
}

impl AudioBuffer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            samples: Vec::new(),
            sample_rate,
        }
    }

    pub fn push(&mut self, new_samples: &[f32]) {
        self.samples.extend_from_slice(new_samples);
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// 回傳適合 Whisper 的音訊資料
    pub fn as_whisper_input(&self) -> &[f32] {
        &self.samples
    }

    /// 如果需要重採樣（目前假設 cpal 已經是 16kHz）
    pub fn resample_if_needed(&self) -> Vec<f32> {
        // TODO: 如果 cpal 不是 16kHz，在這裡加入重採樣邏輯
        self.samples.clone()
    }
}
