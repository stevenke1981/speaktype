use eframe::egui;
use speaktype::modules::audio::LevelMonitor;
use speaktype::modules::config::{
    AppConfig, ChineseConversionMode, OutputBufferMode, OutputRulesConfig, ScenarioOutputRules,
    TranscriptionMode, VocabularyEntry,
};
use speaktype::modules::diagnostics;
use speaktype::modules::engine::{
    load_recording_wav, ModelDownloadProgress, ModelEvent, ModelWorker, SpeakTypeEngine,
    TranscriptionEvent, TranscriptionRequest, TranscriptionResult, WorkerEvent,
};
use speaktype::modules::error::{log_error, log_file_path};
use speaktype::modules::gui::GuiManager;
use speaktype::modules::history::HistoryManager;
use speaktype::modules::input::{GlobalHotkey, HotkeyCombo, HotkeyEvent};
use speaktype::modules::models::{self, MODEL_CATALOG};
use speaktype::modules::paths;
use speaktype::modules::recordings;
use speaktype::modules::scenario::{Scenario, ScenarioManager};
use speaktype::modules::startup;
use speaktype::modules::tray::{create_tray, minimize_main_window, TrayAction, TrayManager};
use speaktype::modules::utils::device::DeviceStatus;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct SpeakTypeApp {
    recording: bool,
    last_result: String,
    last_error: Option<String>,
    recording_start: Option<Instant>,
    scratch_text: String,
    show_history_window: bool,
    show_settings_window: bool,
    show_error_window: bool,
    show_recordings_window: bool,
    show_model_manager_window: bool,
    error_log: Vec<String>,
    transcription_status: String,
    transcription_busy: bool,
    transcription_draft: String,
    pending_retry: Option<TranscriptionRequest>,
    pending_preview: Option<String>,
    model_status_message: String,
    model_download: Option<ModelDownloadProgress>,
    model_cancel: Option<Arc<AtomicBool>>,
    model_worker: ModelWorker,
    hotkey_capture: bool,
    recording_filter: String,
    audio_devices: Vec<String>,
    level_monitor: Option<LevelMonitor>,
    input_level: f32,
    selected_recording_for_retry: Option<std::path::PathBuf>,
    tray: Option<TrayManager>,
    hidden_to_tray: bool,
    start_minimized_pending: bool,
    restore_guard_until: Option<Instant>,
    exit_requested: bool,
    scenario_manager: ScenarioManager,
    history: HistoryManager,
    gui: GuiManager,
    config: AppConfig,
    engine: SpeakTypeEngine,
    hotkey: GlobalHotkey,
    device_status: DeviceStatus,
}

impl SpeakTypeApp {
    pub fn new(ctx: &egui::Context, start_hidden_to_tray: bool) -> Self {
        configure_cjk_fonts(ctx);

        let config = AppConfig::load();
        let current_scenario = config
            .last_scenario
            .as_deref()
            .and_then(Scenario::from_name)
            .unwrap_or(Scenario::Chat);
        recordings::cleanup_recordings(
            config.recording.retention_days,
            config.recording.max_total_mb,
        );

        let model_path = config.get_model_path();
        let engine = SpeakTypeEngine::new(model_path, config.use_cuda);
        let device_status = DeviceStatus::detect(engine.model_status_text(), config.use_cuda);
        let last_error = engine.model_error().map(str::to_string);
        let error_log = last_error.iter().cloned().collect();
        if let Some(error) = &last_error {
            log_error("app init", error);
        }

        let mut app = Self {
            recording: false,
            last_result: String::new(),
            last_error,
            recording_start: None,
            scratch_text: String::new(),
            show_history_window: false,
            show_settings_window: false,
            show_error_window: false,
            show_recordings_window: false,
            show_model_manager_window: false,
            error_log,
            transcription_status: "就緒".to_string(),
            transcription_busy: false,
            transcription_draft: String::new(),
            pending_retry: None,
            pending_preview: None,
            model_status_message: String::new(),
            model_download: None,
            model_cancel: None,
            model_worker: ModelWorker::start(),
            hotkey_capture: false,
            recording_filter: String::new(),
            audio_devices: Vec::new(),
            level_monitor: None,
            input_level: 0.0,
            selected_recording_for_retry: None,
            tray: create_tray(ctx),
            hidden_to_tray: start_hidden_to_tray,
            start_minimized_pending: start_hidden_to_tray,
            restore_guard_until: None,
            exit_requested: false,
            scenario_manager: ScenarioManager::with_current(current_scenario),
            history: HistoryManager::load(),
            gui: GuiManager::new(),
            config,
            engine,
            hotkey: GlobalHotkey::new("Ctrl+Shift+L"),
            device_status,
        };
        let record_toggle = app.config.hotkeys.record_toggle.clone();
        if let Err(err) = app.hotkey.update_hotkey(&record_toggle) {
            app.record_error(format!("快捷鍵設定無效，已使用預設值: {}", err));
        }
        app.engine.update_audio_config(
            app.config.recording.input_device_name.clone(),
            app.config.recording.gain,
        );
        app.audio_devices = app.engine.input_devices();
        app.start_model_job();
        if start_hidden_to_tray && app.tray.is_none() {
            app.hidden_to_tray = false;
            app.start_minimized_pending = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        app
    }

    fn toggle_recording_action(&mut self) {
        if self.transcription_busy {
            return;
        }

        match self.engine.toggle_recording() {
            Ok(Some(request)) => self.start_transcription_job(request),
            Ok(None) => {
                self.last_error = None;
                self.recording = true;
                self.recording_start = Some(Instant::now());
                self.transcription_status = "錄音中...".to_string();
            }
            Err(e) => {
                self.record_error(e);
                self.recording = self.engine.is_recording();
            }
        }
    }

    fn start_ptt_recording(&mut self) {
        if self.transcription_busy {
            return;
        }

        match self.engine.start_recording() {
            Ok(()) => {
                self.last_error = None;
                self.recording = true;
                self.recording_start = Some(Instant::now());
                self.transcription_status = "錄音中...".to_string();
            }
            Err(e) => {
                self.record_error(e);
                self.recording = self.engine.is_recording();
            }
        }
    }

    fn finish_ptt_recording(&mut self) {
        match self.engine.stop_recording_capture() {
            Ok(request) => self.start_transcription_job(request),
            Err(e) => {
                self.record_error(e);
                self.recording = self.engine.is_recording();
            }
        }
    }

    fn accept_transcription(&mut self, result: TranscriptionResult) {
        self.last_result = result.text.clone();
        self.scratch_text = result.text.clone();
        self.last_error = None;
        self.recording = self.engine.is_recording();
        self.recording_start = None;
        self.engine
            .set_last_recording_path(result.recording_path.clone());
        self.history.add_record(
            result.text.clone(),
            self.scenario_manager.current().name().to_string(),
            result.duration_sec,
        );
        if result.delivered {
            self.transcription_status = "已完成".to_string();
            self.pending_preview = None;
        } else {
            self.transcription_status = "等待手動送出".to_string();
            self.pending_preview = Some(result.text);
        }
    }

    fn select_scenario(&mut self, scenario: Scenario) {
        self.scenario_manager.select(scenario);
        self.config.last_scenario = Some(scenario.name().to_string());
        if let Err(err) = self.config.save() {
            self.record_error(format!("儲存設定失敗: {}", err));
        }
    }

    fn save_config(&mut self) {
        if let Err(err) = self.config.save() {
            self.record_error(format!("儲存設定失敗: {}", err));
        } else {
            self.last_error = None;
        }
    }

    fn record_error(&mut self, error: String) {
        log_error("gui", &error);
        self.last_error = Some(error.clone());
        self.error_log.insert(0, error);
        self.error_log.truncate(50);
    }

    fn refresh_device_status(&mut self) {
        self.device_status =
            DeviceStatus::detect(self.engine.model_status_text(), self.config.use_cuda);
    }

    fn reload_model_from_config(&mut self) {
        let model_path = self.config.get_model_path();
        self.engine.set_model_path(model_path, self.config.use_cuda);
        self.start_model_job();
        self.refresh_device_status();
    }

    fn select_model(&mut self, model_name: &str, force_download: bool) {
        self.config.model_name = Some(model_name.to_string());
        self.config.models_dir = Some(paths::models_dir().display().to_string());
        if let Err(err) = self.config.save() {
            self.record_error(format!("儲存模型設定失敗: {}", err));
        }

        let model_path = self.config.get_model_path();
        if force_download && model_path.exists() {
            if let Err(err) = std::fs::remove_file(&model_path) {
                self.record_error(format!("刪除舊模型失敗: {}", err));
            }
        }

        self.engine.set_model_path(model_path, self.config.use_cuda);
        self.start_model_job();
        self.refresh_device_status();
    }

    fn start_model_job(&mut self) {
        if let Some(cancel) = &self.model_cancel {
            cancel.store(true, Ordering::Relaxed);
        }

        let model_path = self.config.get_model_path();
        let use_cuda = self.config.use_cuda;
        let cancel = Arc::new(AtomicBool::new(false));
        self.model_status_message = if model_path.exists() {
            "模型預載中...".to_string()
        } else {
            "模型準備中...".to_string()
        };
        self.model_download = None;
        self.model_cancel = Some(cancel.clone());
        if let Err(err) = self.model_worker.load_model(model_path, use_cuda, cancel) {
            self.record_error(format!("模型 worker 啟動失敗: {err}"));
        }
    }

    fn start_transcription_job(&mut self, mut request: TranscriptionRequest) {
        request.output = self.config.output.clone();
        request.scenario = self.scenario_manager.current();
        request.model_path = self.config.get_model_path();
        request.use_cuda = self.config.use_cuda;
        request.mode = self.config.recording.transcription_mode;

        let retry_request = request.clone();
        self.transcription_busy = true;
        self.transcription_draft.clear();
        self.transcription_status = "排入背景轉錄...".to_string();
        self.recording = false;
        self.recording_start = None;
        self.pending_retry = Some(retry_request);

        if let Err(err) = self.model_worker.transcribe(request) {
            self.transcription_busy = false;
            self.record_error(format!("轉錄 worker 啟動失敗: {err}"));
        }
    }

    fn retry_last_transcription(&mut self) {
        if self.transcription_busy {
            return;
        }
        if let Some(request) = self.pending_retry.clone() {
            self.start_transcription_job(request);
        }
    }

    fn retranscribe_recording(&mut self, path: std::path::PathBuf) {
        if self.transcription_busy {
            return;
        }

        match load_recording_wav(&path) {
            Ok(audio) => {
                let duration_sec = if audio.sample_rate > 0 && audio.channels > 0 {
                    audio.samples.len() as f32 / audio.sample_rate as f32 / audio.channels as f32
                } else {
                    0.0
                };
                let request = TranscriptionRequest {
                    audio,
                    output: self.config.output.clone(),
                    model_path: self.config.get_model_path(),
                    use_cuda: self.config.use_cuda,
                    scenario: self.scenario_manager.current(),
                    mode: self.config.recording.transcription_mode,
                    duration_sec,
                };
                self.selected_recording_for_retry = Some(path);
                self.start_transcription_job(request);
            }
            Err(err) => self.record_error(err),
        }
    }

    fn send_pending_preview(&mut self) {
        let Some(text) = self.pending_preview.clone() else {
            return;
        };

        let mut output = self.config.output.clone();
        output.auto_inject_focused_window = true;
        match self.engine.deliver_text_now(&text, &output) {
            Ok(()) => {
                self.pending_preview = None;
                self.transcription_status = "已送出".to_string();
                self.last_error = None;
            }
            Err(err) => self.record_error(err),
        }
    }

    fn toggle_level_monitor(&mut self) {
        if let Some(mut monitor) = self.level_monitor.take() {
            monitor.stop();
            self.input_level = 0.0;
            return;
        }

        match LevelMonitor::start(
            self.config.recording.input_device_name.clone(),
            self.config.recording.gain,
        ) {
            Ok(monitor) => {
                self.level_monitor = Some(monitor);
            }
            Err(err) => self.record_error(format!("音量測試失敗: {}", err)),
        }
    }

    fn poll_input_level(&mut self) {
        if let Some(monitor) = &self.level_monitor {
            self.input_level = monitor.level();
        }
    }

    fn minimize_to_tray(&mut self, ctx: &egui::Context) {
        self.hidden_to_tray = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        minimize_main_window();
    }

    fn show_from_tray(&mut self, ctx: &egui::Context) {
        self.hidden_to_tray = false;
        self.restore_guard_until = Some(Instant::now() + Duration::from_millis(900));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn handle_tray_actions(&mut self, ctx: &egui::Context) {
        while let Some(action) = self.tray.as_ref().and_then(TrayManager::poll_action) {
            match action {
                TrayAction::ShowMain => {
                    self.show_from_tray(ctx);
                }
                TrayAction::ToggleRecording => {
                    self.show_from_tray(ctx);
                    self.toggle_recording_action();
                }
                TrayAction::OpenSettings => {
                    self.show_from_tray(ctx);
                    self.show_settings_window = true;
                }
                TrayAction::OpenHistory => {
                    self.show_from_tray(ctx);
                    self.show_history_window = true;
                }
                TrayAction::Exit => {
                    self.exit_requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn poll_worker_events(&mut self) {
        while let Some(event) = self.model_worker.try_recv() {
            match event {
                WorkerEvent::Model(event) => self.handle_model_event(event),
                WorkerEvent::Transcription(event) => self.handle_transcription_event(event),
            }
        }
    }

    fn handle_model_event(&mut self, event: ModelEvent) {
        let mut should_clear = false;
        let mut pending_error = None;
        match event {
            ModelEvent::Progress(progress) => {
                self.model_status_message = "模型下載中...".to_string();
                self.model_download = Some(progress);
            }
            ModelEvent::Ready => {
                self.engine.mark_model_ready();
                self.model_status_message = "模型已常駐準備完成".to_string();
                self.model_download = None;
                should_clear = true;
            }
            ModelEvent::Cancelled => {
                self.engine.mark_model_failed("模型下載已取消".to_string());
                self.model_status_message = "模型下載已取消".to_string();
                self.model_download = None;
                should_clear = true;
            }
            ModelEvent::Failed(err) => {
                self.engine.mark_model_failed(err.clone());
                pending_error = Some(err);
                self.model_status_message = "模型準備失敗".to_string();
                self.model_download = None;
                should_clear = true;
            }
        }

        if should_clear {
            self.model_cancel = None;
            self.refresh_device_status();
        }
        if let Some(err) = pending_error {
            self.record_error(err);
        }
    }

    fn handle_transcription_event(&mut self, event: TranscriptionEvent) {
        match event {
            TranscriptionEvent::Status(status) => self.transcription_status = status,
            TranscriptionEvent::Draft(draft) => {
                self.transcription_draft = draft;
            }
            TranscriptionEvent::Completed(result) => {
                self.transcription_busy = false;
                self.accept_transcription(result);
            }
            TranscriptionEvent::Failed(err) => {
                self.transcription_busy = false;
                self.transcription_status = "失敗".to_string();
                self.record_error(err);
            }
        }
    }

    fn capture_hotkey_from_input(&mut self, ctx: &egui::Context) {
        if !self.hotkey_capture {
            return;
        }

        let captured = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    let mut parts = Vec::new();
                    if modifiers.ctrl {
                        parts.push("Ctrl".to_string());
                    }
                    if modifiers.alt {
                        parts.push("Alt".to_string());
                    }
                    if modifiers.shift {
                        parts.push("Shift".to_string());
                    }
                    if modifiers.mac_cmd || modifiers.command {
                        parts.push("Win".to_string());
                    }
                    parts.push(format!("{key:?}").to_ascii_uppercase());
                    Some(parts.join("+"))
                } else {
                    None
                }
            })
        });

        if let Some(hotkey) = captured {
            match HotkeyCombo::parse(&hotkey) {
                Ok(combo) => {
                    let display = combo.display();
                    self.config.hotkeys.record_toggle = display.clone();
                    if let Err(err) = self.hotkey.update_hotkey(&display) {
                        self.record_error(err);
                    }
                    self.save_config();
                    self.hotkey_capture = false;
                }
                Err(err) => self.record_error(err),
            }
        }
    }
}

fn configure_cjk_fonts(ctx: &egui::Context) {
    let font_candidates = [
        r"C:\Windows\Fonts\NotoSansTC-VF.ttf",
        r"C:\Windows\Fonts\msjh.ttc",
        r"C:\Windows\Fonts\mingliu.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
    ];

    let Some((font_name, font_bytes)) = font_candidates
        .iter()
        .find_map(|path| load_font_bytes(path).map(|bytes| (path.to_string(), bytes)))
    else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert(font_name.clone(), egui::FontData::from_owned(font_bytes));

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, font_name.clone());
    }

    ctx.set_fonts(fonts);
}

fn load_font_bytes(path: &str) -> Option<Vec<u8>> {
    fs::read(Path::new(path)).ok()
}

impl eframe::App for SpeakTypeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray_actions(ctx);
        self.poll_worker_events();
        self.poll_input_level();
        self.capture_hotkey_from_input(ctx);
        if self.start_minimized_pending {
            self.start_minimized_pending = false;
            self.minimize_to_tray(ctx);
        }
        let suppress_minimize_to_tray = self
            .restore_guard_until
            .map(|until| Instant::now() < until)
            .unwrap_or(false);
        if !suppress_minimize_to_tray {
            self.restore_guard_until = None;
        }

        if ctx.input(|input| input.viewport().close_requested()) && !self.exit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.minimize_to_tray(ctx);
        }
        if self.tray.is_some()
            && !self.hidden_to_tray
            && !suppress_minimize_to_tray
            && ctx.input(|input| input.viewport().minimized.unwrap_or(false))
        {
            self.minimize_to_tray(ctx);
        }

        if self.tray.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if self.config.hotkeys.global_hotkey_enabled {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.transcription_busy || self.model_cancel.is_some() || self.level_monitor.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        if self.config.hotkeys.global_hotkey_enabled
            && self.config.hotkeys.hold_to_record
            && !self.hotkey_capture
        {
            match self.hotkey.poll_record_hotkey_event() {
                Some(HotkeyEvent::Pressed) => self.start_ptt_recording(),
                Some(HotkeyEvent::Released) => self.finish_ptt_recording(),
                None => {}
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("SpeakType");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.button("紀錄視窗").clicked() {
                    self.show_history_window = true;
                }
                if ui.button("錄音檔").clicked() {
                    self.show_recordings_window = true;
                }
                if ui.button("模型中心").clicked() {
                    self.show_model_manager_window = true;
                }
                if ui.button("設定").clicked() {
                    self.show_settings_window = true;
                }
                if ui.button("錯誤紀錄").clicked() {
                    self.show_error_window = true;
                }
                if ui.button("刷新狀態").clicked() {
                    self.refresh_device_status();
                }
                if ui
                    .add_enabled(self.tray.is_some(), egui::Button::new("最小化到 Tray"))
                    .clicked()
                {
                    self.minimize_to_tray(ctx);
                }
            });

            if self.hidden_to_tray {
                ui.colored_label(
                    egui::Color32::from_rgb(80, 180, 120),
                    "主視窗已藏到系統匣，可從 tray 右鍵選單叫回。",
                );
            }

            ui.add_space(8.0);

            self.gui.draw_device_status(
                ui,
                &self.device_status.microphone,
                &self.device_status.gpu,
                &self.device_status.model,
            );

            ui.separator();

            let mut selected_scenario = self.scenario_manager.current();
            self.gui
                .draw_scenario_selector(ui, self.scenario_manager.current(), &mut |scenario| {
                    selected_scenario = scenario
                });
            if selected_scenario != self.scenario_manager.current() {
                self.select_scenario(selected_scenario);
            }

            ui.add_space(8.0);
            ui.label(format!("狀態：{}", self.transcription_status));
            if !self.model_status_message.is_empty() {
                ui.label(format!("模型：{}", self.model_status_message));
            }
            self.draw_model_download_status(ui);

            ui.add_space(8.0);
            if ui
                .add_enabled(
                    !self.transcription_busy && self.model_cancel.is_none(),
                    egui::Button::new(if self.recording {
                        format!("停止錄音 ({})", self.config.hotkeys.record_toggle)
                    } else {
                        format!("開始錄音 ({})", self.config.hotkeys.record_toggle)
                    }),
                )
                .clicked()
            {
                self.toggle_recording_action();
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.transcription_busy && self.pending_retry.is_some(),
                        egui::Button::new("重試上一段"),
                    )
                    .clicked()
                {
                    self.retry_last_transcription();
                }
                if ui
                    .add_enabled(
                        self.pending_preview.is_some(),
                        egui::Button::new("送出到焦點視窗"),
                    )
                    .clicked()
                {
                    self.send_pending_preview();
                }
            });

            if self.recording {
                let elapsed = self
                    .recording_start
                    .map(|start| start.elapsed().as_secs_f32())
                    .unwrap_or(0.0);
                ui.label(format!("錄音中：{:.1} 秒", elapsed));
                ctx.request_repaint_after(Duration::from_millis(100));
            }

            if let Some(path) = self.engine.last_recording_path() {
                ui.label(format!("最近錄音檔：{}", path.display()));
            }

            if let Some(error) = &self.last_error {
                ui.colored_label(egui::Color32::RED, error);
            }

            if !self.transcription_draft.is_empty() {
                ui.separator();
                ui.label("快速模式草稿");
                let mut draft = self.transcription_draft.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut draft)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            }

            if !self.last_result.is_empty() {
                ui.separator();
                ui.label("最近辨識結果");
                let mut display_text = self.last_result.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut display_text)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            }

            if !self.scratch_text.is_empty()
                && self.config.output.buffer_mode == OutputBufferMode::Temporary
            {
                ui.separator();
                ui.label("暫存區");
                let mut scratch_text = self.scratch_text.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut scratch_text)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            }
        });

        self.draw_history_window(ctx);
        self.draw_settings_window(ctx);
        self.draw_model_manager_window(ctx);
        self.draw_error_window(ctx);
        self.draw_recordings_window(ctx);
        self.draw_model_download_window(ctx);
    }
}

impl SpeakTypeApp {
    fn draw_model_download_status(&mut self, ui: &mut egui::Ui) {
        let Some(progress) = &self.model_download else {
            return;
        };

        let fraction = progress
            .total_bytes
            .map(|total| progress.downloaded_bytes as f32 / total.max(1) as f32)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        ui.add(egui::ProgressBar::new(fraction).show_percentage());

        let total = progress
            .total_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "未知大小".to_string());
        ui.label(format!(
            "{} / {}，{}/s",
            format_bytes(progress.downloaded_bytes),
            total,
            format_bytes(progress.speed_bytes_per_sec as u64)
        ));
        ui.label(format!("來源：{}", progress.url));
        ui.horizontal(|ui| {
            if ui.button("取消下載").clicked() {
                if let Some(cancel) = &self.model_cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            if ui.button("重試下載").clicked() {
                self.start_model_job();
            }
        });
    }

    fn draw_history_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("辨識紀錄")
            .open(&mut self.show_history_window)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                if self.history.records().is_empty() {
                    ui.label("尚無紀錄");
                    return;
                }

                if let Some(path) = HistoryManager::history_path() {
                    ui.label(format!("紀錄檔：{}", path.display()));
                    ui.separator();
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for record in self.history.records() {
                        ui.group(|ui| {
                            ui.label(format!(
                                "{} [{}] {:.1} 秒",
                                record.timestamp.format("%Y-%m-%d %H:%M:%S"),
                                record.scenario,
                                record.duration_sec
                            ));
                            let mut text = record.text.clone();
                            ui.add(
                                egui::TextEdit::multiline(&mut text)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        });
                    }
                });

                ui.separator();
                if ui.button("清除紀錄").clicked() {
                    self.history.clear();
                }
            });
    }

    fn draw_settings_window(&mut self, ctx: &egui::Context) {
        let mut should_save = false;
        let mut should_reload_model = false;
        let mut show_settings_window = self.show_settings_window;
        let mut pending_startup_update = None;
        let mut pending_diagnostic_export = false;
        let max_settings_height = (ctx.available_rect().height() - 80.0).max(360.0);

        egui::Window::new("設定")
            .open(&mut show_settings_window)
            .resizable(true)
            .default_width(620.0)
            .default_height(max_settings_height.min(760.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(max_settings_height)
                    .show(ui, |ui| {
                        ui.label("PTT");
                        ui.horizontal(|ui| {
                            ui.label("快捷鍵");
                            ui.monospace(&self.config.hotkeys.record_toggle);
                            if ui
                                .button(if self.hotkey_capture {
                                    "按下新的快捷鍵..."
                                } else {
                                    "捕捉"
                                })
                                .clicked()
                            {
                                self.hotkey_capture = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("手動輸入");
                            let mut hotkey = self.config.hotkeys.record_toggle.clone();
                            if ui.text_edit_singleline(&mut hotkey).lost_focus()
                                && hotkey != self.config.hotkeys.record_toggle
                            {
                                match HotkeyCombo::parse(&hotkey) {
                                    Ok(combo) => {
                                        let display = combo.display();
                                        self.config.hotkeys.record_toggle = display.clone();
                                        if let Err(err) = self.hotkey.update_hotkey(&display) {
                                            self.record_error(err);
                                        }
                                        should_save = true;
                                    }
                                    Err(err) => self.record_error(err),
                                }
                            }
                        });
                        should_save |= ui
                            .checkbox(
                                &mut self.config.hotkeys.global_hotkey_enabled,
                                "啟用全域快捷鍵",
                            )
                            .changed();
                        should_save |= ui
                            .checkbox(
                                &mut self.config.hotkeys.hold_to_record,
                                "按住錄音，放開後轉錄",
                            )
                            .changed();

                        ui.separator();
                        ui.label("Windows 啟動");
                        if ui
                            .checkbox(
                                &mut self.config.startup.launch_on_startup,
                                "登入 Windows 後自動啟動",
                            )
                            .changed()
                        {
                            pending_startup_update = Some((
                                self.config.startup.launch_on_startup,
                                self.config.startup.start_hidden_to_tray,
                            ));
                            should_save = true;
                        }
                        if ui
                            .checkbox(
                                &mut self.config.startup.start_hidden_to_tray,
                                "自動啟動時直接進入系統匣",
                            )
                            .changed()
                        {
                            if self.config.startup.launch_on_startup {
                                pending_startup_update = Some((
                                    self.config.startup.launch_on_startup,
                                    self.config.startup.start_hidden_to_tray,
                                ));
                            }
                            should_save = true;
                        }

                        ui.separator();
                        ui.label("麥克風");
                        ui.horizontal(|ui| {
                            if ui.button("刷新裝置").clicked() {
                                self.audio_devices = self.engine.input_devices();
                            }
                            if ui
                                .button(if self.level_monitor.is_some() {
                                    "停止音量測試"
                                } else {
                                    "開始音量測試"
                                })
                                .clicked()
                            {
                                self.toggle_level_monitor();
                            }
                        });
                        let selected_device_label = self
                            .config
                            .recording
                            .input_device_name
                            .clone()
                            .unwrap_or_else(|| "系統預設".to_string());
                        egui::ComboBox::from_label("輸入裝置")
                            .selected_text(selected_device_label)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        self.config.recording.input_device_name.is_none(),
                                        "系統預設",
                                    )
                                    .clicked()
                                {
                                    self.config.recording.input_device_name = None;
                                    self.engine.update_audio_config(
                                        self.config.recording.input_device_name.clone(),
                                        self.config.recording.gain,
                                    );
                                    should_save = true;
                                }
                                for device in &self.audio_devices {
                                    if ui
                                        .selectable_label(
                                            self.config.recording.input_device_name.as_ref()
                                                == Some(device),
                                            device,
                                        )
                                        .clicked()
                                    {
                                        self.config.recording.input_device_name =
                                            Some(device.clone());
                                        self.engine.update_audio_config(
                                            self.config.recording.input_device_name.clone(),
                                            self.config.recording.gain,
                                        );
                                        should_save = true;
                                    }
                                }
                            });
                        if ui
                            .add(
                                egui::Slider::new(&mut self.config.recording.gain, 0.2..=4.0)
                                    .text("錄音增益"),
                            )
                            .changed()
                        {
                            self.engine.update_audio_config(
                                self.config.recording.input_device_name.clone(),
                                self.config.recording.gain,
                            );
                            should_save = true;
                        }
                        ui.add(egui::ProgressBar::new(self.input_level).text("即時音量"));

                        ui.separator();
                        ui.label("轉錄模式");
                        should_save |= ui
                            .radio_value(
                                &mut self.config.recording.transcription_mode,
                                TranscriptionMode::Stable,
                                "穩定模式：完整錄音檔後轉錄",
                            )
                            .changed();
                        should_save |= ui
                            .radio_value(
                                &mut self.config.recording.transcription_mode,
                                TranscriptionMode::Fast,
                                "快速模式：先顯示草稿狀態，再輸出最終文字",
                            )
                            .changed();

                        ui.separator();
                        ui.label("文字暫存");
                        should_save |= ui
                            .radio_value(
                                &mut self.config.output.buffer_mode,
                                OutputBufferMode::Temporary,
                                "暫存區（不保留到剪貼簿）",
                            )
                            .changed();
                        should_save |= ui
                            .radio_value(
                                &mut self.config.output.buffer_mode,
                                OutputBufferMode::Clipboard,
                                "剪貼簿",
                            )
                            .changed();
                        should_save |= ui
                            .checkbox(
                                &mut self.config.output.auto_inject_focused_window,
                                "轉錄完成後自動注入焦點視窗",
                            )
                            .changed();
                        should_save |= ui
                            .checkbox(
                                &mut self.config.output.manual_review_before_send,
                                "輸出前先預覽，手動確認後送出",
                            )
                            .changed();
                        should_save |= ui
                            .add_enabled(
                                self.config.output.buffer_mode == OutputBufferMode::Temporary,
                                egui::Checkbox::new(
                                    &mut self.config.output.restore_clipboard_after_inject,
                                    "暫存區注入後還原原本剪貼簿文字",
                                ),
                            )
                            .changed();

                        ui.separator();
                        ui.label("簡繁與用語轉換");
                        should_save |= ui
                            .radio_value(
                                &mut self.config.output.chinese_conversion,
                                ChineseConversionMode::Disabled,
                                "不轉換",
                            )
                            .changed();
                        should_save |= ui
                            .radio_value(
                                &mut self.config.output.chinese_conversion,
                                ChineseConversionMode::TraditionalTaiwan,
                                "輸出繁體（台灣用語）",
                            )
                            .changed();
                        should_save |= ui
                            .radio_value(
                                &mut self.config.output.chinese_conversion,
                                ChineseConversionMode::SimplifiedChina,
                                "輸出簡體（中國大陸用語）",
                            )
                            .changed();

                        ui.separator();
                        should_save |= draw_vocabulary_settings(
                            ui,
                            &mut self.config.output.vocabulary.entries,
                        );

                        ui.separator();
                        should_save |=
                            draw_output_rules_settings(ui, &mut self.config.output.rules);

                        ui.separator();
                        ui.label("模型");
                        ui.label(format!("模型目錄：{}", paths::models_dir().display()));
                        if ui
                            .checkbox(&mut self.config.use_cuda, "啟用 CUDA 推論")
                            .changed()
                        {
                            should_save = true;
                            should_reload_model = true;
                        }
                        if ui
                            .button("重新載入目前模型")
                            .on_hover_text("套用模型名稱、目錄與 CUDA 設定")
                            .clicked()
                        {
                            should_save = true;
                            should_reload_model = true;
                        }
                        ui.horizontal(|ui| {
                            ui.label("名稱");
                            let mut model_name = self.config.get_model_name();
                            if ui.text_edit_singleline(&mut model_name).changed() {
                                self.config.model_name = Some(model_name);
                                should_save = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("目錄");
                            let mut models_dir = self.config.get_models_dir();
                            if ui.text_edit_singleline(&mut models_dir).changed() {
                                self.config.models_dir = Some(models_dir);
                                should_save = true;
                            }
                        });

                        ui.separator();
                        ui.label("錄音檔保留");
                        should_save |= ui
                            .add(
                                egui::DragValue::new(&mut self.config.recording.retention_days)
                                    .speed(1)
                                    .clamp_range(0..=3650)
                                    .suffix(" 天"),
                            )
                            .changed();
                        should_save |= ui
                            .add(
                                egui::DragValue::new(&mut self.config.recording.max_total_mb)
                                    .speed(128)
                                    .clamp_range(0..=1024 * 1024)
                                    .suffix(" MB"),
                            )
                            .changed();
                        if ui.button("立即清理超出限制的錄音檔").clicked() {
                            recordings::cleanup_recordings(
                                self.config.recording.retention_days,
                                self.config.recording.max_total_mb,
                            );
                        }

                        ui.separator();
                        ui.label("診斷");
                        if ui.button("匯出診斷包").clicked() {
                            pending_diagnostic_export = true;
                        }
                        ui.label(
                            "診斷包會包含設定、log、最近錯誤、裝置與模型資訊；不包含錄音內容。",
                        );

                        ui.separator();
                        if ui.button("儲存設定").clicked() {
                            should_save = true;
                        }
                    });
            });

        if let Some((enabled, hidden)) = pending_startup_update {
            if let Err(err) = startup::set_launch_on_startup(enabled, hidden) {
                self.record_error(err);
            }
        }
        if pending_diagnostic_export {
            match diagnostics::export_diagnostic_bundle(
                &self.config,
                &self.device_status,
                &self.audio_devices,
                &self.error_log,
            ) {
                Ok(path) => {
                    self.transcription_status = format!("診斷包已匯出：{}", path.display());
                    if let Err(err) = diagnostics::open_diagnostic_folder(&path) {
                        self.record_error(err);
                    }
                }
                Err(err) => self.record_error(format!("匯出診斷包失敗: {err}")),
            }
        }
        if should_save {
            self.save_config();
        }
        if should_reload_model {
            self.reload_model_from_config();
        }
        self.show_settings_window = show_settings_window;
    }

    fn draw_model_manager_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_model_manager_window;
        let mut selected_model: Option<(&'static str, bool)> = None;

        egui::Window::new("模型管理中心")
            .open(&mut open)
            .resizable(true)
            .default_width(760.0)
            .show(ctx, |ui| {
                ui.label(format!("模型資料夾：{}", paths::models_dir().display()));
                ui.label("SHA256 會針對已安裝檔案即時計算；下載時會使用遠端 ETag 可用資訊做驗證。");
                ui.separator();

                for entry in MODEL_CATALOG {
                    let path = models::model_path_for_name(entry.name);
                    let installed = path.exists();
                    let current = self.config.get_model_name() == entry.name;

                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.heading(entry.label);
                            if current {
                                ui.colored_label(egui::Color32::from_rgb(60, 200, 120), "目前使用");
                            }
                            ui.label(entry.approx_size);
                        });
                        ui.label(entry.recommendation);
                        ui.label(format!("URL：{}", models::model_url(entry.file_name)));
                        ui.label(format!(
                            "狀態：{}",
                            if installed { "已安裝" } else { "未下載" }
                        ));

                        if installed {
                            match models::sha256_file(&path) {
                                Ok(hash) => {
                                    ui.monospace(format!("SHA256：{hash}"));
                                }
                                Err(err) => {
                                    ui.colored_label(
                                        egui::Color32::RED,
                                        format!("SHA256 計算失敗：{err}"),
                                    );
                                }
                            }
                        }

                        ui.horizontal(|ui| {
                            if ui.button("使用此模型").clicked() {
                                selected_model = Some((entry.name, false));
                            }
                            if ui
                                .button(if installed { "重新下載" } else { "下載" })
                                .clicked()
                            {
                                selected_model = Some((entry.name, true));
                            }
                        });
                    });
                }
            });

        self.show_model_manager_window = open;
        if let Some((model_name, force_download)) = selected_model {
            self.select_model(model_name, force_download);
        }
    }

    fn draw_error_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("錯誤紀錄")
            .open(&mut self.show_error_window)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                if let Some(path) = log_file_path() {
                    ui.label(format!("Log 檔案：{}", path.display()));
                }

                ui.separator();
                if self.error_log.is_empty() {
                    ui.label("目前沒有錯誤紀錄");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for error in &self.error_log {
                            ui.colored_label(egui::Color32::RED, error);
                            ui.separator();
                        }
                    });
                    if ui.button("清除畫面紀錄").clicked() {
                        self.error_log.clear();
                        self.last_error = None;
                    }
                }
            });
    }

    fn draw_recordings_window(&mut self, ctx: &egui::Context) {
        let mut error = None;

        egui::Window::new("錄音檔管理")
            .open(&mut self.show_recordings_window)
            .resizable(true)
            .default_width(720.0)
            .show(ctx, |ui| {
                ui.label(format!("資料夾：{}", paths::recordings_dir().display()));
                ui.horizontal(|ui| {
                    ui.label("日期篩選");
                    ui.text_edit_singleline(&mut self.recording_filter);
                    if ui.button("開啟資料夾").clicked() {
                        if let Err(err) = recordings::open_recordings_folder() {
                            error = Some(err);
                        }
                    }
                });

                ui.separator();
                let files = recordings::list_recordings(&self.recording_filter);
                if files.is_empty() {
                    ui.label("沒有符合條件的錄音檔");
                    return;
                }

                let total_size = files.iter().map(|file| file.size_bytes).sum::<u64>();
                ui.label(format!(
                    "共 {} 筆，{}",
                    files.len(),
                    format_bytes(total_size)
                ));

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for file in files {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(file.modified.format("%Y-%m-%d %H:%M:%S").to_string());
                                ui.monospace(&file.file_name);
                                ui.label(format_bytes(file.size_bytes));
                            });
                            ui.horizontal(|ui| {
                                if ui.button("播放").clicked() {
                                    if let Err(err) = recordings::play_recording(&file.path) {
                                        error = Some(err);
                                    }
                                }
                                if ui.button("重新轉錄").clicked() {
                                    self.selected_recording_for_retry = Some(file.path.clone());
                                }
                                if ui.button("刪除").clicked() {
                                    if let Err(err) = recordings::delete_recording(&file.path) {
                                        error = Some(err);
                                    }
                                }
                            });
                        });
                    }
                });
            });

        if let Some(error) = error {
            self.record_error(error);
        }
        if let Some(path) = self.selected_recording_for_retry.take() {
            self.retranscribe_recording(path);
        }
    }

    fn draw_model_download_window(&mut self, ctx: &egui::Context) {
        if self.model_cancel.is_none() {
            return;
        }

        egui::Window::new("模型下載")
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(&self.model_status_message);
                if self.model_download.is_some() {
                    self.draw_model_download_status(ui);
                } else {
                    ui.spinner();
                }
            });
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.2} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{} B", bytes as u64)
    }
}

fn draw_vocabulary_settings(ui: &mut egui::Ui, entries: &mut Vec<VocabularyEntry>) -> bool {
    let mut changed = false;
    let mut remove_index = None;

    ui.label("自訂詞庫 / 專有名詞");
    ui.label("source 是辨識可能出現的文字，replacement 是要輸出的正式寫法。");

    egui::Grid::new("vocabulary_grid")
        .num_columns(4)
        .spacing([8.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label("source");
            ui.label("replacement");
            ui.label("保護");
            ui.label("");
            ui.end_row();

            for (index, entry) in entries.iter_mut().enumerate() {
                changed |= ui.text_edit_singleline(&mut entry.source).changed();
                changed |= ui.text_edit_singleline(&mut entry.replacement).changed();
                changed |= ui.checkbox(&mut entry.protect, "").changed();
                if ui.button("刪除").clicked() {
                    remove_index = Some(index);
                }
                ui.end_row();
            }
        });

    if let Some(index) = remove_index {
        entries.remove(index);
        changed = true;
    }

    ui.horizontal(|ui| {
        if ui.button("新增詞彙").clicked() {
            entries.push(VocabularyEntry::blank());
            changed = true;
        }
        if ui.button("清除空白列").clicked() {
            entries.retain(|entry| !entry.is_blank());
            changed = true;
        }
    });

    if entries.is_empty() {
        entries.push(VocabularyEntry::blank());
    }

    changed
}

fn draw_output_rules_settings(ui: &mut egui::Ui, rules: &mut OutputRulesConfig) -> bool {
    let mut changed = false;

    ui.label("輸出規則模板");
    ui.collapsing("聊天", |ui| {
        changed |= draw_rule_template(ui, &mut rules.chat);
    });
    ui.collapsing("寫作", |ui| {
        changed |= draw_rule_template(ui, &mut rules.writing);
    });
    ui.collapsing("程式碼", |ui| {
        changed |= draw_rule_template(ui, &mut rules.code);
    });

    changed
}

fn draw_rule_template(ui: &mut egui::Ui, rules: &mut ScenarioOutputRules) -> bool {
    let mut changed = false;

    changed |= ui
        .checkbox(&mut rules.auto_punctuation, "自動標點")
        .changed();
    changed |= ui
        .checkbox(&mut rules.format_paragraphs, "整理段落")
        .changed();
    changed |= ui
        .checkbox(&mut rules.remove_fillers, "去除語助詞")
        .changed();
    changed |= ui
        .checkbox(&mut rules.preserve_code_symbols, "保留英文符號")
        .changed();
    changed |= ui
        .checkbox(&mut rules.auto_line_breaks, "自動加換行")
        .changed();

    changed
}
