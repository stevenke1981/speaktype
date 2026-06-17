#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;

use app::SpeakTypeApp;
use eframe::NativeOptions;
use speaktype::modules::error::{install_panic_hook, log_error};

fn main() {
    install_panic_hook();

    let Some(_single_instance) = SingleInstanceGuard::try_acquire() else {
        return;
    };

    let start_hidden_to_tray = std::env::args().any(|arg| arg == "--tray");
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 560.0])
            .with_title("SpeakType"),
        ..Default::default()
    };

    if let Err(err) = eframe::run_native(
        "SpeakType",
        options,
        Box::new(move |cc| Box::new(SpeakTypeApp::new(&cc.egui_ctx, start_hidden_to_tray))),
    ) {
        log_error("app startup", err);
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
