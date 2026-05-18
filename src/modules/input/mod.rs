// input/mod.rs - 鍵盤輸入與熱鍵模組
// 職責：全域熱鍵偵測、PTT 控制、文字自動貼上

use device_query::{DeviceQuery, DeviceState, Keycode};
use enigo::{Enigo, Key, KeyboardControllable};
use std::collections::VecDeque;
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

impl GlobalHotkey {
    pub fn new() -> Self {
        let hotkey_pressed = Arc::new(AtomicBool::new(false));
        let hotkey_clone = hotkey_pressed.clone();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let events_clone = events.clone();

        // 啟動 rdev 全域監聽器
        thread::spawn(move || {
            use rdev::{listen, Event, EventType, Key};
            let mut ctrl_down = false;
            let mut shift_down = false;
            let mut l_down = false;
            let mut combo_down = false;

            if let Err(error) = listen(move |event: Event| {
                match event.event_type {
                    EventType::KeyPress(Key::ControlLeft | Key::ControlRight) => {
                        ctrl_down = true;
                    }
                    EventType::KeyPress(Key::ShiftLeft | Key::ShiftRight) => {
                        shift_down = true;
                    }
                    EventType::KeyPress(Key::KeyL) => {
                        l_down = true;
                    }
                    EventType::KeyRelease(Key::ControlLeft | Key::ControlRight) => {
                        ctrl_down = false;
                    }
                    EventType::KeyRelease(Key::ShiftLeft | Key::ShiftRight) => {
                        shift_down = false;
                    }
                    EventType::KeyRelease(Key::KeyL) => {
                        l_down = false;
                    }
                    _ => {}
                }

                let next_combo_down = ctrl_down && shift_down && l_down;
                if next_combo_down != combo_down {
                    combo_down = next_combo_down;
                    if let Ok(mut events) = events_clone.lock() {
                        events.push_back(if combo_down {
                            HotkeyEvent::Pressed
                        } else {
                            HotkeyEvent::Released
                        });
                    }
                }
                hotkey_clone.store(next_combo_down, Ordering::Relaxed);
            }) {
                eprintln!("[hotkey] rdev 監聽錯誤: {:?}", error);
            }
        });

        Self {
            device_state: DeviceState::new(),
            last_hotkey_pressed: false,
            hotkey_pressed,
            events,
        }
    }

    pub fn poll_record_hotkey_event(&mut self) -> Option<HotkeyEvent> {
        if let Ok(mut events) = self.events.lock() {
            if let Some(event) = events.pop_front() {
                self.last_hotkey_pressed = matches!(event, HotkeyEvent::Pressed);
                return Some(event);
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
        let l_down = keys.contains(&Keycode::L);
        ctrl_down && shift_down && l_down
    }
}

impl Default for GlobalHotkey {
    fn default() -> Self {
        Self::new()
    }
}

/// 熱鍵狀態（全域共享）
#[derive(Clone)]
pub struct HotkeyState {
    pub is_recording: Arc<AtomicBool>,
    pub should_exit: Arc<AtomicBool>,
}

impl HotkeyState {
    pub fn new() -> Self {
        Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            should_exit: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for HotkeyState {
    fn default() -> Self {
        Self::new()
    }
}

/// 簡單的 PTT 熱鍵監聽器（背景執行緒）
pub fn start_hotkey_listener(state: HotkeyState) {
    thread::spawn(move || {
        println!("[input] 熱鍵監聽器已啟動（Ctrl+Shift+L 切換錄音）");

        loop {
            if state.should_exit.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }

        println!("[input] 熱鍵監聽器已停止");
    });
}

pub fn toggle_recording(state: &HotkeyState) -> bool {
    let current = state.is_recording.load(Ordering::Relaxed);
    let new_state = !current;
    state.is_recording.store(new_state, Ordering::Relaxed);
    new_state
}

pub fn shutdown(state: &HotkeyState) {
    state.should_exit.store(true, Ordering::Relaxed);
}
