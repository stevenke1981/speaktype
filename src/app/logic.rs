//! `App` behaviour: construction, events, recording pipeline and pushing state to the UI.

use super::*;

impl App {
    pub(super) fn new(ui: &MainWindow, indicator: Option<Indicator>, opts: &Options) -> Self {
        speaktype::modules::utils::device::prefetch_gpu_name();
        let config = AppConfig::load();
        let current_scenario =
            config.last_scenario.as_deref().and_then(Scenario::from_name).unwrap_or(Scenario::Chat);
        recordings::cleanup_recordings(config.recording.retention_days, config.recording.max_total_mb);
        cleanup_stale_temp_files();

        let engine = SpeakTypeEngine::new(config.get_model_path(), config.use_cuda);
        let device_status = DeviceStatus::detect(engine.model_status_text(), config.use_cuda);
        let last_error = engine.model_error().map(str::to_string);
        if let Some(error) = &last_error {
            log_error("app init", error);
        }
        let error_log = last_error.iter().cloned().collect();
        let model_worker = ModelWorker::start();
        model_worker.set_waker(wake);
        let tray = create_tray(Arc::new(wake));

        let mut app = Self {
            ui: ui.as_weak(),
            indicator,
            anim_timer: Timer::default(),
            level_timer: Timer::default(),
            gpu_timer: Timer::default(),
            rules_model: Rc::new(VecModel::default()),
            vocab_model: Rc::new(VecModel::default()),
            history_rows: Rc::new(VecModel::default()),
            history_filter: String::new(),
            recording_filter: String::new(),
            audio_devices: Vec::new(),
            audio_devices_loaded: false,
            last_latency: None,
            recording: false,
            recording_start: None,
            last_result: String::new(),
            scratch_text: String::new(),
            last_error,
            error_log,
            transcription_status: "就緒".to_string(),
            transcription_busy: false,
            transcription_job: 0,
            transcription_draft: String::new(),
            pending_retry: None,
            pending_preview: None,
            model_status_message: String::new(),
            model_download: None,
            model_failed: false,
            model_cancel: None,
            model_worker,
            model_sha256_errors: HashMap::new(),
            model_download_name: None,
            pending_download_confirm: None,
            hotkey_capture: false,
            level_monitor: None,
            input_level: 0.0,
            hidden_to_tray: opts.start_hidden_to_tray && tray.is_some(),
            tray,
            exit_requested: false,
            scenario_manager: ScenarioManager::with_current(current_scenario),
            history: HistoryManager::load(),
            config,
            engine,
            hotkey: GlobalHotkey::new("Ctrl+Shift+L"),
            device_status,
            started: opts.startup_started,
            first_frame_seen: false,
            startup_probe: opts.startup_probe.clone(),
        };
        app.hotkey.set_waker(wake);
        let record_toggle = app.config.hotkeys.record_toggle.clone();
        if let Err(err) = app.hotkey.update_hotkey(&record_toggle) {
            app.record_error(format!("快捷鍵設定無效，已使用預設值: {}", err));
        }
        app.engine.update_audio_config(app.config.recording.input_device_name.clone(), app.config.recording.gain);
        if std::env::var_os("SPEAKTYPE_SKIP_MODEL_LOAD").is_none() {
            app.start_model_job();
        }
        ui.set_rules(ModelRc::from(app.rules_model.clone()));
        ui.set_vocab(ModelRc::from(app.vocab_model.clone()));
        ui.set_history(ModelRc::from(app.history_rows.clone()));
        ui.set_tray_available(app.tray.is_some());
        ui.set_languages(strings(LANGUAGES.iter().map(|(_, l)| *l)));
        ui.set_methods(strings(InjectionMethod::all().iter().map(|m| m.label())));
        ui.set_rule_methods(strings(
            std::iter::once("預設方式").chain(InjectionMethod::all().iter().map(|m| m.label())),
        ));
        ui.set_rule_scenarios(strings(std::iter::once("目前情境").chain(Scenario::all().iter().map(|s| s.name()))));
        ui.set_themes(strings(THEMES.iter().map(|t| t.label)));
        ui.set_voice_command_lines(strings(voice_commands::available_commands().iter().map(|c| {
            format!("「{}」— {}", voice_commands::voice_command_label(c), voice_commands::voice_command_description(c))
        })));
        ui.set_log_path(ss(log_file_path().unwrap_or_default().display().to_string()));
        ui.set_recordings_dir(ss(paths::recordings_dir().display().to_string()));
        app
    }

    pub(super) fn apply_screenshot_hooks(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        match std::env::var("SPEAKTYPE_UI_PAGE").as_deref() {
            Ok("settings") => self.open_page(1),
            Ok("history") => self.open_page(2),
            Ok("models") => self.open_page(3),
            Ok("speed") => {
                self.open_page(1);
                ui.set_jump_to_speed(true);
            }
            _ => {}
        }
        if std::env::var_os("SPEAKTYPE_DEMO_HISTORY").is_some() && self.history.records().is_empty() {
            let samples = [
                ("我們明天下午三點在會議室討論新版的語音輸入流程。", "LINE.exe", "聊天", 2.4, 612.0),
                ("Please review the pull request before Friday, thanks.", "chrome.exe", "寫作", 3.1, 845.0),
                ("let total = items.iter().map(|x| x.price).sum();", "Code.exe", "程式碼", 4.0, 1032.0),
            ];
            for (text, app, sc, dur, lat) in samples {
                self.history.add_entry(RecognitionRecord {
                    timestamp: chrono::Local::now(),
                    text: text.to_string(),
                    scenario: sc.to_string(),
                    duration_sec: dur,
                    app: Some(app.to_string()),
                    latency_ms: Some(lat),
                    raw_text: None,
                });
            }
            self.push_history();
        }
        if std::env::var_os("SPEAKTYPE_DEMO_INDICATOR").is_some() {
            self.recording = true;
            self.recording_start = Some(Instant::now() - Duration::from_millis(7800));
            self.input_level = 0.35;
            self.update_indicator();
            self.push_main();
        }
    }

    // ------------------------------------------------------------------ events

    /// Drain hotkey, tray and worker events (called on wake-ups).
    pub(super) fn pump(&mut self) {
        self.handle_tray_actions();
        let mut changed = false;
        while let Some(event) = self.model_worker.try_recv() {
            changed = true;
            match event {
                WorkerEvent::Model(event) => self.handle_model_event(event),
                WorkerEvent::Transcription(event) => self.handle_transcription_event(event),
            }
        }
        if self.config.hotkeys.global_hotkey_enabled && !self.hotkey_capture {
            while let Some(event) = self.hotkey.poll_record_hotkey_event() {
                changed = true;
                match (self.config.hotkeys.hold_to_record, event) {
                    (true, HotkeyEvent::Pressed) => self.start_ptt_recording(),
                    (true, HotkeyEvent::Released) => self.finish_ptt_recording(),
                    (false, HotkeyEvent::Pressed) => self.toggle_recording_action(),
                    (false, HotkeyEvent::Released) => {}
                }
            }
        } else {
            while self.hotkey.poll_record_hotkey_event().is_some() {}
        }
        if changed {
            self.push_main();
        }
    }

    pub(super) fn on_first_frame(&mut self) {
        if self.first_frame_seen {
            return;
        }
        self.first_frame_seen = true;
        // Open the warm mic stream only after the first frame is on screen.
        if self.config.recording.keep_mic_warm {
            self.engine.set_keep_mic_warm(true);
        }
        let ms = self.started.elapsed().as_secs_f64() * 1000.0;
        log_error("startup", format!("time to first frame: {ms:.0} ms"));
        if let Some(path) = self.startup_probe.clone() {
            let _ = std::fs::write(&path, format!("{ms:.0}\n"));
            if std::env::var_os("SPEAKTYPE_PROBE_KEEP_OPEN").is_none() {
                self.exit_requested = true;
                let _ = slint::quit_event_loop();
            }
        }
        self.start_gpu_probe_timer();
    }

    pub(super) fn start_gpu_probe_timer(&mut self) {
        if self.device_status.gpu != "偵測中…" {
            return;
        }
        self.gpu_timer.start(TimerMode::Repeated, Duration::from_millis(300), || {
            with_app(|a| {
                a.refresh_device_status();
                a.push_main();
                if a.device_status.gpu != "偵測中…" {
                    a.gpu_timer.stop();
                }
            })
        });
    }

    pub(super) fn handle_tray_actions(&mut self) {
        while let Some(action) = self.tray.as_ref().and_then(TrayManager::poll_action) {
            match action {
                TrayAction::ShowMain => self.show_from_tray(),
                TrayAction::ToggleRecording => {
                    self.toggle_recording_action();
                    self.push_main();
                }
                TrayAction::OpenSettings => {
                    self.show_from_tray();
                    self.open_page(1);
                }
                TrayAction::OpenHistory => {
                    self.show_from_tray();
                    self.open_page(2);
                }
                TrayAction::Exit => {
                    self.exit_requested = true;
                    let _ = slint::quit_event_loop();
                }
            }
        }
    }

    pub(super) fn on_action(&mut self, name: &str) {
        match name {
            "open-settings" => self.open_page(1),
            "open-history" => self.open_page(2),
            "open-models" => self.open_page(3),
            "open-recordings" => self.open_page(4),
            "open-errors" => self.open_page(5),
            "refresh-devices" => {
                self.refresh_device_status();
                self.push_main();
            }
            "hide-to-tray" => self.minimize_to_tray(),
            "toggle-recording" => {
                self.toggle_recording_action();
                self.push_main();
            }
            "retry-last" => {
                self.retry_last_transcription();
                self.push_main();
            }
            "send-preview" => {
                self.send_pending_preview();
                self.push_main();
            }
            "copy-last" => {
                let text = self.last_result.clone();
                self.copy_text(&text);
            }
            "cancel-model" => {
                if let Some(cancel) = &self.model_cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            "retry-model" | "reload-model" => {
                self.reload_model_from_config();
                self.push_main();
            }
            "refresh-audio-devices" => {
                self.audio_devices = self.engine.input_devices();
                self.push_devices();
            }
            "toggle-level-test" => self.toggle_level_monitor(),
            "hotkey-capture" => {
                self.hotkey_capture = !self.hotkey_capture;
                self.push_settings_misc();
            }
            "hotkey-capture-cancel" => {
                self.hotkey_capture = false;
                self.push_settings_misc();
            }
            "vocab-add" => {
                self.config.output.vocabulary.entries.push(VocabularyEntry::blank());
                self.save_config();
                self.push_vocab();
            }
            "vocab-prune" => {
                let entries = &mut self.config.output.vocabulary.entries;
                entries.retain(|e| !e.is_blank());
                self.save_config();
                self.push_vocab();
            }
            "rule-add" => {
                self.config.injection.app_rules.push(blank_rule(String::new()));
                self.save_config();
                self.push_rules();
            }
            "rule-add-last-app" => {
                if let Some(app) = self.history.records().iter().find_map(|r| r.app.clone()) {
                    if !self.config.injection.app_rules.iter().any(|r| r.process.eq_ignore_ascii_case(&app)) {
                        self.config.injection.app_rules.push(blank_rule(app));
                        self.save_config();
                        self.push_rules();
                    }
                }
            }
            "rule-reset" => {
                self.config.injection.app_rules = default_app_rules();
                self.save_config();
                self.push_rules();
            }
            "ai-preset-ollama" | "ai-preset-openai" | "ai-preset-groq" => {
                let (endpoint, model) = match name {
                    "ai-preset-ollama" => ("http://localhost:11434/v1/chat/completions", "qwen2.5:3b"),
                    "ai-preset-openai" => ("https://api.openai.com/v1/chat/completions", "gpt-4o-mini"),
                    _ => ("https://api.groq.com/openai/v1/chat/completions", "llama-3.1-8b-instant"),
                };
                self.config.ai_cleanup.endpoint = endpoint.into();
                self.config.ai_cleanup.model = model.into();
                self.save_config();
                self.push_settings();
            }
            "cleanup-recordings" => {
                recordings::cleanup_recordings(self.config.recording.retention_days, self.config.recording.max_total_mb);
            }
            "apply-startup" => {
                if let Err(err) = startup::set_launch_on_startup(
                    self.config.startup.launch_on_startup,
                    self.config.startup.start_hidden_to_tray,
                ) {
                    self.record_error(err);
                    self.push_main();
                }
            }
            "export-diagnostics" => {
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
                self.push_main();
            }
            "history-clear" => {
                self.history.clear();
                self.push_history();
            }
            "download-confirm" => {
                if let Some(name) = self.pending_download_confirm.take() {
                    self.select_model(&name, true);
                }
                self.push_models();
                self.push_main();
            }
            "download-cancel-confirm" => {
                self.pending_download_confirm = None;
                self.push_models();
            }
            "open-recordings-folder" => {
                if let Err(err) = recordings::open_recordings_folder() {
                    self.record_error(err);
                    self.push_main();
                }
            }
            "errors-clear" => {
                self.error_log.clear();
                self.last_error = None;
                self.push_errors();
                self.push_main();
            }
            other => log_error("ui", format!("unknown action {other}")),
        }
    }

    pub(super) fn open_page(&mut self, page: i32) {
        let Some(ui) = self.ui.upgrade() else { return };
        match page {
            1 => {
                if !self.audio_devices_loaded {
                    // Enumerate devices only when Settings is opened (slow on Windows).
                    self.audio_devices_loaded = true;
                    self.audio_devices = self.engine.input_devices();
                }
                self.push_settings();
                self.push_devices();
            }
            2 => self.push_history(),
            3 => self.push_models(),
            4 => self.push_recordings(),
            5 => self.push_errors(),
            _ => {}
        }
        ui.set_page(page);
    }

    // ------------------------------------------------------------------ tray / window

    pub(super) fn minimize_to_tray(&mut self) {
        if self.tray.is_none() {
            return;
        }
        self.hidden_to_tray = true;
        if let Some(ui) = self.ui.upgrade() {
            let _ = ui.hide();
        }
    }

    pub(super) fn show_from_tray(&mut self) {
        self.hidden_to_tray = false;
        if let Some(ui) = self.ui.upgrade() {
            let _ = ui.show();
            use slint::winit_030::WinitWindowAccessor;
            ui.window().with_winit_window(|w| {
                w.set_minimized(false);
                w.focus_window();
            });
        }
        self.push_main();
    }

    pub(super) fn shutdown(&mut self) {
        if let Some(mut monitor) = self.level_monitor.take() {
            monitor.stop();
        }
        self.anim_timer.stop();
        self.level_timer.stop();
        self.gpu_timer.stop();
    }

    // ------------------------------------------------------------------ recording

    pub(super) fn toggle_recording_action(&mut self) {
        if self.transcription_busy {
            return;
        }
        match self.engine.toggle_recording(
            &self.config.output,
            self.scenario_manager.current(),
            self.config.recording.transcription_mode,
        ) {
            Ok(Some(request)) => self.start_transcription_job(request),
            Ok(None) => self.on_recording_started(),
            Err(e) => {
                self.record_error(e);
                self.recording = self.engine.is_recording();
            }
        }
        self.update_indicator();
    }

    pub(super) fn on_recording_started(&mut self) {
        self.last_error = None;
        self.recording = true;
        self.recording_start = Some(Instant::now());
        self.transcription_status = "錄音中...".to_string();
    }

    pub(super) fn start_ptt_recording(&mut self) {
        if self.transcription_busy {
            return;
        }
        match self.engine.start_recording() {
            Ok(()) => self.on_recording_started(),
            Err(e) => {
                self.record_error(e);
                self.recording = self.engine.is_recording();
            }
        }
        self.update_indicator();
    }

    pub(super) fn finish_ptt_recording(&mut self) {
        match self.engine.stop_recording_capture(
            &self.config.output,
            self.scenario_manager.current(),
            self.config.recording.transcription_mode,
        ) {
            Ok(request) => self.start_transcription_job(request),
            Err(e) => {
                self.record_error(e);
                self.recording = self.engine.is_recording();
            }
        }
        self.update_indicator();
    }

    /// Show/hide the floating pill; its animation timer runs only while it is visible.
    pub(super) fn update_indicator(&mut self) {
        let want = self.config.indicator.enabled && (self.recording || self.transcription_busy);
        let Some(ind) = self.indicator.as_mut() else { return };
        if want {
            ind.window.set_recording(self.recording);
            if !ind.show() {
                // Native window not created yet (event loop not running): retry.
                Timer::single_shot(Duration::from_millis(30), || with_app(|a| a.update_indicator()));
            }
            if !self.anim_timer.running() {
                self.anim_timer
                    .start(TimerMode::Repeated, Duration::from_millis(50), || with_app(|a| a.tick_indicator()));
            }
            self.tick_indicator();
        } else {
            ind.hide();
            self.anim_timer.stop();
        }
    }

    pub(super) fn tick_indicator(&mut self) {
        let level = match &self.level_monitor {
            Some(m) => m.level(),
            None => self.input_level,
        };
        let Some(ind) = self.indicator.as_ref() else { return };
        let w = &ind.window;
        w.set_phase(self.started.elapsed().as_secs_f32());
        w.set_recording(self.recording);
        w.set_level(level);
        let elapsed = self.recording_start.map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0);
        w.set_elapsed(ss(format!("{elapsed:.1}s")));
    }

    pub(super) fn accept_transcription(&mut self, result: TranscriptionResult) {
        self.last_result = result.text.clone();
        self.scratch_text = result.text.clone();
        self.last_error = None;
        self.recording = self.engine.is_recording();
        self.recording_start = None;
        self.engine.set_last_recording_path(result.recording_path.clone());
        self.last_latency = Some(result.timings.clone());
        self.history.add_entry(RecognitionRecord {
            timestamp: chrono::Local::now(),
            text: result.text.clone(),
            scenario: self.scenario_manager.current().name().to_string(),
            duration_sec: result.duration_sec,
            app: result.app.clone(),
            latency_ms: Some(result.timings.total_ms),
            raw_text: Some(result.raw_text.clone()),
        });
        if result.delivered {
            self.transcription_status = "已完成".to_string();
            self.pending_preview = None;
        } else {
            self.transcription_status = "等待手動送出".to_string();
            self.pending_preview = Some(result.text);
        }
        self.push_history();
    }

    pub(super) fn select_scenario(&mut self, scenario: Scenario) {
        self.scenario_manager.select(scenario);
        self.config.last_scenario = Some(scenario.name().to_string());
        self.save_config();
    }

    pub(super) fn save_config(&mut self) {
        if let Err(err) = self.config.save() {
            self.record_error(format!("儲存設定失敗: {}", err));
        }
    }

    pub(super) fn record_error(&mut self, error: String) {
        log_error("gui", &error);
        self.last_error = Some(error.clone());
        self.error_log.insert(0, error);
        self.error_log.truncate(50);
    }

    pub(super) fn refresh_device_status(&mut self) {
        self.device_status = DeviceStatus::detect(self.engine.model_status_text(), self.config.use_cuda);
    }

    pub(super) fn reload_model_from_config(&mut self) {
        self.engine.set_model_path(self.config.get_model_path(), self.config.use_cuda);
        self.start_model_job();
        self.refresh_device_status();
    }

    pub(super) fn select_model(&mut self, model_name: &str, force_download: bool) {
        self.config.model_name = Some(model_name.to_string());
        self.model_download_name = Some(model_name.to_string());
        self.model_sha256_errors.remove(model_name);
        self.save_config();
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

    pub(super) fn start_model_job(&mut self) {
        if let Some(cancel) = &self.model_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        let model_path = self.config.get_model_path();
        let cancel = Arc::new(AtomicBool::new(false));
        self.model_status_message =
            if model_path.exists() { "模型預載中...".to_string() } else { "模型準備中...".to_string() };
        self.model_download = None;
        self.model_failed = false;
        self.model_cancel = Some(cancel.clone());
        if let Err(err) =
            self.model_worker.load_model(model_path, self.config.use_cuda, cancel, self.config.transcription.warm_up)
        {
            self.record_error(format!("模型 worker 啟動失敗: {err}"));
        }
    }

    pub(super) fn start_transcription_job(&mut self, mut request: TranscriptionRequest) {
        request.output = self.config.output.clone();
        request.scenario = self.scenario_manager.current();
        request.model_path = self.config.get_model_path();
        request.use_cuda = self.config.use_cuda;
        request.mode = self.config.recording.transcription_mode;
        request.transcription = self.config.transcription.clone();
        request.injection = self.config.injection.clone();
        request.ai = self.config.ai_cleanup.clone();

        let timeout = transcription_timeout(request.duration_sec);
        self.pending_retry = Some(request.clone());
        self.transcription_busy = true;
        self.transcription_job += 1;
        let job = self.transcription_job;
        Timer::single_shot(timeout, move || with_app(move |a| a.transcription_watchdog(job, timeout)));
        self.transcription_draft.clear();
        self.transcription_status = "排入背景轉錄...".to_string();
        self.recording = false;
        self.recording_start = None;
        if let Err(err) = self.model_worker.transcribe(request) {
            self.transcription_busy = false;
            self.record_error(format!("轉錄 worker 啟動失敗: {err}"));
        }
        self.update_indicator();
    }

    pub(super) fn retry_last_transcription(&mut self) {
        if self.transcription_busy {
            return;
        }
        if let Some(mut request) = self.pending_retry.clone() {
            request.released_at = Instant::now();
            request.target_app = None;
            self.start_transcription_job(request);
        }
    }

    pub(super) fn retranscribe_recording(&mut self, path: PathBuf) {
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
                    transcription: self.config.transcription.clone(),
                    injection: self.config.injection.clone(),
                    ai: self.config.ai_cleanup.clone(),
                    target_app: None,
                    released_at: Instant::now(),
                };
                self.start_transcription_job(request);
            }
            Err(err) => self.record_error(err),
        }
    }

    pub(super) fn send_pending_preview(&mut self) {
        let Some(text) = self.pending_preview.clone() else { return };
        let mut output = self.config.output.clone();
        output.auto_inject_focused_window = true;
        match self.engine.deliver_text_now(&text, &output, &self.config.injection) {
            Ok(()) => {
                self.pending_preview = None;
                self.transcription_status = "已送出".to_string();
                self.last_error = None;
            }
            Err(err) => self.record_error(err),
        }
    }

    pub(super) fn copy_text(&mut self, text: &str) {
        match arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())) {
            Ok(()) => self.transcription_status = "已複製到剪貼簿".to_string(),
            Err(err) => self.record_error(format!("複製失敗: {err}")),
        }
        self.push_main();
    }

    pub(super) fn toggle_level_monitor(&mut self) {
        if let Some(mut monitor) = self.level_monitor.take() {
            monitor.stop();
            self.input_level = 0.0;
            self.level_timer.stop();
        } else {
            match LevelMonitor::start(self.config.recording.input_device_name.clone(), self.config.recording.gain) {
                Ok(monitor) => {
                    self.level_monitor = Some(monitor);
                    self.level_timer.start(TimerMode::Repeated, Duration::from_millis(100), || {
                        with_app(|a| {
                            if let Some(m) = &a.level_monitor {
                                a.input_level = m.level();
                            }
                            if let Some(ui) = a.ui.upgrade() {
                                ui.set_input_level(a.input_level.clamp(0.0, 1.0));
                            }
                        })
                    });
                }
                Err(err) => self.record_error(format!("音量測試失敗: {}", err)),
            }
        }
        self.push_settings_misc();
        self.push_main();
    }

    pub(super) fn handle_model_event(&mut self, event: ModelEvent) {
        let mut should_clear = false;
        match event {
            ModelEvent::Progress(progress) => {
                self.model_status_message = "模型下載中...".to_string();
                self.model_download = Some(progress);
            }
            ModelEvent::Ready => {
                self.engine.mark_model_ready();
                self.model_status_message = "模型已常駐準備完成".to_string();
                self.model_download = None;
                if let Some(name) = self.model_download_name.take() {
                    self.model_sha256_errors.remove(&name);
                }
                should_clear = true;
            }
            ModelEvent::Cancelled => {
                self.engine.mark_model_failed("模型下載已取消".to_string());
                self.model_status_message = "模型下載已取消".to_string();
                self.model_download = None;
                self.model_download_name = None;
                self.model_failed = true;
                should_clear = true;
            }
            ModelEvent::Warning(warn) => {
                if let Some(name) = &self.model_download_name {
                    self.model_sha256_errors.insert(name.clone(), warn);
                }
            }
            ModelEvent::Failed(err) => {
                self.engine.mark_model_failed(err.clone());
                self.model_status_message = "模型準備失敗".to_string();
                self.model_download = None;
                self.model_failed = true;
                should_clear = true;
                self.record_error(err);
            }
        }
        if should_clear {
            self.model_cancel = None;
            self.refresh_device_status();
            self.push_models();
        }
    }

    pub(super) fn transcription_watchdog(&mut self, job: u64, timeout: Duration) {
        if !self.transcription_busy || self.transcription_job != job {
            return;
        }
        self.transcription_busy = false;
        self.transcription_status = "逾時".to_string();
        self.record_error(format!(
            "轉錄超過 {} 秒仍未完成，已取消等待。可能是 CPU 太慢：請改用較小的模型（small q5_1），或安裝 CUDA 版以使用 NVIDIA GPU。詳見記錄檔。",
            timeout.as_secs()
        ));
        self.update_indicator();
    }

    pub(super) fn handle_transcription_event(&mut self, event: TranscriptionEvent) {
        match event {
            TranscriptionEvent::Status(status) => self.transcription_status = status,
            TranscriptionEvent::Draft(draft) => self.transcription_draft = draft,
            TranscriptionEvent::Completed(result) => {
                // A late result after the watchdog fired is still delivered.
                self.transcription_busy = false;
                self.accept_transcription(result);
                self.update_indicator();
            }
            TranscriptionEvent::Failed(err) => {
                self.transcription_busy = false;
                self.transcription_status = "失敗".to_string();
                self.record_error(err);
                self.update_indicator();
            }
        }
    }

    pub(super) fn set_hotkey(&mut self, text: &str) {
        match HotkeyCombo::parse(text) {
            Ok(combo) => {
                let display = combo.display();
                self.config.hotkeys.record_toggle = display.clone();
                if let Err(err) = self.hotkey.update_hotkey(&display) {
                    self.record_error(err);
                }
                self.save_config();
            }
            Err(err) => self.record_error(err),
        }
        self.push_settings();
        self.push_main();
    }
}

/// Upper bound for one transcription before the UI gives up waiting:
/// 90 s minimum (covers a first-time model load), plus 20x the clip length.
pub(super) fn transcription_timeout(duration_sec: f32) -> Duration {
    let secs = 90.0 + 20.0 * duration_sec.max(0.0) as f64;
    Duration::from_secs_f64(secs.min(900.0))
}

#[cfg(test)]
mod watchdog_tests {
    #[test]
    fn timeout_scales_and_caps() {
        assert_eq!(super::transcription_timeout(0.0).as_secs(), 90);
        assert_eq!(super::transcription_timeout(10.0).as_secs(), 290);
        assert_eq!(super::transcription_timeout(1000.0).as_secs(), 900);
    }
}
