//! Fast, reliable text insertion into the focused application.
//!
//! * **Paste** (default): clipboard + Ctrl+V sent with `SendInput`. The previous
//!   clipboard (text *and* image) is restored on a background thread after a
//!   delay, and only if nobody changed the clipboard in the meantime
//!   (`GetClipboardSequenceNumber`), so the pipeline returns immediately and
//!   slow apps (Office, LINE) never paste stale content.
//! * **Unicode**: one batched `SendInput` call with `KEYEVENTF_UNICODE` events —
//!   bypasses the IME, handles CJK / emoji (surrogate pairs) and never touches the
//!   clipboard. Best for consoles and remote desktop.
//! * Before sending anything we wait for the hotkey modifiers to be physically
//!   released (otherwise Ctrl+Shift+L → Ctrl+**Shift**+V), and re-focus the
//!   window that was active when the hotkey was released.
use crate::modules::config::{InjectionConfig, InjectionMethod};
use crate::modules::error::log_error;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForegroundApp {
    /// Opaque window handle (HWND on Windows).
    pub window: isize,
    /// Executable file name, e.g. `chrome.exe`.
    pub process: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InjectPlan {
    pub method: InjectionMethod,
    pub newline_as_shift_enter: bool,
    pub paste_delay_ms: u64,
    pub restore_clipboard: bool,
    pub restore_delay_ms: u64,
    pub refocus: bool,
}

impl InjectPlan {
    pub fn resolve(config: &InjectionConfig, app: Option<&ForegroundApp>, restore_clipboard: bool) -> Self {
        let rule = config.rule_for(app.and_then(|a| a.process.as_deref()));
        Self {
            method: rule.and_then(|r| r.method).unwrap_or(config.default_method),
            newline_as_shift_enter: rule.map(|r| r.newline_as_shift_enter).unwrap_or(false),
            paste_delay_ms: config.paste_delay_ms,
            restore_clipboard,
            restore_delay_ms: config.restore_delay_ms,
            refocus: config.refocus_original_window,
        }
    }
}

/// Normalise line endings for the target (CRLF for Windows clipboard).
pub fn prepare_text(text: &str) -> String {
    let unix = text.replace("\r\n", "\n");
    if cfg!(windows) {
        unix.replace('\n', "\r\n")
    } else {
        unix
    }
}

pub fn inject(text: &str, plan: &InjectPlan, target: Option<&ForegroundApp>) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    wait_for_modifiers_released(Duration::from_millis(600));
    if plan.refocus {
        if let Some(target) = target {
            refocus(target);
        }
    }
    match plan.method {
        InjectionMethod::Paste | InjectionMethod::PasteShift => {
            paste(text, plan, plan.method == InjectionMethod::PasteShift)
        }
        InjectionMethod::Unicode => platform::send_unicode(text, plan.newline_as_shift_enter),
        InjectionMethod::ClipboardOnly => {
            let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
            clipboard.set_text(prepare_text(text)).map_err(|e| e.to_string())
        }
    }
}

enum SavedClipboard {
    Text(String),
    Image(arboard::ImageData<'static>),
    Empty,
}

fn paste(text: &str, plan: &InjectPlan, shift: bool) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("無法開啟剪貼簿: {e}"))?;
    let saved = if plan.restore_clipboard {
        match clipboard.get_text() {
            Ok(t) => SavedClipboard::Text(t),
            Err(_) => clipboard
                .get_image()
                .map(|img| SavedClipboard::Image(img.to_owned_img()))
                .unwrap_or(SavedClipboard::Empty),
        }
    } else {
        SavedClipboard::Empty
    };
    clipboard
        .set_text(prepare_text(text))
        .map_err(|e| format!("寫入剪貼簿失敗: {e}"))?;
    let sequence_after_set = platform::clipboard_sequence();
    if plan.paste_delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(plan.paste_delay_ms));
    }
    platform::send_paste_chord(shift)?;

    if plan.restore_clipboard && !matches!(saved, SavedClipboard::Empty) {
        let delay = Duration::from_millis(plan.restore_delay_ms);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            // Someone (the user or another app) changed the clipboard: keep theirs.
            if platform::clipboard_sequence() != sequence_after_set {
                return;
            }
            let Ok(mut clipboard) = arboard::Clipboard::new() else { return };
            let result = match saved {
                SavedClipboard::Text(t) => clipboard.set_text(t),
                SavedClipboard::Image(img) => clipboard.set_image(img),
                SavedClipboard::Empty => Ok(()),
            };
            if let Err(err) = result {
                log_error("clipboard restore", err);
            }
        });
    }
    Ok(())
}

pub fn foreground_app() -> Option<ForegroundApp> {
    platform::foreground_app()
}

fn refocus(target: &ForegroundApp) {
    if target.window == 0 {
        return;
    }
    if platform::foreground_app().map(|f| f.window) != Some(target.window) {
        platform::set_foreground(target.window);
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn wait_for_modifiers_released(timeout: Duration) {
    let start = Instant::now();
    while platform::modifiers_down() && start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Build the UTF-16 unit sequence sent by the Unicode method. `None` = Enter.
pub fn unicode_units(text: &str) -> Vec<Option<u16>> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.replace("\r\n", "\n").chars() {
        if ch == '\n' {
            out.push(None);
        } else if ch == '\r' {
            continue;
        } else {
            let mut buf = [0u16; 2];
            out.extend(ch.encode_utf16(&mut buf).iter().map(|u| Some(*u)));
        }
    }
    out
}

#[cfg(windows)]
mod platform {
    use super::ForegroundApp;
    use windows_sys::Win32::Foundation::{CloseHandle, HWND};
    use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };

    fn key(vk: VIRTUAL_KEY, scan: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 },
            },
        }
    }

    fn send(inputs: &[INPUT]) -> Result<(), String> {
        if inputs.is_empty() {
            return Ok(());
        }
        // SAFETY: `inputs` is a valid slice of initialised INPUT structs.
        let sent = unsafe {
            SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32)
        };
        if sent as usize == inputs.len() {
            Ok(())
        } else {
            Err("SendInput 被阻擋（目標程式可能以系統管理員權限執行）".to_string())
        }
    }

    pub fn send_paste_chord(shift: bool) -> Result<(), String> {
        let v = b'V' as VIRTUAL_KEY;
        let mut inputs = vec![key(VK_CONTROL, 0, 0)];
        if shift {
            inputs.push(key(VK_SHIFT, 0, 0));
        }
        inputs.push(key(v, 0, 0));
        inputs.push(key(v, 0, KEYEVENTF_KEYUP));
        if shift {
            inputs.push(key(VK_SHIFT, 0, KEYEVENTF_KEYUP));
        }
        inputs.push(key(VK_CONTROL, 0, KEYEVENTF_KEYUP));
        send(&inputs)
    }

    pub fn send_unicode(text: &str, newline_shift_enter: bool) -> Result<(), String> {
        let mut inputs = Vec::new();
        for unit in super::unicode_units(text) {
            match unit {
                Some(u) => {
                    inputs.push(key(0, u, KEYEVENTF_UNICODE));
                    inputs.push(key(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                }
                None => {
                    if newline_shift_enter {
                        inputs.push(key(VK_SHIFT, 0, 0));
                    }
                    inputs.push(key(VK_RETURN, 0, 0));
                    inputs.push(key(VK_RETURN, 0, KEYEVENTF_KEYUP));
                    if newline_shift_enter {
                        inputs.push(key(VK_SHIFT, 0, KEYEVENTF_KEYUP));
                    }
                }
            }
        }
        // Chunk to stay well below input-queue limits on slow targets.
        for chunk in inputs.chunks(512) {
            send(chunk)?;
        }
        Ok(())
    }

    pub fn clipboard_sequence() -> u32 {
        // SAFETY: no arguments, always safe to call.
        unsafe { GetClipboardSequenceNumber() }
    }

    pub fn modifiers_down() -> bool {
        [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
            .iter()
            // SAFETY: GetAsyncKeyState accepts any virtual-key code.
            .any(|vk| unsafe { GetAsyncKeyState(*vk as i32) } as u16 & 0x8000 != 0)
    }

    pub fn foreground_app() -> Option<ForegroundApp> {
        // SAFETY: plain Win32 queries; handles are checked and closed.
        unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let mut process = None;
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if !handle.is_null() {
                let mut buf = [0u16; 1024];
                let mut len = buf.len() as u32;
                if QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0 {
                    let path = String::from_utf16_lossy(&buf[..len as usize]);
                    process = path.rsplit('\\').next().map(str::to_string);
                }
                CloseHandle(handle);
            }
            Some(ForegroundApp { window: hwnd as isize, process })
        }
    }

    pub fn set_foreground(window: isize) {
        // SAFETY: SetForegroundWindow tolerates stale handles.
        unsafe {
            SetForegroundWindow(window as HWND);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::ForegroundApp;
    use enigo::{Enigo, Key, KeyboardControllable};

    pub fn send_paste_chord(shift: bool) -> Result<(), String> {
        let mut enigo = Enigo::new();
        enigo.key_down(Key::Control);
        if shift {
            enigo.key_down(Key::Shift);
        }
        enigo.key_click(Key::Layout('v'));
        if shift {
            enigo.key_up(Key::Shift);
        }
        enigo.key_up(Key::Control);
        Ok(())
    }

    pub fn send_unicode(text: &str, newline_shift_enter: bool) -> Result<(), String> {
        let mut enigo = Enigo::new();
        for line in text.replace("\r\n", "\n").split_inclusive('\n') {
            let body = line.trim_end_matches('\n');
            enigo.key_sequence(body);
            if line.ends_with('\n') {
                if newline_shift_enter {
                    enigo.key_down(Key::Shift);
                }
                enigo.key_click(Key::Return);
                if newline_shift_enter {
                    enigo.key_up(Key::Shift);
                }
            }
        }
        Ok(())
    }

    pub fn clipboard_sequence() -> u32 {
        0
    }

    pub fn modifiers_down() -> bool {
        false
    }

    pub fn foreground_app() -> Option<ForegroundApp> {
        None
    }

    pub fn set_foreground(_window: isize) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::config::InjectionConfig;

    #[test]
    fn unicode_units_handle_cjk_emoji_newlines() {
        let units = unicode_units("中a\r\n😀");
        assert_eq!(units[0], Some('中' as u16));
        assert_eq!(units[1], Some('a' as u16));
        assert_eq!(units[2], None);
        assert_eq!(units.len(), 5, "emoji is a surrogate pair");
    }

    #[test]
    fn per_app_rules_pick_method() {
        let cfg = InjectionConfig::default();
        let app = |p: &str| ForegroundApp { window: 1, process: Some(p.to_string()) };
        assert_eq!(InjectPlan::resolve(&cfg, Some(&app("CMD.EXE")), true).method, InjectionMethod::Unicode);
        let line = InjectPlan::resolve(&cfg, Some(&app("LINE.exe")), true);
        assert_eq!(line.method, InjectionMethod::Paste);
        assert!(line.newline_as_shift_enter);
        assert_eq!(InjectPlan::resolve(&cfg, Some(&app("chrome.exe")), true).method, InjectionMethod::Paste);
        assert_eq!(InjectPlan::resolve(&cfg, None, true).method, InjectionMethod::Paste);
    }

    #[test]
    fn prepare_text_uses_platform_newlines() {
        let out = prepare_text("a\nb");
        if cfg!(windows) { assert_eq!(out, "a\r\nb") } else { assert_eq!(out, "a\nb") }
    }
}
