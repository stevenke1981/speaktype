use crate::modules::audio::{RecordedAudio, Recorder};
use crate::modules::config::{OutputBufferMode, OutputConfig};
use crate::modules::error::log_error;
use crate::modules::input::InputController;
use crate::modules::text_conversion::convert_chinese_text;
use crate::modules::transcription::Transcriber;
use chrono::Local;
use reqwest::blocking::get;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Instant;

const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub struct SpeakTypeEngine {
    recorder: Recorder,
    transcriber: Option<Transcriber>,
    model_path: PathBuf,
    use_cuda: bool,
    model_error: Option<String>,
    input: InputController,
    recording: bool,
    recording_start: Option<Instant>,
    last_recording_path: Option<PathBuf>,
    last_recording_duration_sec: f32,
}

impl SpeakTypeEngine {
    pub fn new(model_path: PathBuf, use_cuda: bool) -> Self {
        let (transcriber, model_error) = match ensure_model_file(&model_path) {
            Ok(()) => match Transcriber::new(&model_path, use_cuda) {
                Ok(transcriber) => (Some(transcriber), None),
                Err(err) => {
                    log_error("model load", &err);
                    (None, Some(format!("模型載入失敗: {}", err)))
                }
            },
            Err(err) => {
                log_error("model download", &err);
                (None, Some(format!("模型下載失敗: {}", err)))
            }
        };

        Self {
            recorder: Recorder::new(),
            transcriber,
            model_path,
            use_cuda,
            model_error,
            input: InputController::new(),
            recording: false,
            recording_start: None,
            last_recording_path: None,
            last_recording_duration_sec: 0.0,
        }
    }

    pub fn reload_model(&mut self, model_path: PathBuf, use_cuda: bool) -> Result<(), String> {
        if self.recording {
            return Err("錄音中無法重新載入模型".to_string());
        }

        ensure_model_file(&model_path).inspect_err(|err| log_error("model download", err))?;
        let transcriber = Transcriber::new(&model_path, use_cuda).map_err(|err| {
            log_error("model reload", &err);
            format!("模型載入失敗: {}", err)
        })?;

        self.transcriber = Some(transcriber);
        self.model_path = model_path;
        self.use_cuda = use_cuda;
        self.model_error = None;
        Ok(())
    }

    pub fn toggle_recording(&mut self, output: &OutputConfig) -> Result<Option<String>, String> {
        if self.recording {
            self.stop_recording_and_transcribe(output).map(Some)
        } else {
            self.start_recording()?;
            Ok(None)
        }
    }

    pub fn start_recording(&mut self) -> Result<(), String> {
        if self.recording {
            return Ok(());
        }

        self.recording_start = Some(Instant::now());
        self.recorder
            .start_recording()
            .map_err(|e| format!("無法開始錄音: {}", e))?;
        self.recording = true;
        Ok(())
    }

    pub fn stop_recording_and_transcribe(
        &mut self,
        output: &OutputConfig,
    ) -> Result<String, String> {
        if !self.recording {
            return Err("目前沒有正在錄音".to_string());
        }

        self.recording = false;
        let duration = self
            .recording_start
            .take()
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        self.last_recording_duration_sec = duration;

        let audio_data = self.recorder.stop_recording();
        if audio_data.is_empty() {
            return Err("沒有錄到音訊".to_string());
        }

        let recording_path = save_recording_wav(&audio_data).inspect_err(|err| {
            log_error("recording save", err);
        })?;
        self.last_recording_path = Some(recording_path);

        let transcriber = self.transcriber.as_ref().ok_or_else(|| {
            self.model_error
                .clone()
                .unwrap_or_else(|| "模型尚未載入".to_string())
        })?;
        let whisper_audio = audio_data.to_whisper_mono_16khz();

        let text = transcriber
            .transcribe(&whisper_audio)
            .inspect_err(|err| log_error("transcription", err))
            .map_err(|e| format!("轉錄失敗: {}", e))?;
        let text = normalize_transcription_text(&text);
        let text = convert_chinese_text(&text, output.chinese_conversion)
            .map_err(|e| format!("簡繁用語轉換失敗: {}", e))?;

        if text.is_empty() {
            return Err("辨識結果為空".to_string());
        }

        self.deliver_text(&text, output)
            .inspect_err(|err| log_error("text delivery", err))?;
        Ok(text)
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn model_status_text(&self) -> String {
        if self.transcriber.is_some() {
            let acceleration = if self.use_cuda { "CUDA" } else { "CPU" };
            format!(
                "{}：已載入 ({acceleration})",
                self.model_path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("模型")
                    .trim_start_matches("ggml-")
            )
        } else {
            self.model_error
                .clone()
                .unwrap_or_else(|| "模型尚未載入".to_string())
        }
    }

    pub fn model_error(&self) -> Option<&str> {
        self.model_error.as_deref()
    }

    pub fn last_recording_path(&self) -> Option<&Path> {
        self.last_recording_path.as_deref()
    }

    pub fn last_recording_duration_sec(&self) -> f32 {
        self.last_recording_duration_sec
    }

    fn deliver_text(&mut self, text: &str, output: &OutputConfig) -> Result<(), String> {
        match output.buffer_mode {
            OutputBufferMode::Clipboard => {
                if output.auto_inject_focused_window {
                    self.input.inject_via_clipboard(text, false)
                } else {
                    self.input.copy_to_clipboard(text)
                }
            }
            OutputBufferMode::Temporary => {
                if output.auto_inject_focused_window {
                    self.input
                        .inject_via_clipboard(text, output.restore_clipboard_after_inject)
                } else {
                    Ok(())
                }
            }
        }
        .map_err(|e| format!("文字輸出失敗: {}", e))
    }
}

fn save_recording_wav(audio: &RecordedAudio) -> Result<PathBuf, String> {
    let recordings_dir = PathBuf::from("recordings");
    fs::create_dir_all(&recordings_dir).map_err(|err| err.to_string())?;

    let file_name = format!("recording_{}.wav", Local::now().format("%Y%m%d_%H%M%S_%3f"));
    let path = recordings_dir.join(file_name);
    let spec = hound::WavSpec {
        channels: audio.channels.max(1),
        sample_rate: audio.sample_rate.max(1),
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut writer = hound::WavWriter::create(&path, spec)
        .map_err(|err| format!("無法建立錄音檔 {}: {}", path.display(), err))?;
    for sample in &audio.samples {
        writer
            .write_sample(sample.clamp(-1.0, 1.0))
            .map_err(|err| format!("寫入錄音檔失敗: {}", err))?;
    }
    writer
        .finalize()
        .map_err(|err| format!("完成錄音檔失敗: {}", err))?;

    Ok(path)
}

fn normalize_transcription_text(text: &str) -> String {
    text.chars()
        .filter(|ch| *ch != '\0' && *ch != '\u{feff}' && *ch != '\u{fffd}')
        .collect::<String>()
        .trim()
        .to_string()
}

fn ensure_model_file(model_path: &Path) -> Result<(), String> {
    if model_path.exists() {
        return Ok(());
    }

    let parent = model_path
        .parent()
        .ok_or_else(|| format!("模型路徑無效: {}", model_path.display()))?;
    fs::create_dir_all(parent).map_err(|err| err.to_string())?;

    let file_name = model_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("模型檔名無效: {}", model_path.display()))?;
    let url = format!("{}/{}", MODEL_BASE_URL, file_name);
    let temp_path = model_path.with_extension("bin.part");

    println!("[model] 找不到模型，開始下載: {}", url);
    let mut response = get(&url)
        .map_err(|err| format!("無法連線到模型來源: {}", err))?
        .error_for_status()
        .map_err(|err| format!("模型來源回應失敗: {}", err))?;

    let mut output = File::create(&temp_path)
        .map_err(|err| format!("無法建立暫存模型檔 {}: {}", temp_path.display(), err))?;
    response
        .copy_to(&mut output)
        .map_err(|err| format!("下載模型資料失敗: {}", err))?;

    fs::rename(&temp_path, model_path).map_err(|err| {
        let _ = fs::remove_file(&temp_path);
        format!("無法寫入模型檔 {}: {}", model_path.display(), err)
    })?;

    println!("[model] 模型下載完成: {}", model_path.display());
    Ok(())
}
