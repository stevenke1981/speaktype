#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;

use app::SpeakTypeApp;
use eframe::NativeOptions;
use speaktype::modules::error::{install_panic_hook, log_error};
use std::sync::Arc;

const WINDOW_TITLE: &str = "SpeakType · 語音輸入";
const WINDOW_DEFAULT_SIZE: [f32; 2] = [640.0, 680.0];
const WINDOW_MIN_SIZE: [f32; 2] = [480.0, 520.0];

fn main() {
    install_panic_hook();

    let Some(_single_instance) = SingleInstanceGuard::try_acquire() else {
        return;
    };

    let start_hidden_to_tray = std::env::args().any(|arg| arg == "--tray");
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(WINDOW_DEFAULT_SIZE)
            .with_min_inner_size(WINDOW_MIN_SIZE)
            .with_title(WINDOW_TITLE)
            .with_icon(Arc::new(app_icon())),
        ..Default::default()
    };

    if let Err(err) = eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| Box::new(SpeakTypeApp::new(&cc.egui_ctx, start_hidden_to_tray))),
    ) {
        log_error("app startup", err);
    }
}

/// 建立不依賴外部圖片檔的 SpeakType 麥克風圖示。
///
/// 使用程式生成可避免安裝包遺漏資源，也能讓開發版與正式版使用相同圖示。
fn app_icon() -> egui::IconData {
    const SIZE: usize = 32;
    let mut rgba = vec![0_u8; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let index = (y * SIZE + x) * 4;
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;

            // 圓形主背景，採用 SpeakType 預設主題的暖棕色。
            if dx * dx + dy * dy <= 14.5 * 14.5 {
                rgba[index..index + 4].copy_from_slice(&[139, 111, 71, 255]);
            }

            // 麥克風膠囊。
            let mic_body = (x >= 12 && x <= 19 && y >= 7 && y <= 19)
                && ((y >= 10 && y <= 16)
                    || ((x as i32 - 15).pow(2) + (y as i32 - 10).pow(2) <= 16)
                    || ((x as i32 - 15).pow(2) + (y as i32 - 16).pow(2) <= 16));

            // 支架弧線、直桿與底座。
            let side_arc = (y >= 14 && y <= 21)
                && ((x == 9 || x == 10 || x == 21 || x == 22)
                    || (y >= 20 && x >= 11 && x <= 20));
            let stem = (x == 15 || x == 16) && y >= 20 && y <= 25;
            let base = y >= 25 && y <= 26 && x >= 11 && x <= 20;

            if mic_body || side_arc || stem || base {
                rgba[index..index + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }

    egui::IconData {
        rgba,
        width: SIZE as u32,
        height: SIZE as u32,
    }
}

struct SingleInstanceGuard {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl SingleInstanceGuard {
    fn try_acquire() -> Option<Self> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
            use windows_sys::Win32::System::Threading::CreateMutexW;

            let name = wide_null("Global\\SpeakType.SingleInstance");
            // SAFETY: CreateMutexW is called with a valid wide string and
            // null security attributes. The returned handle is checked for
            // null before use and properly closed on error/cleanup.
            let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
            if handle.is_null() {
                log_error("single instance", "CreateMutexW returned null handle");
                return None;
            }

            // SAFETY: GetLastError is called immediately after CreateMutexW,
            // before any other API call that could overwrite the error code.
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                // SAFETY: handle is known to be valid (non-null) from the
                // check above, and CloseHandle is the correct cleanup.
                unsafe {
                    CloseHandle(handle);
                }
                return None;
            }

            Some(Self { handle })
        }

        #[cfg(not(windows))]
        {
            Some(Self {})
        }
    }
}

#[cfg(windows)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        // SAFETY: self.handle is a valid handle from CreateMutexW; this is
        // the single cleanup point and is called at most once.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_icon_has_expected_dimensions_and_buffer_size() {
        let icon = app_icon();
        assert_eq!(icon.width, 32);
        assert_eq!(icon.height, 32);
        assert_eq!(icon.rgba.len(), 32 * 32 * 4);
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }
}
