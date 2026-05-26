use anyhow::Result;
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub fn new(model_path: &Path, use_gpu: bool) -> Result<Self> {
        let params = WhisperContextParameters {
            use_gpu,
            ..Default::default()
        };
        let ctx = WhisperContext::new_with_params(model_path.to_str().unwrap(), params)?;
        Ok(Self { ctx })
    }

    /// 將錄音 buffer 轉成文字
    pub fn transcribe(&self, audio_samples: &[f32]) -> Result<String> {
        let mut state = self.ctx.create_state()?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("zh"));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state.full(params, audio_samples)?;

        let num_segments = state.full_n_segments();
        let mut result = String::new();

        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                result.push_str(&segment.to_str_lossy()?);
            }
            result.push(' ');
        }

        Ok(result.trim().to_string())
    }
}
