use crate::modules::error::log_error;
#[cfg(not(windows))]
use crate::modules::icon::load_app_icon;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub const TRAY_SHOW_MAIN: &str = "speaktype.tray.show_main";
pub const TRAY_TOGGLE_RECORDING: &str = "speaktype.tray.toggle_recording";
pub const TRAY_SETTINGS: &str = "speaktype.tray.settings";
pub const TRAY_HISTORY: &str = "speaktype.tray.history";
pub const TRAY_EXIT: &str = "speaktype.tray.exit";

pub struct TrayManager {
    _icon: TrayIcon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ShowMain,
    ToggleRecording,
    OpenSettings,
    OpenHistory,
    Exit,
}

impl TrayManager {
    pub fn new() -> Result<Self, String> {
        let menu = Menu::new();
        let show_main = MenuItem::with_id(TRAY_SHOW_MAIN, "顯示主視窗", true, None);
        let toggle = MenuItem::with_id(TRAY_TOGGLE_RECORDING, "開始 / 停止錄音", true, None);
        let settings = MenuItem::with_id(TRAY_SETTINGS, "設定", true, None);
        let history = MenuItem::with_id(TRAY_HISTORY, "紀錄", true, None);
        let exit = MenuItem::with_id(TRAY_EXIT, "退出", true, None);

        menu.append(&show_main).map_err(|err| err.to_string())?;
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
            if let Some(action) = action_from_menu_id(event.id.as_ref()) {
                return Some(action);
            }
        }

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => return Some(TrayAction::ShowMain),
                _ => {}
            }
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

fn action_from_menu_id(id: &str) -> Option<TrayAction> {
    match id {
        TRAY_SHOW_MAIN => Some(TrayAction::ShowMain),
        TRAY_TOGGLE_RECORDING => Some(TrayAction::ToggleRecording),
        TRAY_SETTINGS => Some(TrayAction::OpenSettings),
        TRAY_HISTORY => Some(TrayAction::OpenHistory),
        TRAY_EXIT => Some(TrayAction::Exit),
        _ => None,
    }
}

fn create_icon() -> Result<Icon, String> {
    #[cfg(windows)]
    {
        Icon::from_resource(1, Some((32, 32))).map_err(|err| err.to_string())
    }

    #[cfg(not(windows))]
    {
        let icon = load_app_icon()?;
        Icon::from_rgba(icon.rgba, icon.width, icon.height).map_err(|err| err.to_string())
    }
}
