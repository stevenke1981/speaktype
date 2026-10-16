//! SpeakType controller: owns all state and logic, pushes it into the Slint UI
//! and reacts to UI callbacks. Event-driven: hotkey, model worker and tray
//! threads wake the UI thread through `slint::invoke_from_event_loop`; timers
//! only run while something is actually changing (recording, level test,
//! GPU probe), so an idle window does no work at all.

use crate::indicator::Indicator;
use crate::{HistRow, MainWindow, ModelRow, RecRow, RuleRow, RuleTpl, Settings, VocabRow};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use speaktype::modules::audio::LevelMonitor;
use speaktype::modules::config::{
    default_app_rules, AppConfig, AppRule, ChineseConversionMode, InjectionMethod, OutputBufferMode,
    ScenarioOutputRules, TranscriptionMode, VocabularyEntry,
};
use speaktype::modules::diagnostics;
use speaktype::modules::engine::{
    load_recording_wav, ModelDownloadProgress, ModelEvent, ModelWorker, PipelineTimings, SpeakTypeEngine,
    TranscriptionEvent, TranscriptionRequest, TranscriptionResult, WorkerEvent,
};
use speaktype::modules::error::{log_error, log_file_path};
use speaktype::modules::history::{HistoryManager, RecognitionRecord};
use speaktype::modules::input::{GlobalHotkey, HotkeyCombo, HotkeyEvent};
use speaktype::modules::models::{self, cleanup_stale_temp_files, MODEL_CATALOG};
use speaktype::modules::paths;
use speaktype::modules::recordings;
use speaktype::modules::scenario::{Scenario, ScenarioManager};
use speaktype::modules::startup;
use speaktype::modules::tray::{create_tray, TrayAction, TrayManager};
use speaktype::modules::ui_style::{self, format_bytes, THEMES};
use speaktype::modules::utils::device::DeviceStatus;
use speaktype::modules::voice_commands;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const LANGUAGES: [(&str, &str); 5] =
    [("auto", "自動（中英夾雜）"), ("zh", "中文"), ("en", "English"), ("ja", "日本語"), ("ko", "한국어")];

mod logic;
mod sync;

thread_local! {
    static APP: RefCell<Option<Rc<RefCell<App>>>> = const { RefCell::new(None) };
}

/// Wake the UI thread from any thread and let it drain pending events.
pub fn wake() {
    let _ = slint::invoke_from_event_loop(|| {
        if let Some(app) = APP.with(|a| a.borrow().clone()) {
            if let Ok(mut app) = app.try_borrow_mut() {
                app.pump();
            } else {
                // Re-entrant wake while a callback holds the borrow: retry shortly.
                Timer::single_shot(Duration::from_millis(5), wake);
            }
        }
    });
}

pub struct Options {
    pub start_hidden_to_tray: bool,
    pub startup_started: Instant,
    pub startup_probe: Option<PathBuf>,
}

pub struct App {
    ui: slint::Weak<MainWindow>,
    indicator: Option<Indicator>,
    anim_timer: Timer,
    level_timer: Timer,
    gpu_timer: Timer,
    rules_model: Rc<VecModel<RuleRow>>,
    vocab_model: Rc<VecModel<VocabRow>>,
    history_rows: Rc<VecModel<HistRow>>,
    history_filter: String,
    recording_filter: String,
    audio_devices: Vec<String>,
    audio_devices_loaded: bool,
    last_latency: Option<PipelineTimings>,
    recording: bool,
    recording_start: Option<Instant>,
    last_result: String,
    scratch_text: String,
    last_error: Option<String>,
    error_log: Vec<String>,
    transcription_status: String,
    transcription_busy: bool,
    transcription_draft: String,
    pending_retry: Option<TranscriptionRequest>,
    pending_preview: Option<String>,
    model_status_message: String,
    model_download: Option<ModelDownloadProgress>,
    model_failed: bool,
    model_cancel: Option<Arc<AtomicBool>>,
    model_worker: ModelWorker,
    model_sha256_errors: HashMap<String, String>,
    model_download_name: Option<String>,
    pending_download_confirm: Option<String>,
    hotkey_capture: bool,
    level_monitor: Option<LevelMonitor>,
    input_level: f32,
    tray: Option<TrayManager>,
    hidden_to_tray: bool,
    exit_requested: bool,
    scenario_manager: ScenarioManager,
    history: HistoryManager,
    config: AppConfig,
    engine: SpeakTypeEngine,
    hotkey: GlobalHotkey,
    device_status: DeviceStatus,
    started: Instant,
    first_frame_seen: bool,
    startup_probe: Option<PathBuf>,
}

pub fn run(opts: Options) -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    let font = ui_style::ui_font_family();
    ui.set_ui_font(font.into());
    let indicator = match Indicator::new(font) {
        Ok(ind) => Some(ind),
        Err(err) => {
            log_error("indicator", err.to_string());
            None
        }
    };

    let app = Rc::new(RefCell::new(App::new(&ui, indicator, &opts)));
    APP.with(|a| *a.borrow_mut() = Some(app.clone()));
    wire_callbacks(&ui, &app);
    first_frame_hook(&ui, &app);

    {
        let mut a = app.borrow_mut();
        a.apply_theme();
        a.push_all();
        a.apply_screenshot_hooks();
    }

    let start_hidden = app.borrow().hidden_to_tray;
    if !start_hidden {
        ui.show()?;
    }
    let result = slint::run_event_loop_until_quit();
    if let Ok(mut a) = app.try_borrow_mut() {
        a.shutdown();
    }
    APP.with(|a| *a.borrow_mut() = None);
    result
}

/// Startup probe / time-to-first-frame: the first redraw of the main window.
fn first_frame_hook(ui: &MainWindow, app: &Rc<RefCell<App>>) {
    use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
    let weak_app = Rc::downgrade(app);
    ui.window().on_winit_window_event(move |_w, event| {
        match event {
            winit::event::WindowEvent::RedrawRequested => {
                if let Some(app) = weak_app.upgrade() {
                    let pending = app.try_borrow().map(|a| !a.first_frame_seen).unwrap_or(false);
                    if pending {
                        // Run after this frame has been presented.
                        Timer::single_shot(Duration::ZERO, move || {
                            if let Ok(mut a) = app.try_borrow_mut() {
                                a.on_first_frame();
                            }
                        });
                    }
                }
            }
            winit::event::WindowEvent::Resized(_) => {
                // Minimising the window sends it to the tray (as in v0.4).
                if let Some(app) = weak_app.upgrade() {
                    let minimized = _w.is_minimized();
                    if minimized {
                        Timer::single_shot(Duration::ZERO, move || {
                            if let Ok(mut a) = app.try_borrow_mut() {
                                if a.tray.is_some() && !a.hidden_to_tray {
                                    a.minimize_to_tray();
                                }
                            }
                        });
                    }
                }
            }
            _ => {}
        }
        EventResult::Propagate
    });
}

macro_rules! cb {
    ($app:ident, |$a:ident $(, $arg:ident)*| $body:expr) => {{
        let weak = Rc::downgrade($app);
        move |$($arg),*| {
            if let Some(rc) = weak.upgrade() {
                let mut guard = rc.borrow_mut();
                let $a: &mut App = &mut guard;
                $body
            }
        }
    }};
}

fn wire_callbacks(ui: &MainWindow, app: &Rc<RefCell<App>>) {
    ui.on_action(cb!(app, |a, name| a.on_action(name.as_str())));
    ui.on_select_scenario(cb!(app, |a, i| {
        let sc = Scenario::all().get(i as usize).copied().unwrap_or(Scenario::Chat);
        a.select_scenario(sc);
        a.push_main();
    }));
    ui.on_settings_changed(cb!(app, |a| a.read_settings()));
    ui.on_rules_changed(cb!(app, |a| a.read_rules()));
    ui.on_rule_delete(cb!(app, |a, i| {
        let i = i as usize;
        if i < a.config.injection.app_rules.len() {
            a.config.injection.app_rules.remove(i);
            a.save_config();
            a.push_rules();
        }
    }));
    ui.on_vocab_changed(cb!(app, |a| a.read_vocab()));
    ui.on_vocab_delete(cb!(app, |a, i| {
        let entries = &mut a.config.output.vocabulary.entries;
        if (i as usize) < entries.len() {
            entries.remove(i as usize);
            if entries.is_empty() {
                entries.push(VocabularyEntry::blank());
            }
            a.save_config();
            a.push_vocab();
        }
    }));
    ui.on_device_selected(cb!(app, |a, i| {
        a.config.recording.input_device_name =
            if i <= 0 { None } else { a.audio_devices.get(i as usize - 1).cloned() };
        a.engine.update_audio_config(a.config.recording.input_device_name.clone(), a.config.recording.gain);
        a.save_config();
        a.refresh_device_status();
        a.push_main();
    }));
    ui.on_theme_selected(cb!(app, |a, i| {
        a.config.ui_theme = THEMES.get(i as usize).map(|t| t.id.to_string());
        a.save_config();
        a.apply_theme();
    }));
    ui.on_hotkey_entered(cb!(app, |a, text| a.set_hotkey(text.as_str())));
    ui.on_hotkey_captured(cb!(app, |a, text, ctrl, alt, shift, meta| {
        if let Some(combo) = hotkey_from_key(text.as_str(), ctrl, alt, shift, meta) {
            a.set_hotkey(&combo);
        }
        a.hotkey_capture = false;
        a.push_settings_misc();
    }));
    ui.on_history_search(cb!(app, |a, text| {
        a.history_filter = text.to_string();
        a.push_history();
    }));
    ui.on_history_copy(cb!(app, |a, idx| {
        if let Some(r) = a.history.records().get(idx as usize) {
            let text = r.text.clone();
            a.copy_text(&text);
        }
    }));
    ui.on_model_use(cb!(app, |a, name| {
        a.select_model(name.as_str(), false);
        a.push_all();
    }));
    ui.on_model_download(cb!(app, |a, name| {
        a.pending_download_confirm = Some(name.to_string());
        a.push_models();
    }));
    ui.on_recordings_filter(cb!(app, |a, text| {
        a.recording_filter = text.to_string();
        a.push_recordings();
    }));
    ui.on_recording_play(cb!(app, |a, path| {
        if let Err(err) = recordings::play_recording(std::path::Path::new(path.as_str())) {
            a.record_error(err);
            a.push_main();
        }
    }));
    ui.on_recording_retranscribe(cb!(app, |a, path| {
        a.retranscribe_recording(PathBuf::from(path.as_str()));
        if let Some(ui) = a.ui.upgrade() {
            ui.set_page(0);
        }
        a.push_main();
    }));
    ui.on_recording_delete(cb!(app, |a, path| {
        if let Err(err) = recordings::delete_recording(std::path::Path::new(path.as_str())) {
            a.record_error(err);
        }
        a.push_recordings();
        a.push_main();
    }));

    // Close button: hide to tray (or quit when there is no tray).
    let weak = Rc::downgrade(app);
    ui.window().on_close_requested(move || {
        if let Some(rc) = weak.upgrade() {
            if let Ok(mut a) = rc.try_borrow_mut() {
                if a.tray.is_some() && !a.exit_requested {
                    a.hidden_to_tray = true;
                    return slint::CloseRequestResponse::HideWindow;
                }
            }
        }
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });
}

/// Build a hotkey string ("Ctrl+Shift+L") from a Slint key event.
pub fn hotkey_from_key(text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Option<String> {
    let c = text.chars().next()?;
    let key = match c as u32 {
        0xF704..=0xF70F => format!("F{}", c as u32 - 0xF704 + 1),
        0x20 => "SPACE".to_string(),
        0x0A | 0x0D => "ENTER".to_string(),
        0x09 => "TAB".to_string(),
        0x01..=0x1A => ((b'A' + (c as u8) - 1) as char).to_string(), // Ctrl+letter control codes
        _ if c.is_ascii_alphanumeric() => c.to_ascii_uppercase().to_string(),
        _ => return None,
    };
    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if alt {
        parts.push("Alt");
    }
    if shift {
        parts.push("Shift");
    }
    if meta {
        parts.push("Win");
    }
    let mut s = parts.join("+");
    if !s.is_empty() {
        s.push('+');
    }
    s.push_str(&key);
    Some(s)
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

fn strings(items: impl IntoIterator<Item = impl AsRef<str>>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(items.into_iter().map(ss).collect::<Vec<_>>()))
}

fn tpl_to_ui(r: &ScenarioOutputRules) -> RuleTpl {
    RuleTpl {
        auto_punctuation: r.auto_punctuation,
        format_paragraphs: r.format_paragraphs,
        remove_fillers: r.remove_fillers,
        preserve_code_symbols: r.preserve_code_symbols,
        auto_line_breaks: r.auto_line_breaks,
        format_lists: r.format_lists,
        resolve_self_corrections: r.resolve_self_corrections,
        cjk_spacing: r.cjk_spacing,
    }
}

fn tpl_from_ui(t: &RuleTpl, r: &mut ScenarioOutputRules) {
    r.auto_punctuation = t.auto_punctuation;
    r.format_paragraphs = t.format_paragraphs;
    r.remove_fillers = t.remove_fillers;
    r.preserve_code_symbols = t.preserve_code_symbols;
    r.auto_line_breaks = t.auto_line_breaks;
    r.format_lists = t.format_lists;
    r.resolve_self_corrections = t.resolve_self_corrections;
    r.cjk_spacing = t.cjk_spacing;
}

fn method_index(m: InjectionMethod) -> i32 {
    InjectionMethod::all().iter().position(|x| *x == m).unwrap_or(0) as i32
}

fn blank_rule(process: String) -> AppRule {
    AppRule { process, method: None, scenario: None, tone: String::new(), newline_as_shift_enter: false }
}

/// Run `f` with the app if it is not currently borrowed (timer callbacks).
fn with_app(f: impl FnOnce(&mut App)) {
    if let Some(app) = APP.with(|a| a.borrow().clone()) {
        if let Ok(mut a) = app.try_borrow_mut() {
            f(&mut a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::hotkey_from_key;

    #[test]
    fn hotkey_capture_maps_keys() {
        assert_eq!(hotkey_from_key("l", true, false, true, false).as_deref(), Some("Ctrl+Shift+L"));
        assert_eq!(hotkey_from_key("\u{c}", true, false, false, false).as_deref(), Some("Ctrl+L"));
        assert_eq!(hotkey_from_key("\u{F708}", false, true, false, false).as_deref(), Some("Alt+F5"));
        assert_eq!(hotkey_from_key(" ", true, false, false, false).as_deref(), Some("Ctrl+SPACE"));
        assert_eq!(hotkey_from_key("\u{F700}", true, false, false, false), None);
    }
}
