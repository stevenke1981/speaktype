use chrono::Local;
use directories::ProjectDirs;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown location".to_string());

        let message = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|message| (*message).to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string());

        log_error("panic", format!("{message} at {location}"));
    }));
}

pub fn log_error(context: &str, error: impl std::fmt::Display) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{timestamp}] {context}: {error}\n");

    if let Some(path) = log_file_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

pub fn log_file_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "SpeakType", "SpeakType")
        .map(|dirs| dirs.data_local_dir().join("logs").join("speaktype.log"))
}
