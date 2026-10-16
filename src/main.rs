#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

slint::include_modules!();

mod app;
mod indicator;

use speaktype::modules::error::{install_panic_hook, log_error};

fn console_print(msg: &str) {
    #[cfg(windows)]
    // SAFETY: AttachConsole has no memory-safety preconditions; it lets the
    // GUI-subsystem exe print to the console it was started from.
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(u32::MAX);
    }
    println!("{msg}");
}

/// Windows: Skia (GPU, D3D) first, then Skia's CPU rasteriser, then Slint's
/// software renderer (VMs / RDP / CI runners without a GPU). Elsewhere the
/// software renderer. `SLINT_BACKEND` overrides the choice. Every selection
/// installs the winit hook that makes the indicator a non-activating window.
fn select_renderer() {
    let selector = || slint::BackendSelector::new().with_winit_window_attributes_hook(indicator::indicator_attributes);
    if std::env::var_os("SLINT_BACKEND").is_some() {
        if let Err(err) = selector().select() {
            log_error("renderer", err.to_string());
        }
        return;
    }
    let renderers: &[&str] = if cfg!(windows) { &["skia", "skia-software", "software"] } else { &["software"] };
    for name in renderers {
        match selector().backend_name("winit".into()).renderer_name(name.to_string()).select() {
            Ok(()) => {
                log_error("renderer", format!("using winit/{name}"));
                return;
            }
            Err(err) => log_error("renderer", format!("{name}: {err}")),
        }
    }
}

fn main() {
    let startup_started = std::time::Instant::now();
    install_panic_hook();
    let args: Vec<String> = std::env::args().collect();
    // `--startup-probe <file>`: write time-to-first-frame (ms) to <file> and exit.
    let startup_probe = args
        .iter()
        .position(|a| a == "--startup-probe")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from);
    if args.iter().any(|a| a == "--version" || a == "-V") {
        console_print(&format!("SpeakType {}", env!("CARGO_PKG_VERSION")));
        return;
    }

    let Some(_single_instance) = SingleInstanceGuard::try_acquire() else {
        return;
    };

    select_renderer();
    let opts = app::Options {
        start_hidden_to_tray: args.iter().any(|arg| arg == "--tray"),
        startup_started,
        startup_probe,
    };
    if let Err(err) = app::run(opts) {
        log_error("app startup", err.to_string());
        std::process::exit(1);
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
