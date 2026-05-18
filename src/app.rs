use eframe::egui;
use speaktype::modules::config::{AppConfig, ChineseConversionMode, OutputBufferMode};
use speaktype::modules::engine::SpeakTypeEngine;
use speaktype::modules::gui::GuiManager;
use speaktype::modules::history::HistoryManager;
use speaktype::modules::input::{GlobalHotkey, HotkeyEvent};
use speaktype::modules::scenario::{Scenario, ScenarioManager};
use speaktype::modules::utils::device::DeviceStatus;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

pub struct SpeakTypeApp {
    recording: bool,
    last_result: String,
    last_error: Option<String>,
    recording_start: Option<Instant>,
    scratch_text: String,
    show_history_window: bool,
    show_settings_window: bool,
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
        let model_path = config.get_model_path();
        let engine = SpeakTypeEngine::new(model_path);
        let mut device_status = DeviceStatus::detect();
        device_status.model = engine.model_status_text();
        let last_error = engine.model_error().map(str::to_string);

        Self {
            recording: false,
            last_result: String::new(),
            last_error,
            recording_start: None,
            scratch_text: String::new(),
            show_history_window: false,
            show_settings_window: false,
            scenario_manager: ScenarioManager::with_current(current_scenario),
            history: HistoryManager::new(),
            gui: GuiManager::new(),
            config,
            engine,
            hotkey: GlobalHotkey::new(),
            device_status,
        }
    }

    fn toggle_recording_action(&mut self) {
        match self.engine.toggle_recording(&self.config.output) {
            Ok(Some(text)) => {
                self.accept_transcription(text);
            }
            Ok(None) => {
                // 開始錄音
                self.last_error = None;
                self.recording = true;
                self.recording_start = Some(Instant::now());
            }
            Err(e) => {
                self.last_error = Some(e);
                self.recording = self.engine.is_recording();
            }
        }
    }

    fn start_ptt_recording(&mut self) {
        match self.engine.start_recording() {
            Ok(()) => {
                self.last_error = None;
                self.recording = true;
                self.recording_start = Some(Instant::now());
            }
            Err(e) => {
                self.last_error = Some(e);
                self.recording = self.engine.is_recording();
            }
        }
    }

    fn finish_ptt_recording(&mut self) {
        match self
            .engine
            .stop_recording_and_transcribe(&self.config.output)
        {
            Ok(text) => self.accept_transcription(text),
            Err(e) => {
                self.last_error = Some(e);
                self.recording = self.engine.is_recording();
            }
        }
    }

    fn accept_transcription(&mut self, text: String) {
        self.last_result = text.clone();
        self.scratch_text = text.clone();
        self.last_error = None;
        self.recording = self.engine.is_recording();
        self.recording_start = None;
        self.history.add_record(
            text,
            self.scenario_manager.current().name().to_string(),
            0.0,
        );
    }

    fn select_scenario(&mut self, scenario: Scenario) {
        self.scenario_manager.select(scenario);
        self.config.last_scenario = Some(scenario.name().to_string());
        if let Err(err) = self.config.save() {
            self.last_error = Some(format!("儲存設定失敗: {}", err));
        }
    }

    fn save_config(&mut self) {
        if let Err(err) = self.config.save() {
            self.last_error = Some(format!("儲存設定失敗: {}", err));
        } else {
            self.last_error = None;
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
        if self.config.hotkeys.global_hotkey_enabled {
            ctx.request_repaint_after(Duration::from_millis(50));
        }

        if self.config.hotkeys.global_hotkey_enabled && self.config.hotkeys.hold_to_record {
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
                if ui.button("設定").clicked() {
                    self.show_settings_window = true;
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
            if self.gui.draw_record_button(ui, self.recording) {
                self.toggle_recording_action();
            }

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
    }
}

impl SpeakTypeApp {
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

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for record in self.history.records() {
                        ui.group(|ui| {
                            ui.label(format!(
                                "{} [{}]",
                                record.timestamp.format("%Y-%m-%d %H:%M:%S"),
                                record.scenario
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

        egui::Window::new("設定")
            .open(&mut self.show_settings_window)
            .resizable(true)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label("PTT");
                ui.horizontal(|ui| {
                    ui.label("快捷鍵");
                    ui.monospace(&self.config.hotkeys.record_toggle);
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
                if ui.button("儲存設定").clicked() {
                    should_save = true;
                }
            });

        if should_save {
            self.save_config();
        }
    }
}
