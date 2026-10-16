//! Moving state between `AppConfig` / `App` and the Slint properties.

use super::*;

impl App {
    pub(super) fn settings_to_ui(&self) -> Settings {
        let c = &self.config;
        Settings {
            gain: c.recording.gain,
            hotkey: ss(&c.hotkeys.record_toggle),
            global_hotkey: c.hotkeys.global_hotkey_enabled,
            hold_to_record: c.hotkeys.hold_to_record,
            fast_mode: c.recording.transcription_mode == TranscriptionMode::Fast,
            buffer_clipboard: c.output.buffer_mode == OutputBufferMode::Clipboard,
            auto_inject: c.output.auto_inject_focused_window,
            manual_review: c.output.manual_review_before_send,
            restore_clipboard: c.output.restore_clipboard_after_inject,
            conversion: match c.output.chinese_conversion {
                ChineseConversionMode::Disabled => 0,
                ChineseConversionMode::TraditionalTaiwan => 1,
                ChineseConversionMode::SimplifiedChina => 2,
            },
            voice_commands: c.output.voice_commands_enabled,
            tpl_chat: tpl_to_ui(&c.output.rules.chat),
            tpl_writing: tpl_to_ui(&c.output.rules.writing),
            tpl_code: tpl_to_ui(&c.output.rules.code),
            language: LANGUAGES.iter().position(|(code, _)| *code == c.transcription.language).unwrap_or(0) as i32,
            fast_encoder: c.transcription.fast_encoder,
            traditional_prompt: c.transcription.prefer_traditional_prompt,
            keep_mic_warm: c.recording.keep_mic_warm,
            indicator: c.indicator.enabled,
            default_method: method_index(c.injection.default_method),
            restore_delay_ms: c.injection.restore_delay_ms.min(i32::MAX as u64) as i32,
            refocus: c.injection.refocus_original_window,
            ai_enabled: c.ai_cleanup.enabled,
            ai_endpoint: ss(&c.ai_cleanup.endpoint),
            ai_model: ss(&c.ai_cleanup.model),
            ai_key: ss(&c.ai_cleanup.api_key),
            ai_timeout_ms: c.ai_cleanup.timeout_ms.min(i32::MAX as u64) as i32,
            ai_instructions: ss(&c.ai_cleanup.custom_instructions),
            use_cuda: c.use_cuda,
            gpu_mode: c.gpu_mode().index(),
            model_name: ss(c.get_model_name()),
            models_dir: ss(c.get_models_dir()),
            retention_days: c.recording.retention_days.min(i32::MAX as u32) as i32,
            max_total_mb: c.recording.max_total_mb.min(i32::MAX as u64) as i32,
            theme: ui_style::theme_index(c.ui_theme.as_deref()) as i32,
            launch_on_startup: c.startup.launch_on_startup,
            start_hidden: c.startup.start_hidden_to_tray,
        }
    }

    /// Read the whole settings form back into the config and save it.
    pub(super) fn read_settings(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let s = ui.get_s();
        let old_warm = self.config.recording.keep_mic_warm;
        let old_gain = self.config.recording.gain;
        let c = &mut self.config;
        c.recording.gain = s.gain.clamp(0.2, 4.0);
        c.hotkeys.global_hotkey_enabled = s.global_hotkey;
        c.hotkeys.hold_to_record = s.hold_to_record;
        c.recording.transcription_mode = if s.fast_mode { TranscriptionMode::Fast } else { TranscriptionMode::Stable };
        c.output.buffer_mode = if s.buffer_clipboard { OutputBufferMode::Clipboard } else { OutputBufferMode::Temporary };
        c.output.auto_inject_focused_window = s.auto_inject;
        c.output.manual_review_before_send = s.manual_review;
        c.output.restore_clipboard_after_inject = s.restore_clipboard;
        c.output.chinese_conversion = match s.conversion {
            0 => ChineseConversionMode::Disabled,
            2 => ChineseConversionMode::SimplifiedChina,
            _ => ChineseConversionMode::TraditionalTaiwan,
        };
        c.output.voice_commands_enabled = s.voice_commands;
        tpl_from_ui(&s.tpl_chat, &mut c.output.rules.chat);
        tpl_from_ui(&s.tpl_writing, &mut c.output.rules.writing);
        tpl_from_ui(&s.tpl_code, &mut c.output.rules.code);
        c.transcription.language = LANGUAGES
            .get(s.language.max(0) as usize)
            .map(|(code, _)| code.to_string())
            .unwrap_or_else(|| "auto".into());
        c.transcription.fast_encoder = s.fast_encoder;
        c.transcription.prefer_traditional_prompt = s.traditional_prompt;
        c.recording.keep_mic_warm = s.keep_mic_warm;
        c.indicator.enabled = s.indicator;
        c.injection.default_method =
            InjectionMethod::all().get(s.default_method.max(0) as usize).copied().unwrap_or_default();
        c.injection.restore_delay_ms = s.restore_delay_ms.clamp(50, 3000) as u64;
        c.injection.refocus_original_window = s.refocus;
        c.ai_cleanup.enabled = s.ai_enabled;
        c.ai_cleanup.endpoint = s.ai_endpoint.to_string();
        c.ai_cleanup.model = s.ai_model.to_string();
        c.ai_cleanup.api_key = s.ai_key.to_string();
        c.ai_cleanup.timeout_ms = s.ai_timeout_ms.clamp(500, 30000) as u64;
        c.ai_cleanup.custom_instructions = s.ai_instructions.to_string();
        c.set_gpu_mode(speaktype::modules::config::GpuMode::from_index(s.gpu_mode));
        c.model_name = Some(s.model_name.to_string());
        c.models_dir = Some(s.models_dir.to_string());
        c.recording.retention_days = s.retention_days.max(0) as u32;
        c.recording.max_total_mb = s.max_total_mb.max(0) as u64;
        c.startup.launch_on_startup = s.launch_on_startup;
        c.startup.start_hidden_to_tray = s.start_hidden;

        if old_warm != self.config.recording.keep_mic_warm {
            self.engine.set_keep_mic_warm(self.config.recording.keep_mic_warm);
        }
        if (old_gain - self.config.recording.gain).abs() > f32::EPSILON {
            self.engine
                .update_audio_config(self.config.recording.input_device_name.clone(), self.config.recording.gain);
        }
        self.save_config();
    }

    pub(super) fn read_rules(&mut self) {
        let methods = InjectionMethod::all();
        let scenarios = Scenario::all();
        self.config.injection.app_rules = self
            .rules_model
            .iter()
            .map(|r| AppRule {
                process: r.process.trim().to_string(),
                method: if r.method <= 0 { None } else { methods.get(r.method as usize - 1).copied() },
                scenario: if r.scenario <= 0 {
                    None
                } else {
                    scenarios.get(r.scenario as usize - 1).map(|s| s.name().to_string())
                },
                tone: r.tone.to_string(),
                newline_as_shift_enter: r.shift_enter,
            })
            .collect();
        self.save_config();
    }

    pub(super) fn read_vocab(&mut self) {
        let entries = self
            .vocab_model
            .iter()
            .map(|r| {
                let mut e = VocabularyEntry::blank();
                e.source = r.source.to_string();
                e.replacement = r.replacement.to_string();
                e.protect = r.protect;
                e
            })
            .collect();
        self.config.output.vocabulary.entries = entries;
        self.save_config();
    }

    pub(super) fn apply_theme(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let t = THEMES[ui_style::theme_index(self.config.ui_theme.as_deref())];
        let col = |c: [u8; 3]| slint::Color::from_rgb_u8(c[0], c[1], c[2]);
        let pal = ui.global::<crate::Pal>();
        pal.set_bg(col(t.bg));
        pal.set_panel(col(t.panel));
        pal.set_text(col(t.text));
        pal.set_accent(col(t.accent));
        pal.set_hover(col(t.hover));
        pal.set_dark(t.dark);
        ui.set_dark_scheme(t.dark);
    }

    // ------------------------------------------------------------------ push state to UI

    pub(super) fn push_all(&mut self) {
        self.push_main();
        self.push_settings();
        self.push_history();
    }

    pub(super) fn push_main(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        ui.set_hidden_to_tray(self.hidden_to_tray);
        ui.set_mic_status(ss(&self.device_status.microphone));
        ui.set_gpu_status(ss(&self.device_status.gpu));
        ui.set_model_status(ss(&self.device_status.model));
        ui.set_scenario(
            Scenario::all().iter().position(|s| *s == self.scenario_manager.current()).unwrap_or(0) as i32,
        );
        ui.set_status(ss(&self.transcription_status));
        ui.set_model_message(ss(&self.model_status_message));
        match &self.model_download {
            Some(p) => {
                let fraction = p
                    .total_bytes
                    .map(|t| p.downloaded_bytes as f32 / t.max(1) as f32)
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                let total = p.total_bytes.map(format_bytes).unwrap_or_else(|| "未知大小".to_string());
                ui.set_downloading(true);
                ui.set_download_progress(fraction);
                ui.set_download_text(ss(format!(
                    "{} / {}　{}/s　{:.0}%",
                    format_bytes(p.downloaded_bytes),
                    total,
                    format_bytes(p.speed_bytes_per_sec as u64),
                    fraction * 100.0
                )));
            }
            None => ui.set_downloading(false),
        }
        ui.set_download_failed(self.model_failed && self.model_cancel.is_none());
        ui.set_recording(self.recording);
        ui.set_record_enabled(self.recording || (!self.transcription_busy && self.model_cancel.is_none()));
        ui.set_record_hotkey(ss(&self.config.hotkeys.record_toggle));
        ui.set_can_retry(!self.transcription_busy && self.pending_retry.is_some());
        ui.set_can_send(self.pending_preview.is_some());
        ui.set_last_error(ss(self.last_error.as_deref().unwrap_or("")));
        ui.set_draft(ss(&self.transcription_draft));
        ui.set_last_result(ss(&self.last_result));
        ui.set_scratch(ss(if self.config.output.buffer_mode == OutputBufferMode::Temporary {
            self.scratch_text.as_str()
        } else {
            ""
        }));
        ui.set_last_recording(ss(self
            .engine
            .last_recording_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default()));
        if ui.get_page() == 5 {
            self.push_errors();
        }
    }

    pub(super) fn push_settings(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        ui.set_s(self.settings_to_ui());
        ui.set_models_dir_note(ss(format!("模型目錄：{}", paths::models_dir().display())));
        self.push_rules();
        self.push_vocab();
        self.push_settings_misc();
    }

    pub(super) fn push_settings_misc(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        ui.set_capturing_hotkey(self.hotkey_capture);
        ui.set_level_testing(self.level_monitor.is_some());
        ui.set_input_level(self.input_level.clamp(0.0, 1.0));
        ui.set_latency_text(ss(self.last_latency.as_ref().map(|t| t.summary()).unwrap_or_default()));
        ui.set_last_app(ss(self.history.records().iter().find_map(|r| r.app.clone()).unwrap_or_default()));
    }

    pub(super) fn push_devices(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let mut names = vec!["系統預設".to_string()];
        names.extend(self.audio_devices.iter().cloned());
        let idx = self
            .config
            .recording
            .input_device_name
            .as_ref()
            .and_then(|n| self.audio_devices.iter().position(|d| d == n))
            .map(|i| i + 1)
            .unwrap_or(0);
        ui.set_devices(strings(names));
        ui.set_device_index(idx as i32);
    }

    pub(super) fn push_rules(&mut self) {
        let methods = InjectionMethod::all();
        let rows: Vec<RuleRow> = self
            .config
            .injection
            .app_rules
            .iter()
            .map(|r| RuleRow {
                process: ss(&r.process),
                method: r
                    .method
                    .and_then(|m| methods.iter().position(|x| *x == m))
                    .map(|i| i as i32 + 1)
                    .unwrap_or(0),
                scenario: r
                    .scenario
                    .as_deref()
                    .and_then(Scenario::from_name)
                    .and_then(|s| Scenario::all().iter().position(|x| *x == s))
                    .map(|i| i as i32 + 1)
                    .unwrap_or(0),
                tone: ss(&r.tone),
                shift_enter: r.newline_as_shift_enter,
            })
            .collect();
        self.rules_model.set_vec(rows);
    }

    pub(super) fn push_vocab(&mut self) {
        if self.config.output.vocabulary.entries.is_empty() {
            self.config.output.vocabulary.entries.push(VocabularyEntry::blank());
        }
        let rows: Vec<VocabRow> = self
            .config
            .output
            .vocabulary
            .entries
            .iter()
            .map(|e| VocabRow { source: ss(&e.source), replacement: ss(&e.replacement), protect: e.protect })
            .collect();
        self.vocab_model.set_vec(rows);
    }

    pub(super) fn push_history(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let needle = self.history_filter.trim().to_lowercase();
        let rows: Vec<HistRow> = self
            .history
            .records()
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                needle.is_empty()
                    || r.text.to_lowercase().contains(&needle)
                    || r.app.as_deref().unwrap_or("").to_lowercase().contains(&needle)
            })
            .map(|(i, r)| {
                let mut meta = format!("{:.1} 秒", r.duration_sec);
                if let Some(ms) = r.latency_ms {
                    meta.push_str(&format!(" · 延遲 {ms:.0} ms"));
                }
                HistRow {
                    idx: i as i32,
                    time: ss(r.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
                    scenario: ss(&r.scenario),
                    app: ss(r.app.as_deref().unwrap_or("")),
                    meta: ss(meta),
                    text: ss(&r.text),
                }
            })
            .collect();
        self.history_rows.set_vec(rows);
        ui.set_history_total(self.history.records().len() as i32);
        ui.set_history_path(ss(HistoryManager::history_path().map(|p| p.display().to_string()).unwrap_or_default()));
    }

    pub(super) fn push_models(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let base = PathBuf::from(self.config.get_models_dir());
        let current = self.config.get_model_name();
        let rows: Vec<ModelRow> = MODEL_CATALOG
            .iter()
            .map(|e| {
                let path = base.join(e.file_name);
                let installed = path.exists();
                let file_size = if installed {
                    std::fs::metadata(&path).map(|m| format_bytes(m.len())).unwrap_or_default()
                } else {
                    String::new()
                };
                ModelRow {
                    name: ss(e.name),
                    label: ss(e.label),
                    size: ss(e.approx_size),
                    recommendation: ss(e.recommendation),
                    url: ss(models::model_url(e.file_name)),
                    installed,
                    current: current == e.name,
                    file_size: ss(file_size),
                    sha_warning: ss(self.model_sha256_errors.get(e.name).cloned().unwrap_or_default()),
                }
            })
            .collect();
        ui.set_models(ModelRc::new(VecModel::from(rows)));
        ui.set_models_dir_note(ss(format!("模型資料夾：{}", self.config.get_models_dir())));
        match self.pending_download_confirm.as_deref() {
            Some(name) => {
                let entry = MODEL_CATALOG.iter().find(|e| e.name == name);
                ui.set_confirm_label(ss(entry.map(|e| e.label).unwrap_or(name)));
                ui.set_confirm_file(ss(entry.map(|e| e.file_name).unwrap_or("")));
                ui.set_confirm_size(ss(entry.map(|e| e.approx_size).unwrap_or("")));
                ui.set_confirm_dir(ss(self.config.get_models_dir()));
            }
            None => ui.set_confirm_label(ss("")),
        }
    }

    pub(super) fn push_recordings(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let files = recordings::list_recordings(&self.recording_filter);
        let total: u64 = files.iter().map(|f| f.size_bytes).sum();
        ui.set_recordings_summary(ss(if files.is_empty() {
            "沒有符合條件的錄音檔".to_string()
        } else {
            format!("共 {} 筆，{}", files.len(), format_bytes(total))
        }));
        let rows: Vec<RecRow> = files
            .iter()
            .map(|f| RecRow {
                time: ss(f.modified.format("%Y-%m-%d %H:%M:%S").to_string()),
                name: ss(&f.file_name),
                size: ss(format_bytes(f.size_bytes)),
                path: ss(f.path.display().to_string()),
            })
            .collect();
        ui.set_recordings(ModelRc::new(VecModel::from(rows)));
    }

    pub(super) fn push_errors(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        ui.set_errors(strings(self.error_log.iter()));
    }
}
