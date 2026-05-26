use eframe::egui;
use speaktype::modules::config::{AppConfig, ChineseConversionMode, OutputBufferMode};
use speaktype::modules::engine::{
    prepare_model_with_progress, run_transcription_request, ModelDownloadProgress, ModelEvent,
    SpeakTypeEngine, TranscriptionEvent, TranscriptionRequest, TranscriptionResult,
};
use speaktype::modules::error::{log_error, log_file_path};
use speaktype::modules::gui::GuiManager;
use speaktype::modules::history::HistoryManager;
use speaktype::modules::input::{GlobalHotkey, HotkeyCombo, HotkeyEvent};
use speaktype::modules::paths;
use speaktype::modules::recordings;
use speaktype::modules::scenario::{Scenario, ScenarioManager};
use speaktype::modules::utils::device::DeviceStatus;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
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
    error_log: Vec<String>,
    transcription_status: String,
    transcription_busy: bool,
    transcription_events: Option<Receiver<TranscriptionEvent>>,
    pending_retry: Option<TranscriptionRequest>,
    pending_preview: Option<String>,
    model_status_message: String,
    model_download: Option<ModelDownloadProgress>,
    model_events: Option<Receiver<ModelEvent>>,
    model_cancel: Option<Arc<AtomicBool>>,
    hotkey_capture: bool,
    recording_filter: String,
    scenario_manager: ScenarioManager,
    history: HistoryManager,
    gui: GuiManager,
    config: AppConfig,
    engine: SpeakTypeEngine,
    hotkey: GlobalHotkey,
    device_status: DeviceStatus,
}

impl SpeakTypeApp {
    pub fn new(ctx: &egui::Context) -> Self {
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
            error_log,
            transcription_status: "就緒".to_string(),
            transcription_busy: false,
            transcription_events: None,
            pending_retry: None,
            pending_preview: None,
            model_status_message: String::new(),
            model_download: None,
            model_events: None,
            model_cancel: None,
            hotkey_capture: false,
            recording_filter: String::new(),
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
        if !app.engine.model_path().exists() {
            app.start_model_job();
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

    fn start_model_job(&mut self) {
        if let Some(cancel) = &self.model_cancel {
            cancel.store(true, Ordering::Relaxed);
        }

        let model_path = self.config.get_model_path();
        let use_cuda = self.config.use_cuda;
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = cancel.clone();
        let (tx, rx) = mpsc::channel();
        self.model_status_message = "模型準備中...".to_string();
        self.model_download = None;
        self.model_events = Some(rx);
        self.model_cancel = Some(cancel);

        thread::spawn(move || {
            prepare_model_with_progress(&model_path, use_cuda, cancel_for_thread, |event| {
                let _ = tx.send(event);
            });
        });
    }

    fn start_transcription_job(&mut self, mut request: TranscriptionRequest) {
        request.output = self.config.output.clone();
        request.scenario = self.scenario_manager.current();
        request.model_path = self.config.get_model_path();
        request.use_cuda = self.config.use_cuda;

        let retry_request = request.clone();
        let (tx, rx) = mpsc::channel();
        self.transcription_events = Some(rx);
        self.transcription_busy = true;
        self.transcription_status = "排入背景轉錄...".to_string();
        self.recording = false;
        self.recording_start = None;
        self.pending_retry = Some(retry_request);

        thread::spawn(move || {
            run_transcription_request(request, |event| {
                let _ = tx.send(event);
            });
        });
    }

    fn retry_last_transcription(&mut self) {
        if self.transcription_busy {
            return;
        }
        if let Some(request) = self.pending_retry.clone() {
            self.start_transcription_job(request);
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

    fn poll_model_events(&mut self) {
        let mut should_clear = false;
        let mut pending_error = None;
        if let Some(rx) = &self.model_events {
            while let Ok(event) = rx.try_recv() {
                match event {
                    ModelEvent::Progress(progress) => {
                        self.model_status_message = "模型下載中...".to_string();
                        self.model_download = Some(progress);
                    }
                    ModelEvent::Ready => {
                        self.engine.mark_model_ready();
                        self.model_status_message = "模型已準備完成".to_string();
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
            }
        }

        if should_clear {
            self.model_events = None;
            self.model_cancel = None;
            self.refresh_device_status();
        }
        if let Some(err) = pending_error {
            self.record_error(err);
        }
    }

    fn poll_transcription_events(&mut self) {
        let mut completed = None;
        let mut failed = None;

        if let Some(rx) = &self.transcription_events {
            while let Ok(event) = rx.try_recv() {
                match event {
                    TranscriptionEvent::Status(status) => self.transcription_status = status,
                    TranscriptionEvent::Completed(result) => completed = Some(result),
                    TranscriptionEvent::Failed(err) => failed = Some(err),
                }
            }
        }

        if let Some(result) = completed {
            self.transcription_busy = false;
            self.transcription_events = None;
            self.accept_transcription(result);
        }

        if let Some(err) = failed {
            self.transcription_busy = false;
            self.transcription_events = None;
            self.transcription_status = "失敗".to_string();
            self.record_error(err);
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
        self.poll_model_events();
        self.poll_transcription_events();
        self.capture_hotkey_from_input(ctx);

        if self.config.hotkeys.global_hotkey_enabled {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.transcription_busy || self.model_events.is_some() {
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
                if ui.button("設定").clicked() {
                    self.show_settings_window = true;
                }
                if ui.button("錯誤紀錄").clicked() {
                    self.show_error_window = true;
                }
                if ui.button("刷新狀態").clicked() {
                    self.refresh_device_status();
                }
            });

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
                    !self.transcription_busy && self.model_events.is_none(),
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

        egui::Window::new("設定")
            .open(&mut show_settings_window)
            .resizable(true)
            .default_width(460.0)
            .show(ctx, |ui| {
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
                if ui.button("儲存設定").clicked() {
                    should_save = true;
                }
            });

        if should_save {
            self.save_config();
        }
        if should_reload_model {
            self.reload_model_from_config();
        }
        self.show_settings_window = show_settings_window;
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
    }

    fn draw_model_download_window(&mut self, ctx: &egui::Context) {
        if self.model_events.is_none() {
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
