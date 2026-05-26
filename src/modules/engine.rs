use crate::modules::audio::{RecordedAudio, Recorder};
use crate::modules::config::{OutputBufferMode, OutputConfig};
use crate::modules::error::log_error;
use crate::modules::input::InputController;
use crate::modules::paths;
use crate::modules::scenario::Scenario;
use crate::modules::text_conversion::convert_chinese_text;
use crate::modules::transcription::Transcriber;
use chrono::Local;
use reqwest::blocking::Client;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
const DOWNLOAD_CHUNK_SIZE: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ModelDownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_sec: f64,
    pub url: String,
}

#[derive(Clone, Debug)]
pub enum ModelEvent {
    Progress(ModelDownloadProgress),
    Ready,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug)]
pub enum TranscriptionEvent {
    Status(String),
    Completed(TranscriptionResult),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct TranscriptionRequest {
    pub audio: RecordedAudio,
    pub output: OutputConfig,
    pub model_path: PathBuf,
    pub use_cuda: bool,
    pub scenario: Scenario,
    pub duration_sec: f32,
}

#[derive(Clone, Debug)]
pub struct TranscriptionResult {
    pub text: String,
    pub recording_path: PathBuf,
    pub duration_sec: f32,
    pub delivered: bool,
}

pub struct SpeakTypeEngine {
    recorder: Recorder,
    model_path: PathBuf,
    use_cuda: bool,
    model_ready: bool,
    model_error: Option<String>,
    input: InputController,
    recording: bool,
    recording_start: Option<Instant>,
    last_recording_path: Option<PathBuf>,
    last_recording_duration_sec: f32,
}

impl SpeakTypeEngine {
    pub fn new(model_path: PathBuf, use_cuda: bool) -> Self {
        let model_ready = model_path.exists();
        let model_error = if model_ready {
            None
        } else {
            Some("模型尚未下載或尚未準備完成".to_string())
        };

        Self {
            recorder: Recorder::new(),
            model_path,
            use_cuda,
            model_ready,
            model_error,
            input: InputController::new(),
            recording: false,
            recording_start: None,
            last_recording_path: None,
            last_recording_duration_sec: 0.0,
        }
    }

    pub fn set_model_path(&mut self, model_path: PathBuf, use_cuda: bool) {
        self.model_path = model_path;
        self.use_cuda = use_cuda;
        self.model_ready = self.model_path.exists();
        self.model_error = if self.model_ready {
            None
        } else {
            Some("模型尚未下載或尚未準備完成".to_string())
        };
    }

    pub fn mark_model_ready(&mut self) {
        self.model_ready = true;
        self.model_error = None;
    }

    pub fn mark_model_failed(&mut self, error: String) {
        self.model_ready = false;
        self.model_error = Some(error);
    }

    pub fn toggle_recording(&mut self) -> Result<Option<TranscriptionRequest>, String> {
        if self.recording {
            self.stop_recording_capture().map(Some)
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
            .map_err(|e| classify_recording_error(&e.to_string()))?;
        self.recording = true;
        Ok(())
    }

    pub fn stop_recording_capture(&mut self) -> Result<TranscriptionRequest, String> {
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

        let audio = self.recorder.stop_recording();
        if audio.is_empty() {
            return Err("沒有錄到音訊，請確認麥克風權限與輸入裝置".to_string());
        }
        if duration < 0.25 {
            return Err("錄音太短，請按住久一點再放開".to_string());
        }
        if !self.model_ready || !self.model_path.exists() {
            return Err(self
                .model_error
                .clone()
                .unwrap_or_else(|| "模型尚未載入，請先等待下載完成或按重試".to_string()));
        }

        Ok(TranscriptionRequest {
            audio,
            output: OutputConfig::default(),
            model_path: self.model_path.clone(),
            use_cuda: self.use_cuda,
            scenario: Scenario::Chat,
            duration_sec: duration,
        })
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn model_status_text(&self) -> String {
        if self.model_ready {
            let acceleration = if self.use_cuda { "CUDA" } else { "CPU" };
            format!(
                "{}：已準備 ({acceleration})",
                self.model_path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("模型")
                    .trim_start_matches("ggml-")
            )
        } else {
            self.model_error
                .clone()
                .unwrap_or_else(|| "模型尚未準備完成".to_string())
        }
    }

    pub fn model_error(&self) -> Option<&str> {
        self.model_error.as_deref()
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    pub fn use_cuda(&self) -> bool {
        self.use_cuda
    }

    pub fn last_recording_path(&self) -> Option<&Path> {
        self.last_recording_path.as_deref()
    }

    pub fn set_last_recording_path(&mut self, path: PathBuf) {
        self.last_recording_path = Some(path);
    }

    pub fn last_recording_duration_sec(&self) -> f32 {
        self.last_recording_duration_sec
    }

    pub fn deliver_text_now(&mut self, text: &str, output: &OutputConfig) -> Result<(), String> {
        deliver_text_with_controller(&mut self.input, text, output)
    }
}

pub fn prepare_model_with_progress(
    model_path: &Path,
    use_cuda: bool,
    cancel: Arc<AtomicBool>,
    mut on_event: impl FnMut(ModelEvent),
) {
    match ensure_model_file(model_path, cancel, |progress| {
        on_event(ModelEvent::Progress(progress));
    })
    .and_then(|_| {
        Transcriber::new(model_path, use_cuda)
            .map(|_| ())
            .map_err(|err| format!("模型載入失敗: {}", err))
    }) {
        Ok(()) => on_event(ModelEvent::Ready),
        Err(err) if err == "cancelled" => on_event(ModelEvent::Cancelled),
        Err(err) => {
            log_error("model prepare", &err);
            on_event(ModelEvent::Failed(err));
        }
    }
}

pub fn run_transcription_request(
    mut request: TranscriptionRequest,
    mut on_event: impl FnMut(TranscriptionEvent),
) {
    on_event(TranscriptionEvent::Status("儲存錄音檔...".to_string()));

    let recording_path = match save_recording_wav(&request.audio) {
        Ok(path) => path,
        Err(err) => {
            log_error("recording save", &err);
            on_event(TranscriptionEvent::Failed(err));
            return;
        }
    };

    on_event(TranscriptionEvent::Status("載入模型...".to_string()));
    let transcriber = match Transcriber::new(&request.model_path, request.use_cuda) {
        Ok(transcriber) => transcriber,
        Err(err) => {
            let error = format!("模型載入失敗: {}", err);
            log_error("model load", &error);
            on_event(TranscriptionEvent::Failed(error));
            return;
        }
    };

    on_event(TranscriptionEvent::Status("轉錄中...".to_string()));
    let whisper_audio = request.audio.to_whisper_mono_16khz();
    let text = match transcriber.transcribe(&whisper_audio) {
        Ok(text) => text,
        Err(err) => {
            let error = format!("轉錄失敗: {}", err);
            log_error("transcription", &error);
            on_event(TranscriptionEvent::Failed(error));
            return;
        }
    };

    let text = normalize_transcription_text(&text);
    let text = match convert_chinese_text(&text, request.output.chinese_conversion) {
        Ok(text) => request.scenario.postprocess(&text),
        Err(err) => {
            let error = format!("簡繁用語轉換失敗: {}", err);
            log_error("text conversion", &error);
            on_event(TranscriptionEvent::Failed(error));
            return;
        }
    };

    if text.is_empty() {
        on_event(TranscriptionEvent::Failed("辨識結果為空".to_string()));
        return;
    }

    let delivered = if request.output.manual_review_before_send {
        false
    } else {
        on_event(TranscriptionEvent::Status("輸出文字...".to_string()));
        let mut input = InputController::new();
        match deliver_text_with_controller(&mut input, &text, &request.output) {
            Ok(()) => true,
            Err(err) => {
                log_error("text delivery", &err);
                on_event(TranscriptionEvent::Failed(err));
                return;
            }
        }
    };

    request.audio.samples.clear();
    on_event(TranscriptionEvent::Completed(TranscriptionResult {
        text,
        recording_path,
        duration_sec: request.duration_sec,
        delivered,
    }));
}

fn deliver_text_with_controller(
    input: &mut InputController,
    text: &str,
    output: &OutputConfig,
) -> Result<(), String> {
    match output.buffer_mode {
        OutputBufferMode::Clipboard => {
            if output.auto_inject_focused_window {
                input.inject_via_clipboard(text, false)
            } else {
                input.copy_to_clipboard(text)
            }
        }
        OutputBufferMode::Temporary => {
            if output.auto_inject_focused_window {
                input.inject_via_clipboard(text, output.restore_clipboard_after_inject)
            } else {
                Ok(())
            }
        }
    }
    .map_err(|e| format!("文字輸出失敗: {}", e))
}

fn save_recording_wav(audio: &RecordedAudio) -> Result<PathBuf, String> {
    let recordings_dir = paths::recordings_dir();
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

fn ensure_model_file(
    model_path: &Path,
    cancel: Arc<AtomicBool>,
    mut on_progress: impl FnMut(ModelDownloadProgress),
) -> Result<(), String> {
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

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| format!("建立下載器失敗: {}", err))?;
    let mut response = client
        .get(&url)
        .send()
        .map_err(|err| format!("無法連線到模型來源: {}", err))?
        .error_for_status()
        .map_err(|err| format!("模型來源回應失敗: {}", err))?;

    let total_bytes = response.content_length();
    let mut output = File::create(&temp_path)
        .map_err(|err| format!("無法建立暫存模型檔 {}: {}", temp_path.display(), err))?;
    let mut buffer = vec![0_u8; DOWNLOAD_CHUNK_SIZE];
    let started = Instant::now();
    let mut downloaded_bytes = 0_u64;

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = fs::remove_file(&temp_path);
            return Err("cancelled".to_string());
        }

        let read = response
            .read(&mut buffer)
            .map_err(|err| format!("下載模型資料失敗: {}", err))?;
        if read == 0 {
            break;
        }

        output
            .write_all(&buffer[..read])
            .map_err(|err| format!("寫入暫存模型檔失敗: {}", err))?;
        downloaded_bytes += read as u64;
        let elapsed = started.elapsed().as_secs_f64().max(0.001);
        on_progress(ModelDownloadProgress {
            downloaded_bytes,
            total_bytes,
            speed_bytes_per_sec: downloaded_bytes as f64 / elapsed,
            url: url.clone(),
        });
    }

    fs::rename(&temp_path, model_path).map_err(|err| {
        let _ = fs::remove_file(&temp_path);
        format!("無法寫入模型檔 {}: {}", model_path.display(), err)
    })?;

    Ok(())
}

fn classify_recording_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("access") || lower.contains("permission") {
        "無法開始錄音：麥克風權限可能未開啟".to_string()
    } else if lower.contains("device") || lower.contains("input") {
        "無法開始錄音：找不到可用麥克風".to_string()
    } else {
        format!("無法開始錄音: {error}")
    }
}
