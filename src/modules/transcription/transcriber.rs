//! Local whisper.cpp transcription.
//!
//! Speed notes (see `docs/PERFORMANCE.md`):
//! * the `WhisperState` (KV cache / compute buffers) is created once and reused,
//! * the encoder window (`audio_ctx`) is shrunk to the clip length instead of the
//!   fixed 30 s window, which is the single biggest CPU win for short dictation,
//! * timestamps are disabled and a single segment is produced,
//! * all physical cores are used (whisper.cpp defaults to 4),
//! * a 1 s warm-up pass runs right after loading so the first real dictation
//!   does not pay for buffer allocation.
use std::path::Path;
use std::sync::Mutex;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

/// Per-call decoding options.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscribeOptions {
    /// Whisper language code ("zh", "en", ...) or "auto" for detection.
    pub language: String,
    /// Prompt used to bias spelling / script (e.g. Traditional Chinese, dictionary terms).
    pub initial_prompt: String,
    /// Shrink the encoder window to the clip length (much faster for short clips).
    pub fast_encoder: bool,
    /// Worker threads; 0 = automatic.
    pub threads: usize,
}

impl Default for TranscribeOptions {
    fn default() -> Self {
        Self {
            language: "auto".to_string(),
            initial_prompt: String::new(),
            fast_encoder: true,
            threads: 0,
        }
    }
}

pub struct Transcriber {
    // Field order matters: the state must be dropped before the context.
    state: Mutex<WhisperState>,
    _ctx: WhisperContext,
}

const SAMPLE_RATE: usize = 16_000;
/// Whisper encoder frames per second of audio (1500 frames / 30 s).
const FRAMES_PER_SEC: usize = 50;

impl Transcriber {
    pub fn new(model_path: &Path, use_gpu: bool) -> Result<Self, String> {
        let mut params = WhisperContextParameters::default();
        params.use_gpu = use_gpu;
        params.flash_attn = use_gpu;
        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| "模型路徑包含無效 Unicode".to_string())?,
            params,
        )
        .map_err(|e| format!("模型載入失敗: {}", e))?;
        let state = ctx
            .create_state()
            .map_err(|e| format!("建立轉錄狀態失敗: {}", e))?;
        Ok(Self {
            state: Mutex::new(state),
            _ctx: ctx,
        })
    }

    /// Run a short silent pass so the first real dictation is not slower.
    pub fn warm_up(&self) {
        let silence = vec![0.0_f32; SAMPLE_RATE];
        let _ = self.transcribe_with(
            &silence,
            &TranscribeOptions {
                language: "en".into(),
                ..Default::default()
            },
        );
    }

    /// Backwards compatible helper (Chinese, default options).
    pub fn transcribe(&self, audio_samples: &[f32]) -> Result<String, String> {
        self.transcribe_with(audio_samples, &TranscribeOptions::default())
    }

    pub fn transcribe_with(
        &self,
        audio_samples: &[f32],
        options: &TranscribeOptions,
    ) -> Result<String, String> {
        if audio_samples.is_empty() {
            return Ok(String::new());
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let language = options.language.trim();
        if language.is_empty() || language.eq_ignore_ascii_case("auto") {
            params.set_language(Some("auto"));
        } else {
            params.set_language(Some(language));
        }
        params.set_n_threads(resolve_threads(options.threads) as i32);
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_no_timestamps(true);
        params.set_suppress_blank(true);
        params.set_no_speech_thold(0.6);
        let seconds = audio_samples.len() / SAMPLE_RATE;
        if seconds < 30 {
            params.set_single_segment(true);
        }
        if options.fast_encoder {
            if let Some(ctx) = fast_audio_ctx(audio_samples.len()) {
                params.set_audio_ctx(ctx);
            }
        }
        if !options.initial_prompt.trim().is_empty() {
            params.set_initial_prompt(options.initial_prompt.trim());
        }

        state
            .full(params, audio_samples)
            .map_err(|e| format!("轉錄失敗: {}", e))?;

        let mut result = String::new();
        for i in 0..state.full_n_segments() {
            if let Some(segment) = state.get_segment(i) {
                let text = segment.to_str_lossy().map(|s| s.to_string()).unwrap_or_default();
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                if !result.is_empty() {
                    result.push(' ');
                }
                result.push_str(text);
            }
        }
        Ok(result)
    }
}

/// Encoder window for a clip of `samples` 16 kHz samples, with head-room.
/// Returns `None` when the full 30 s window should be used.
pub fn fast_audio_ctx(samples: usize) -> Option<i32> {
    let frames = samples.div_ceil(SAMPLE_RATE / FRAMES_PER_SEC);
    // ~1.3 s of padding and a floor keep short clips from hallucinating.
    let ctx = (frames + 64).max(384);
    if ctx >= 1500 {
        None
    } else {
        Some(ctx.next_multiple_of(64) as i32)
    }
}

pub fn resolve_threads(requested: usize) -> usize {
    if requested > 0 {
        return requested;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_ctx_scales_with_clip() {
        assert_eq!(fast_audio_ctx(SAMPLE_RATE), Some(384));
        assert_eq!(fast_audio_ctx(SAMPLE_RATE * 10), Some(576));
        assert_eq!(fast_audio_ctx(SAMPLE_RATE * 29), None);
    }

    #[test]
    fn threads_auto_is_bounded() {
        let t = resolve_threads(0);
        assert!((1..=8).contains(&t));
        assert_eq!(resolve_threads(3), 3);
    }
}
