// input/mod.rs - 鍵盤輸入與熱鍵模組
// 職責：全域熱鍵偵測、PTT 控制、文字自動貼上

use crate::modules::error::log_error;
use device_query::{DeviceQuery, DeviceState, Keycode};
use enigo::{Enigo, Key, KeyboardControllable};
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// 輸入控制器（包裝 enigo）
pub struct InputController {
    enigo: Enigo,
}

impl InputController {
    pub fn new() -> Self {
        Self {
            enigo: Enigo::new(),
        }
    }

    pub fn enigo(&mut self) -> &mut Enigo {
        &mut self.enigo
    }

    /// 將文字貼上到目前焦點視窗
    pub fn paste_text(&mut self, text: &str) -> Result<(), String> {
        if text.is_empty() {
            return Ok(());
        }
        self.enigo.key_sequence(text);
        Ok(())
    }

    pub fn copy_to_clipboard(&mut self, text: &str) -> Result<(), String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard
            .set_text(text.to_string())
            .map_err(|e| e.to_string())
    }

    pub fn inject_via_clipboard(
        &mut self,
        text: &str,
        restore_previous_text: bool,
    ) -> Result<(), String> {
        if text.is_empty() {
            return Ok(());
        }

        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let previous_text = if restore_previous_text {
            clipboard.get_text().ok()
        } else {
            None
        };

        clipboard
            .set_text(text.to_string())
            .map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(40));
        self.press_ctrl_v();
        thread::sleep(Duration::from_millis(80));

        if let (true, Some(previous_text)) = (restore_previous_text, previous_text) {
            clipboard
                .set_text(previous_text)
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    pub fn press_key(&mut self, key: enigo::Key) {
        self.enigo.key_click(key);
    }

    fn press_ctrl_v(&mut self) {
        self.enigo.key_down(Key::Control);
        self.enigo.key_click(Key::Layout('v'));
        self.enigo.key_up(Key::Control);
    }
}

impl Default for InputController {
    fn default() -> Self {
        Self::new()
    }
}

/// 全域熱鍵管理器（使用 rdev 實作真正即時監聽）
pub struct GlobalHotkey {
    device_state: DeviceState,
    last_hotkey_pressed: bool,
    hotkey_pressed: Arc<AtomicBool>,
    events: Arc<Mutex<VecDeque<HotkeyEvent>>>,
    combo: Arc<Mutex<HotkeyCombo>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

impl GlobalHotkey {
    pub fn new(record_toggle: &str) -> Self {
        let hotkey_pressed = Arc::new(AtomicBool::new(false));
        let hotkey_clone = hotkey_pressed.clone();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let events_clone = events.clone();
        let combo = Arc::new(Mutex::new(
            HotkeyCombo::parse(record_toggle).unwrap_or_default(),
        ));
        let combo_clone = combo.clone();

        // 啟動 rdev 全域監聽器
        thread::spawn(move || {
            use rdev::{listen, Event, EventType, Key};
            let mut ctrl_down = false;
            let mut shift_down = false;
            let mut alt_down = false;
            let mut win_down = false;
            let mut pressed_keys = HashSet::<String>::new();
            let mut combo_down = false;

            if let Err(error) = listen(move |event: Event| {
                match event.event_type {
                    EventType::KeyPress(Key::ControlLeft | Key::ControlRight) => {
                        ctrl_down = true;
                    }
                    EventType::KeyPress(Key::ShiftLeft | Key::ShiftRight) => {
                        shift_down = true;
                    }
                    EventType::KeyPress(Key::Alt | Key::AltGr) => {
                        alt_down = true;
                    }
                    EventType::KeyPress(Key::MetaLeft | Key::MetaRight) => {
                        win_down = true;
                    }
                    EventType::KeyPress(key) => {
                        if let Some(key_name) = normalize_rdev_key(key) {
                            pressed_keys.insert(key_name);
                        }
                    }
                    EventType::KeyRelease(Key::ControlLeft | Key::ControlRight) => {
                        ctrl_down = false;
                    }
                    EventType::KeyRelease(Key::ShiftLeft | Key::ShiftRight) => {
                        shift_down = false;
                    }
                    EventType::KeyRelease(Key::Alt | Key::AltGr) => {
                        alt_down = false;
                    }
                    EventType::KeyRelease(Key::MetaLeft | Key::MetaRight) => {
                        win_down = false;
                    }
                    EventType::KeyRelease(key) => {
                        if let Some(key_name) = normalize_rdev_key(key) {
                            pressed_keys.remove(&key_name);
                        }
                    }
                    _ => {}
                }

                let next_combo_down = combo_clone
                    .lock()
                    .map(|combo| {
                        combo.matches(ctrl_down, shift_down, alt_down, win_down, &pressed_keys)
                    })
                    .unwrap_or(false);
                if next_combo_down != combo_down {
                    combo_down = next_combo_down;
                    match events_clone.lock() {
                        Ok(mut events) => {
                            events.push_back(if combo_down {
                                HotkeyEvent::Pressed
                            } else {
                                HotkeyEvent::Released
                            });
                        }
                        Err(err) => {
                            log_error("hotkey event queue", err);
                        }
                    }
                }
                hotkey_clone.store(next_combo_down, Ordering::Relaxed);
            }) {
                log_error("hotkey listener", format!("{error:?}"));
            }
        });

        Self {
            device_state: DeviceState::new(),
            last_hotkey_pressed: false,
            hotkey_pressed,
            events,
            combo,
        }
    }

    pub fn update_hotkey(&mut self, record_toggle: &str) -> Result<(), String> {
        let combo = HotkeyCombo::parse(record_toggle)?;
        match self.combo.lock() {
            Ok(mut current) => {
                *current = combo;
                Ok(())
            }
            Err(err) => {
                log_error("hotkey update", err);
                Err("無法更新快捷鍵設定".to_string())
            }
        }
    }

    pub fn poll_record_hotkey_event(&mut self) -> Option<HotkeyEvent> {
        match self.events.lock() {
            Ok(mut events) => {
                if let Some(event) = events.pop_front() {
                    self.last_hotkey_pressed = matches!(event, HotkeyEvent::Pressed);
                    return Some(event);
                }
            }
            Err(err) => {
                log_error("hotkey event poll", err);
            }
        }

        let pressed = self.is_record_hotkey_down();

        let event = if pressed && !self.last_hotkey_pressed {
            Some(HotkeyEvent::Pressed)
        } else if !pressed && self.last_hotkey_pressed {
            Some(HotkeyEvent::Released)
        } else {
            None
        };

        self.last_hotkey_pressed = pressed;
        event
    }

    /// 檢查 Ctrl+Shift+L 是否被按下（邊緣觸發，優先使用 rdev）
    pub fn is_record_hotkey_pressed(&mut self) -> bool {
        matches!(self.poll_record_hotkey_event(), Some(HotkeyEvent::Pressed))
    }

    fn is_record_hotkey_down(&self) -> bool {
        let rdev_pressed = self.hotkey_pressed.load(Ordering::Relaxed);
        if rdev_pressed {
            return true;
        }

        let keys: Vec<Keycode> = self.device_state.get_keys();
        let ctrl_down = keys.contains(&Keycode::LControl) || keys.contains(&Keycode::RControl);
        let shift_down = keys.contains(&Keycode::LShift) || keys.contains(&Keycode::RShift);
        let alt_down = keys.contains(&Keycode::LAlt) || keys.contains(&Keycode::RAlt);
        let win_down = keys.contains(&Keycode::LMeta) || keys.contains(&Keycode::RMeta);
        let pressed_keys = keys
            .iter()
            .filter_map(|key| normalize_device_key(*key))
            .collect::<HashSet<_>>();

        self.combo
            .lock()
            .map(|combo| combo.matches(ctrl_down, shift_down, alt_down, win_down, &pressed_keys))
            .unwrap_or(false)
    }
}

impl Default for GlobalHotkey {
    fn default() -> Self {
        Self::new("Ctrl+Shift+L")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyCombo {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub key: String,
}

impl HotkeyCombo {
    pub fn parse(value: &str) -> Result<Self, String> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut win = false;
        let mut key = None;

        for part in value
            .split('+')
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
        {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "shift" => shift = true,
                "alt" => alt = true,
                "win" | "windows" | "meta" => win = true,
                other => key = Some(normalize_config_key(other)),
            }
        }

        let key = key.ok_or_else(|| "快捷鍵必須包含一個主鍵".to_string())?;
        if !(ctrl || shift || alt || win) {
            return Err("快捷鍵至少需要 Ctrl/Alt/Shift/Win 其中一個修飾鍵".to_string());
        }
        if ctrl && shift && !alt && !win && key == "L" {
            return Ok(Self {
                ctrl,
                shift,
                alt,
                win,
                key,
            });
        }
        if ctrl && key == "V" {
            return Err("Ctrl+V 與文字注入衝突，請選其他快捷鍵".to_string());
        }

        Ok(Self {
            ctrl,
            shift,
            alt,
            win,
            key,
        })
    }

    fn matches(
        &self,
        ctrl_down: bool,
        shift_down: bool,
        alt_down: bool,
        win_down: bool,
        pressed_keys: &HashSet<String>,
    ) -> bool {
        (!self.ctrl || ctrl_down)
            && (!self.shift || shift_down)
            && (!self.alt || alt_down)
            && (!self.win || win_down)
            && pressed_keys.contains(&self.key)
    }

    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.win {
            parts.push("Win");
        }
        parts.push(&self.key);
        parts.join("+")
    }
}

impl Default for HotkeyCombo {
    fn default() -> Self {
        Self {
            ctrl: true,
            shift: true,
            alt: false,
            win: false,
            key: "L".to_string(),
        }
    }
}

fn normalize_config_key(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

/// Shared helper: map a canonical enum-to-string name to its display form.
/// Uses a macro so all return values carry `'static` lifetime.
macro_rules! key_name_to_display {
    ($name:expr) => {{
        let name: &str = $name;
        match name {
            "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J" | "K" | "L" | "M"
            | "N" | "O" | "P" | "Q" | "R" | "S" | "T" | "U" | "V" | "W" | "X" | "Y" | "Z"
            | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
            | "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11"
            | "F12" => Some(name.to_string()),
            "SPACE" => Some("SPACE".to_string()),
            "RETURN" | "ENTER" => Some("ENTER".to_string()),
            "TAB" => Some("TAB".to_string()),
            "ESC" | "ESCAPE" => Some("ESC".to_string()),
            _ => None,
        }
    }};
}

/// Convert an `rdev::Key` into a canonical key display string.
fn normalize_rdev_key(key: rdev::Key) -> Option<String> {
    use rdev::Key;
    let name = match key {
        Key::KeyA => "A",
        Key::KeyB => "B",
        Key::KeyC => "C",
        Key::KeyD => "D",
        Key::KeyE => "E",
        Key::KeyF => "F",
        Key::KeyG => "G",
        Key::KeyH => "H",
        Key::KeyI => "I",
        Key::KeyJ => "J",
        Key::KeyK => "K",
        Key::KeyL => "L",
        Key::KeyM => "M",
        Key::KeyN => "N",
        Key::KeyO => "O",
        Key::KeyP => "P",
        Key::KeyQ => "Q",
        Key::KeyR => "R",
        Key::KeyS => "S",
        Key::KeyT => "T",
        Key::KeyU => "U",
        Key::KeyV => "V",
        Key::KeyW => "W",
        Key::KeyX => "X",
        Key::KeyY => "Y",
        Key::KeyZ => "Z",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        Key::Space => "SPACE",
        Key::Return => "RETURN",
        Key::Tab => "TAB",
        Key::Escape => "ESC",
        _ => return None,
    };
    key_name_to_display!(name)
}

/// Convert a `device_query::Keycode` into a canonical key display string.
fn normalize_device_key(key: Keycode) -> Option<String> {
    let name = match key {
        Keycode::A => "A",
        Keycode::B => "B",
        Keycode::C => "C",
        Keycode::D => "D",
        Keycode::E => "E",
        Keycode::F => "F",
        Keycode::G => "G",
        Keycode::H => "H",
        Keycode::I => "I",
        Keycode::J => "J",
        Keycode::K => "K",
        Keycode::L => "L",
        Keycode::M => "M",
        Keycode::N => "N",
        Keycode::O => "O",
        Keycode::P => "P",
        Keycode::Q => "Q",
        Keycode::R => "R",
        Keycode::S => "S",
        Keycode::T => "T",
        Keycode::U => "U",
        Keycode::V => "V",
        Keycode::W => "W",
        Keycode::X => "X",
        Keycode::Y => "Y",
        Keycode::Z => "Z",
        Keycode::Key0 => "0",
        Keycode::Key1 => "1",
        Keycode::Key2 => "2",
        Keycode::Key3 => "3",
        Keycode::Key4 => "4",
        Keycode::Key5 => "5",
        Keycode::Key6 => "6",
        Keycode::Key7 => "7",
        Keycode::Key8 => "8",
        Keycode::Key9 => "9",
        Keycode::F1 => "F1",
        Keycode::F2 => "F2",
        Keycode::F3 => "F3",
        Keycode::F4 => "F4",
        Keycode::F5 => "F5",
        Keycode::F6 => "F6",
        Keycode::F7 => "F7",
        Keycode::F8 => "F8",
        Keycode::F9 => "F9",
        Keycode::F10 => "F10",
        Keycode::F11 => "F11",
        Keycode::F12 => "F12",
        Keycode::Space => "SPACE",
        Keycode::Enter => "ENTER",
        Keycode::Tab => "TAB",
        Keycode::Escape => "ESC",
        _ => return None,
    };
    key_name_to_display!(name)
}


