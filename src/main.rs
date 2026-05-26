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

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 560.0])
            .with_title("SpeakType"),
        ..Default::default()
    };

    if let Err(err) = eframe::run_native(
        "SpeakType",
        options,
        Box::new(|cc| Box::new(SpeakTypeApp::new(&cc.egui_ctx))),
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
            let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
            if handle.is_null() {
                log_error("single instance", "CreateMutexW returned null handle");
                return None;
            }

            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
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
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
