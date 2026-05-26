use crate::modules::error::log_error;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub const TRAY_TOGGLE_RECORDING: &str = "speaktype.tray.toggle_recording";
pub const TRAY_SETTINGS: &str = "speaktype.tray.settings";
pub const TRAY_HISTORY: &str = "speaktype.tray.history";
pub const TRAY_EXIT: &str = "speaktype.tray.exit";

pub struct TrayManager {
    _icon: TrayIcon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ToggleRecording,
    OpenSettings,
    OpenHistory,
    Exit,
}

impl TrayManager {
    pub fn new() -> Result<Self, String> {
        let menu = Menu::new();
        let toggle = MenuItem::with_id(TRAY_TOGGLE_RECORDING, "開始 / 停止錄音", true, None);
        let settings = MenuItem::with_id(TRAY_SETTINGS, "設定", true, None);
        let history = MenuItem::with_id(TRAY_HISTORY, "紀錄", true, None);
        let exit = MenuItem::with_id(TRAY_EXIT, "退出", true, None);

        menu.append(&toggle).map_err(|err| err.to_string())?;
        menu.append(&settings).map_err(|err| err.to_string())?;
        menu.append(&history).map_err(|err| err.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|err| err.to_string())?;
        menu.append(&exit).map_err(|err| err.to_string())?;

        let icon = TrayIconBuilder::new()
            .with_tooltip("SpeakType")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(true)
            .with_icon(create_icon()?)
            .build()
            .map_err(|err| err.to_string())?;

        Ok(Self { _icon: icon })
    }

    pub fn poll_action(&self) -> Option<TrayAction> {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let action = match event.id.as_ref() {
                TRAY_TOGGLE_RECORDING => TrayAction::ToggleRecording,
                TRAY_SETTINGS => TrayAction::OpenSettings,
                TRAY_HISTORY => TrayAction::OpenHistory,
                TRAY_EXIT => TrayAction::Exit,
                _ => continue,
            };
            return Some(action);
        }
        None
    }
}

pub fn create_tray() -> Option<TrayManager> {
    match TrayManager::new() {
        Ok(tray) => Some(tray),
        Err(err) => {
            log_error("tray init", err);
            None
        }
    }
}

fn create_icon() -> Result<Icon, String> {
    let width = 32_u32;
    let height = 32_u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - 16.0;
            let dy = y as f32 - 16.0;
            let distance = (dx * dx + dy * dy).sqrt();
            let inside = distance <= 14.0;
            let mic_body = (12..=20).contains(&x) && (7..=20).contains(&y);
            let mic_stem = (15..=17).contains(&x) && (20..=26).contains(&y);
            let mic_base = (10..=22).contains(&x) && (25..=27).contains(&y);

            let (r, g, b, a) = if mic_body || mic_stem || mic_base {
                (255, 255, 255, 255)
            } else if inside {
                (36, 168, 96, 255)
            } else {
                (0, 0, 0, 0)
            };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }

    Icon::from_rgba(rgba, width, height).map_err(|err| err.to_string())
}
