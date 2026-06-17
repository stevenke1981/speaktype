use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub fn new(model_path: &Path, use_gpu: bool) -> Result<Self, String> {
        let params = WhisperContextParameters {
            use_gpu,
            ..Default::default()
        };
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().ok_or_else(|| "模型路徑包含無效 Unicode".to_string())?,
            params,
        )
        .map_err(|e| format!("模型載入失敗: {}", e))?;
        Ok(Self { ctx })
    }

    pub fn transcribe(&self, audio_samples: &[f32]) -> Result<String, String> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| format!("建立轉錄狀態失敗: {}", e))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("zh"));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state
            .full(params, audio_samples)
            .map_err(|e| format!("轉錄失敗: {}", e))?;

        let num_segments = state.full_n_segments();
        let mut result = String::new();

        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                result.push_str(segment.to_str().unwrap_or(""));
            }
            result.push(' ');
        }

        Ok(result.trim().to_string())
    }
}