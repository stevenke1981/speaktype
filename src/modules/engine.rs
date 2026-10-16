use crate::modules::audio::{RecordedAudio, Recorder};
use crate::modules::ai_cleanup::{self, AiContext};
use crate::modules::config::{
    AiCleanupConfig, ChineseConversionMode, InjectionConfig, InjectionMethod, OutputBufferMode,
    OutputConfig, TranscriptionConfig, TranscriptionMode,
};
use crate::modules::inject::{self, ForegroundApp, InjectPlan};
use crate::modules::transcription::TranscribeOptions;
use crate::modules::error::log_error;
use crate::modules::input::InputController;
use crate::modules::models;
use crate::modules::paths;
use crate::modules::postprocess::postprocess_transcription;
use crate::modules::voice_commands::{execute_voice_command, parse_voice_command, voice_command_label};
use crate::modules::scenario::Scenario;
use crate::modules::transcription::Transcriber;
use chrono::Local;
use reqwest::blocking::Client;
use reqwest::header::RANGE;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    Warning(String),
    Failed(String),
}

#[derive(Clone, Debug)]
pub enum TranscriptionEvent {
    Status(String),
    Draft(String),
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
    pub mode: TranscriptionMode,
    pub duration_sec: f32,
    pub transcription: TranscriptionConfig,
    pub injection: InjectionConfig,
    pub ai: AiCleanupConfig,
    /// Window focused when the hotkey was released (text goes there).
    pub target_app: Option<ForegroundApp>,
    /// When the user released the hotkey (latency is measured from here).
    pub released_at: Instant,
}

/// Stage timings for one dictation, in milliseconds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PipelineTimings {
    pub queue_ms: f64,
    pub asr_ms: f64,
    pub cleanup_ms: f64,
    pub ai_ms: f64,
    pub inject_ms: f64,
    /// Hotkey release → text handed to the target app.
    pub total_ms: f64,
}

impl PipelineTimings {
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{:.0} ms（辨識 {:.0} / 整理 {:.0}",
            self.total_ms, self.asr_ms, self.cleanup_ms
        );
        if self.ai_ms > 0.0 {
            s.push_str(&format!(" / AI {:.0}", self.ai_ms));
        }
        s.push_str(&format!(" / 送出 {:.0}）", self.inject_ms));
        s
    }
}

#[derive(Clone, Debug)]
pub struct TranscriptionResult {
    pub text: String,
    pub recording_path: PathBuf,
    pub duration_sec: f32,
    pub delivered: bool,
    pub timings: PipelineTimings,
    pub app: Option<String>,
    pub raw_text: String,
}

#[derive(Debug)]
pub enum WorkerEvent {
    Model(ModelEvent),
    Transcription(TranscriptionEvent),
}

enum WorkerCommand {
    LoadModel {
        model_path: PathBuf,
        use_cuda: bool,
        cancel: Arc<AtomicBool>,
        warm_up: bool,
    },
    Transcribe(Box<TranscriptionRequest>),
    Shutdown,
}

type WorkerWaker = Arc<std::sync::Mutex<Option<Box<dyn Fn() + Send>>>>;

pub struct ModelWorker {
    commands: Sender<WorkerCommand>,
    events: Receiver<WorkerEvent>,
    waker: WorkerWaker,
}

impl ModelWorker {
    pub fn start() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (raw_tx, event_rx) = mpsc::channel();
        let waker: WorkerWaker = Arc::new(std::sync::Mutex::new(None));
        let event_tx = WakingSender { tx: raw_tx, waker: waker.clone() };

        std::thread::spawn(move || {
            let mut transcriber: Option<Transcriber> = None;
            let mut loaded_model: Option<(PathBuf, bool)> = None;

            while let Ok(command) = command_rx.recv() {
                if matches!(command, WorkerCommand::Shutdown) {
                    break;
                }
                let is_transcribe = matches!(command, WorkerCommand::Transcribe(_));
                // A panic inside whisper/cleanup used to kill this thread
                // silently, leaving the UI on "轉寫中" forever.
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match command {
                    WorkerCommand::LoadModel {
                        model_path,
                        use_cuda,
                        cancel,
                        warm_up,
                    } => match load_worker_model(&model_path, use_cuda, cancel, &event_tx) {
                        Ok(loaded) => {
                            if warm_up {
                                loaded.warm_up();
                            }
                            transcriber = Some(loaded);
                            loaded_model = Some((model_path, use_cuda));
                            let _ = event_tx.send(WorkerEvent::Model(ModelEvent::Ready));
                        }
                        Err(err) if err == "cancelled" => {
                            let _ = event_tx.send(WorkerEvent::Model(ModelEvent::Cancelled));
                        }
                        Err(err) => {
                            log_error("model worker load", &err);
                            transcriber = None;
                            loaded_model = None;
                            let _ = event_tx.send(WorkerEvent::Model(ModelEvent::Failed(err)));
                        }
                    },
                    WorkerCommand::Transcribe(request) => {
                        let needs_model = loaded_model
                            .as_ref()
                            .map(|(path, cuda)| {
                                path != &request.model_path || *cuda != request.use_cuda
                            })
                            .unwrap_or(true);

                        if needs_model {
                            let _ = event_tx.send(WorkerEvent::Transcription(
                                TranscriptionEvent::Status("載入常駐模型...".to_string()),
                            ));
                            match Transcriber::new(&request.model_path, request.use_cuda) {
                                Ok(loaded) => {
                                    transcriber = Some(loaded);
                                    loaded_model =
                                        Some((request.model_path.clone(), request.use_cuda));
                                }
                                Err(err) => {
                                    let error = format!("模型載入失敗: {}", err);
                                    log_error("model worker reload", &error);
                                    let _ = event_tx.send(WorkerEvent::Transcription(
                                        TranscriptionEvent::Failed(error),
                                    ));
                                    return;
                                }
                            }
                        }

                        match transcriber.as_ref() {
                            Some(transcriber) => {
                                run_transcription_with_transcriber(transcriber, *request, |event| {
                                    let _ = event_tx.send(WorkerEvent::Transcription(event));
                                })
                            }
                            None => {
                                let _ = event_tx.send(WorkerEvent::Transcription(
                                    TranscriptionEvent::Failed("模型尚未載入".to_string()),
                                ));
                            }
                        }
                    }
                    WorkerCommand::Shutdown => {}
                }));
                if let Err(panic) = outcome {
                    let msg = panic
                        .downcast_ref::<&str>()
                        .map(|s| s.to_string())
                        .or_else(|| panic.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "unknown panic".to_string());
                    let error = format!("轉錄程序發生內部錯誤，已重設：{msg}");
                    log_error("worker panic", &error);
                    transcriber = None;
                    loaded_model = None;
                    let event = if is_transcribe {
                        WorkerEvent::Transcription(TranscriptionEvent::Failed(error))
                    } else {
                        WorkerEvent::Model(ModelEvent::Failed(error))
                    };
                    let _ = event_tx.send(event);
                }
            }
        });

        Self {
            commands: command_tx,
            events: event_rx,
            waker,
        }
    }

    /// Called (from the worker thread) after every event, so the UI can wake up
    /// instead of polling on a timer.
    pub fn set_waker(&self, wake: impl Fn() + Send + 'static) {
        if let Ok(mut slot) = self.waker.lock() {
            *slot = Some(Box::new(wake));
        }
    }

    pub fn load_model(
        &self,
        model_path: PathBuf,
        use_cuda: bool,
        cancel: Arc<AtomicBool>,
        warm_up: bool,
    ) -> Result<(), String> {
        self.commands
            .send(WorkerCommand::LoadModel {
                model_path,
                use_cuda,
                cancel,
                warm_up,
            })
            .map_err(|err| err.to_string())
    }

    pub fn transcribe(&self, request: TranscriptionRequest) -> Result<(), String> {
        self.commands
            .send(WorkerCommand::Transcribe(Box::new(request)))
            .map_err(|err| err.to_string())
    }

    pub fn try_recv(&self) -> Option<WorkerEvent> {
        self.events.try_recv().ok()
    }
}

/// Event sender that pokes the UI waker after each send.
#[derive(Clone)]
struct WakingSender {
    tx: Sender<WorkerEvent>,
    waker: WorkerWaker,
}

impl WakingSender {
    fn send(&self, event: WorkerEvent) -> Result<(), ()> {
        let result = self.tx.send(event).map_err(|_| ());
        if let Ok(waker) = self.waker.lock() {
            if let Some(wake) = waker.as_ref() {
                wake();
            }
        }
        result
    }
}

impl Drop for ModelWorker {
    fn drop(&mut self) {
        let _ = self.commands.send(WorkerCommand::Shutdown);
    }
}

pub struct SpeakTypeEngine {
    recorder: Recorder,
    model_path: PathBuf,
    use_cuda: bool,
    model_ready: bool,
    model_error: Option<String>,
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
            recorder: Recorder::new(None, 1.0),
            model_path,
            use_cuda,
            model_ready,
            model_error,
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

    pub fn update_audio_config(&mut self, input_device_name: Option<String>, gain: f32) {
        self.recorder.set_input_device_name(input_device_name);
        self.recorder.set_gain(gain);
    }

    /// Keep the microphone stream open between dictations.
    pub fn set_keep_mic_warm(&mut self, keep_warm: bool) {
        self.recorder.set_keep_warm(keep_warm);
    }

    pub fn mic_is_warm(&self) -> bool {
        self.recorder.is_warm()
    }

    pub fn input_devices(&self) -> Vec<String> {
        self.recorder.list_devices()
    }

    pub fn mark_model_ready(&mut self) {
        self.model_ready = true;
        self.model_error = None;
    }

    pub fn mark_model_failed(&mut self, error: String) {
        self.model_ready = false;
        self.model_error = Some(error);
    }

    pub fn toggle_recording(
        &mut self,
        output: &OutputConfig,
        scenario: Scenario,
        mode: TranscriptionMode,
    ) -> Result<Option<TranscriptionRequest>, String> {
        if self.recording {
            self.stop_recording_capture(output, scenario, mode).map(Some)
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

    pub fn stop_recording_capture(
        &mut self,
        output: &OutputConfig,
        scenario: Scenario,
        mode: TranscriptionMode,
    ) -> Result<TranscriptionRequest, String> {
        if !self.recording {
            return Err("目前沒有正在錄音".to_string());
        }

        let released_at = Instant::now();
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
            output: output.clone(),
            model_path: self.model_path.clone(),
            use_cuda: self.use_cuda,
            scenario,
            mode,
            duration_sec: duration,
            transcription: TranscriptionConfig::default(),
            injection: InjectionConfig::default(),
            ai: AiCleanupConfig::default(),
            target_app: inject::foreground_app(),
            released_at,
        })
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn model_status_text(&self) -> String {
        if self.model_ready {
            use crate::modules::transcription::transcriber as t;
            let acceleration =
                t::choose_backend(self.use_cuda, t::built_with_cuda(), t::nvidia_driver_present()).label();
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

    pub fn deliver_text_now(
        &mut self,
        text: &str,
        output: &OutputConfig,
        injection: &InjectionConfig,
    ) -> Result<(), String> {
        deliver_text(text, output, injection, None)
    }
}

fn load_worker_model(
    model_path: &Path,
    use_cuda: bool,
    cancel: Arc<AtomicBool>,
    event_tx: &WakingSender,
) -> Result<Transcriber, String> {
    ensure_model_file(model_path, cancel, |progress| {
        let _ = event_tx.send(WorkerEvent::Model(ModelEvent::Progress(progress)));
    }, |warning| {
        let _ = event_tx.send(WorkerEvent::Model(ModelEvent::Warning(warning)));
    })?;

    Transcriber::new(model_path, use_cuda).map_err(|err| format!("模型載入失敗: {}", err))
}

pub fn load_recording_wav(path: &Path) -> Result<RecordedAudio, String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|err| format!("無法開啟錄音檔 {}: {}", path.display(), err))?;
    let spec = reader.spec();
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("讀取錄音檔失敗: {}", err))?,
        hound::SampleFormat::Int => {
            if spec.bits_per_sample <= 16 {
                reader
                    .samples::<i16>()
                    .map(|sample| sample.map(|sample| sample as f32 / i16::MAX as f32))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|err| format!("讀取錄音檔失敗: {}", err))?
            } else {
                reader
                    .samples::<i32>()
                    .map(|sample| sample.map(|sample| sample as f32 / i32::MAX as f32))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|err| format!("讀取錄音檔失敗: {}", err))?
            }
        }
    };

    Ok(RecordedAudio {
        samples,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
    })
}

/// Prompt that nudges whisper towards Traditional Chinese and the user's terms.
pub fn build_initial_prompt(transcription: &TranscriptionConfig, output: &OutputConfig) -> String {
    let lang = transcription.language.trim().to_ascii_lowercase();
    let chinese = lang.is_empty() || lang == "auto" || lang.starts_with("zh");
    let mut prompt = String::new();
    if transcription.prefer_traditional_prompt
        && chinese
        && output.chinese_conversion != ChineseConversionMode::SimplifiedChina
    {
        prompt.push_str("以下是繁體中文的語音輸入，可能夾雜 English。");
    }
    let terms: Vec<&str> = output
        .vocabulary
        .entries
        .iter()
        .filter(|e| !e.is_blank())
        .map(|e| {
            let r = e.replacement.trim();
            if r.is_empty() { e.source.trim() } else { r }
        })
        .take(40)
        .collect();
    if !terms.is_empty() {
        prompt.push_str("常用詞：");
        prompt.push_str(&terms.join("、"));
        prompt.push('。');
    }
    prompt
}

fn run_transcription_with_transcriber(
    transcriber: &Transcriber,
    request: TranscriptionRequest,
    mut on_event: impl FnMut(TranscriptionEvent),
) {
    let queue_ms = request.released_at.elapsed().as_secs_f64() * 1000.0;
    let recording_path = paths::recordings_dir().join(format!(
        "recording_{}.wav",
        Local::now().format("%Y%m%d_%H%M%S_%3f")
    ));

    if request.mode == TranscriptionMode::Fast {
        on_event(TranscriptionEvent::Draft("轉錄草稿產生中...".to_string()));
    }

    on_event(TranscriptionEvent::Status("轉錄中...".to_string()));
    let asr_start = Instant::now();
    let whisper_audio = if request.audio.sample_rate == 16_000 && request.audio.channels == 1 {
        std::borrow::Cow::Borrowed(&request.audio.samples)
    } else {
        std::borrow::Cow::Owned(request.audio.to_whisper_mono_16khz())
    };
    let options = TranscribeOptions {
        language: request.transcription.language.clone(),
        initial_prompt: build_initial_prompt(&request.transcription, &request.output),
        fast_encoder: request.transcription.fast_encoder,
        threads: request.transcription.threads,
    };
    let text = match transcriber.transcribe_with(&whisper_audio, &options) {
        Ok(text) => text,
        Err(err) => {
            let error = format!("轉錄失敗: {}", err);
            log_error("transcription", &error);
            on_event(TranscriptionEvent::Failed(error));
            return;
        }
    };
    let asr_ms = asr_start.elapsed().as_secs_f64() * 1000.0;

    // The WAV is written after the text is delivered, off the critical path.
    let audio_for_save = request.audio.clone();
    let save_path = recording_path.clone();
    let timings = PipelineTimings { queue_ms, asr_ms, ..Default::default() };
    finish_transcription_text(request, recording_path, text, timings, on_event);
    std::thread::spawn(move || {
        if let Err(err) = save_recording_wav_to(&audio_for_save, &save_path) {
            log_error("recording save", err);
        }
    });
}

/// Pick the scenario from the per-app rule, if any.
pub fn scenario_for_app(injection: &InjectionConfig, app: Option<&ForegroundApp>, fallback: Scenario) -> Scenario {
    injection
        .rule_for(app.and_then(|a| a.process.as_deref()))
        .and_then(|rule| rule.scenario.as_deref())
        .and_then(Scenario::from_name)
        .unwrap_or(fallback)
}

fn finish_transcription_text(
    request: TranscriptionRequest,
    recording_path: PathBuf,
    text: String,
    mut timings: PipelineTimings,
    mut on_event: impl FnMut(TranscriptionEvent),
) {
    let raw_text = normalize_transcription_text(&text);
    let text = raw_text.clone();
    let app_name = request.target_app.as_ref().and_then(|a| a.process.clone());

    if request.output.voice_commands_enabled {
        if let Some(command) = parse_voice_command(&text) {
            on_event(TranscriptionEvent::Status("執行語音指令...".to_string()));
            let mut input = InputController::new();
            execute_voice_command(&command, input.enigo());
            timings.total_ms = request.released_at.elapsed().as_secs_f64() * 1000.0;
            on_event(TranscriptionEvent::Completed(TranscriptionResult {
                text: format!("[指令] {}", voice_command_label(&command)),
                recording_path,
                duration_sec: request.duration_sec,
                delivered: true,
                timings,
                app: app_name,
                raw_text,
            }));
            return;
        }
    }

    let scenario = scenario_for_app(&request.injection, request.target_app.as_ref(), request.scenario);
    let cleanup_start = Instant::now();
    let mut text = match postprocess_transcription(&text, scenario, &request.output) {
        Ok(text) => text,
        Err(err) => {
            let error = format!("文字後處理失敗: {}", err);
            log_error("text postprocess", &error);
            on_event(TranscriptionEvent::Failed(error));
            return;
        }
    };
    timings.cleanup_ms = cleanup_start.elapsed().as_secs_f64() * 1000.0;

    if request.ai.enabled && !text.is_empty() {
        on_event(TranscriptionEvent::Status("AI 潤稿中...".to_string()));
        let ai_start = Instant::now();
        let rule = request
            .injection
            .rule_for(request.target_app.as_ref().and_then(|a| a.process.as_deref()));
        let dictionary: Vec<String> = request
            .output
            .vocabulary
            .entries
            .iter()
            .filter(|e| !e.is_blank())
            .map(|e| if e.replacement.trim().is_empty() { e.source.trim().to_string() } else { e.replacement.trim().to_string() })
            .collect();
        let ctx = AiContext {
            scenario: scenario.name(),
            app: app_name.as_deref(),
            tone: rule.map(|r| r.tone.as_str()).unwrap_or(""),
            dictionary: &dictionary,
            traditional_chinese: request.output.chinese_conversion != ChineseConversionMode::SimplifiedChina,
        };
        match ai_cleanup::rewrite(&request.ai, &ctx, &text) {
            Ok(rewritten) => {
                // keep the script consistent (LLMs sometimes drift to Simplified)
                text = postprocess_transcription(&rewritten, scenario, &request.output).unwrap_or(rewritten);
            }
            Err(err) => log_error("ai cleanup", err),
        }
        timings.ai_ms = ai_start.elapsed().as_secs_f64() * 1000.0;
    }

    if text.is_empty() {
        on_event(TranscriptionEvent::Failed("沒有辨識到語音".to_string()));
        return;
    }

    let delivered = if request.output.manual_review_before_send {
        false
    } else {
        on_event(TranscriptionEvent::Status("輸出文字...".to_string()));
        let inject_start = Instant::now();
        match deliver_text(&text, &request.output, &request.injection, request.target_app.as_ref()) {
            Ok(()) => {
                timings.inject_ms = inject_start.elapsed().as_secs_f64() * 1000.0;
                true
            }
            Err(err) => {
                log_error("text delivery", &err);
                on_event(TranscriptionEvent::Failed(err));
                return;
            }
        }
    };
    timings.total_ms = request.released_at.elapsed().as_secs_f64() * 1000.0;

    on_event(TranscriptionEvent::Completed(TranscriptionResult {
        text,
        recording_path,
        duration_sec: request.duration_sec,
        delivered,
        timings,
        app: app_name,
        raw_text,
    }));
}

/// Deliver `text` according to the output mode and per-app injection rules.
pub fn deliver_text(
    text: &str,
    output: &OutputConfig,
    injection: &InjectionConfig,
    target: Option<&ForegroundApp>,
) -> Result<(), String> {
    let restore = output.buffer_mode == OutputBufferMode::Temporary && output.restore_clipboard_after_inject;
    let mut plan = InjectPlan::resolve(injection, target, restore);
    if !output.auto_inject_focused_window {
        if output.buffer_mode == OutputBufferMode::Clipboard {
            plan.method = InjectionMethod::ClipboardOnly;
        } else {
            return Ok(());
        }
    }
    inject::inject(text, &plan, target).map_err(|e| format!("文字輸出失敗: {}", e))
}

/// Save as 16-bit PCM (half the size of the old 32-bit float files).
pub fn save_recording_wav_to(audio: &RecordedAudio, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    let spec = hound::WavSpec {
        channels: audio.channels.max(1),
        sample_rate: audio.sample_rate.max(1),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|err| format!("無法建立錄音檔 {}: {}", path.display(), err))?;
    for sample in &audio.samples {
        writer
            .write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .map_err(|err| format!("寫入錄音檔失敗: {}", err))?;
    }
    writer
        .finalize()
        .map_err(|err| format!("完成錄音檔失敗: {}", err))?;

    Ok(())
}

fn normalize_transcription_text(text: &str) -> String {
    text.chars()
        .filter(|ch| {
            !ch.is_control()
                && *ch != '\0'
                && *ch != '\u{feff}'
                && *ch != '\u{fffd}'
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn describe_io_error(what: &str, path: &Path, err: &std::io::Error) -> String {
    let hint = if err.kind() == std::io::ErrorKind::PermissionDenied {
        "（沒有寫入權限：請勿把模型資料夾設在 C:\\Program Files，或檔案正被防毒/其他程式使用）"
    } else {
        ""
    };
    format!("{what} {}: {err}{hint}", path.display())
}

fn ensure_model_file(
    model_path: &Path,
    cancel: Arc<AtomicBool>,
    mut on_progress: impl FnMut(ModelDownloadProgress),
    mut on_warning: impl FnMut(String),
) -> Result<(), String> {
    if model_path.exists() {
        return Ok(());
    }

    let parent = model_path
        .parent()
        .ok_or_else(|| format!("模型路徑無效: {}", model_path.display()))?;
    fs::create_dir_all(parent).map_err(|err| describe_io_error("無法建立模型資料夾", parent, &err))?;

    let file_name = model_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("模型檔名無效: {}", model_path.display()))?;
    let url = models::model_url(file_name);
    let temp_path = model_path.with_extension("bin.part");

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| format!("建立下載器失敗: {}", err))?;
    let resume_bytes = temp_path
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let mut request = client.get(&url);
    if resume_bytes > 0 {
        request = request.header(RANGE, format!("bytes={resume_bytes}-"));
    }

    let mut response = request
        .send()
        .map_err(|err| format!("無法連線到模型來源: {}", err))?
        .error_for_status()
        .map_err(|err| match err.status().map(|s| s.as_u16()) {
            Some(401) | Some(403) => format!("模型來源拒絕存取 (HTTP {})：{}，請檢查網路/代理或稍後再試", err.status().unwrap(), url),
            _ => format!("模型來源回應失敗: {}", err),
        })?;

    let resumed = response.status().as_u16() == 206 && resume_bytes > 0;
    if resume_bytes > 0 && !resumed {
        let _ = fs::remove_file(&temp_path);
    }

    let starting_bytes = if resumed { resume_bytes } else { 0 };
    let total_bytes = response
        .content_length()
        .map(|length| length.saturating_add(starting_bytes));
    let remote_sha256 = response
        .headers()
        .get("x-linked-etag")
        .or_else(|| response.headers().get("etag"))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"').to_string())
        .filter(|value| is_sha256_hex(value));
    let mut output = OpenOptions::new()
        .create(true)
        .append(resumed)
        .write(true)
        .truncate(!resumed)
        .open(&temp_path)
        .map_err(|err| describe_io_error("無法建立暫存模型檔", &temp_path, &err))?;
    let mut buffer = vec![0_u8; DOWNLOAD_CHUNK_SIZE];
    let started = Instant::now();
    let mut downloaded_bytes = starting_bytes;

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
        describe_io_error("無法寫入模型檔", model_path, &err)
    })?;

    if let Some(expected) = remote_sha256 {
        let actual = models::sha256_file(&model_path.to_path_buf())
            .map_err(|err| format!("模型 SHA256 計算失敗: {}", err))?;
        if !actual.eq_ignore_ascii_case(&expected) {
            on_warning(format!(
                "模型 SHA256 校驗不一致: expected={expected}, actual={actual}"
            ));
        }
    }

    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn transcription_result_creation() {
        let result = TranscriptionResult {
            text: "hello world".into(),
            recording_path: PathBuf::from("/tmp/test.wav"),
            duration_sec: 2.5,
            delivered: true,
            timings: PipelineTimings::default(),
            app: None,
            raw_text: "hello world".into(),
        };
        assert_eq!(result.text, "hello world");
        assert!(result.delivered);
    }

    #[test]
    fn transcription_request_creation() {
        use crate::modules::config::{OutputConfig, TranscriptionMode};
        let request = TranscriptionRequest {
            audio: RecordedAudio {
                samples: vec![0.0; 16000],
                sample_rate: 16000,
                channels: 1,
            },
            output: OutputConfig::default(),
            model_path: PathBuf::from("model.bin"),
            use_cuda: false,
            scenario: Scenario::Chat,
            mode: TranscriptionMode::Stable,
            duration_sec: 1.0,
            transcription: TranscriptionConfig::default(),
            injection: InjectionConfig::default(),
            ai: AiCleanupConfig::default(),
            target_app: None,
            released_at: Instant::now(),
        };
        assert_eq!(request.audio.sample_rate, 16000);
        assert_eq!(request.audio.samples.len(), 16000);
    }

    #[test]
    fn is_sha256_hex_accepts_correct_hash() {
        let valid = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        assert!(is_sha256_hex(valid));
    }

    #[test]
    fn is_sha256_hex_rejects_short_hash() {
        assert!(!is_sha256_hex("abc123"));
    }

    #[test]
    fn is_sha256_hex_rejects_invalid_chars() {
        let invalid = "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg";
        assert!(!is_sha256_hex(invalid));
    }

    #[test]
    fn is_sha256_hex_rejects_empty() {
        assert!(!is_sha256_hex(""));
    }

    #[test]
    fn classify_recording_error_access() {
        let msg = classify_recording_error("access denied: microphone");
        assert!(msg.contains("權限"));
    }

    #[test]
    fn classify_recording_error_device() {
        let msg = classify_recording_error("no input device found");
        assert!(msg.contains("找不到可用麥克風"));
    }

    #[test]
    fn classify_recording_error_unknown() {
        let msg = classify_recording_error("something else");
        assert!(msg.contains("something else"));
    }

    #[test]
    fn speaktype_engine_initial_state() {
        let engine = SpeakTypeEngine::new(PathBuf::from("nonexistent.bin"), false);
        assert!(!engine.is_recording());
        assert!(engine.model_error().is_some());
        assert!(!engine.model_ready);
    }

    #[test]
    fn speaktype_engine_set_model_path() {
        let mut engine = SpeakTypeEngine::new(PathBuf::from("nonexistent.bin"), false);
        engine.set_model_path(PathBuf::from("/tmp/other.bin"), true);
        assert!(!engine.model_ready);
        assert_eq!(engine.model_status_text(), "模型尚未下載或尚未準備完成");
    }

    #[test]
    fn model_download_progress_display() {
        let progress = ModelDownloadProgress {
            downloaded_bytes: 500,
            total_bytes: Some(1000),
            speed_bytes_per_sec: 100.0,
            url: "https://example.com/model.bin".into(),
        };
        assert_eq!(progress.downloaded_bytes, 500);
        assert_eq!(progress.total_bytes, Some(1000));
    }
}
