use crate::modules::error::log_error;
use crate::modules::paths;
use chrono::{DateTime, Local};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

#[derive(Clone, Debug)]
pub struct RecordingFile {
    pub path: PathBuf,
    pub file_name: String,
    pub size_bytes: u64,
    pub modified: DateTime<Local>,
}

pub fn list_recordings(date_filter: &str) -> Vec<RecordingFile> {
    let filter = date_filter.trim();
    let mut files = fs::read_dir(paths::recordings_dir())
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("wav") {
                return None;
            }

            let metadata = entry.metadata().ok()?;
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let modified: DateTime<Local> = modified.into();
            let file_name = path.file_name()?.to_string_lossy().to_string();

            if !filter.is_empty() && !file_name.contains(filter) {
                return None;
            }

            Some(RecordingFile {
                path,
                file_name,
                size_bytes: metadata.len(),
                modified,
            })
        })
        .collect::<Vec<_>>();

    files.sort_by(|left, right| right.modified.cmp(&left.modified));
    files
}

pub fn cleanup_recordings(retention_days: u32, max_total_mb: u64) {
    let mut files = list_recordings("");

    if retention_days > 0 {
        let cutoff = Local::now() - chrono::Duration::days(retention_days as i64);
        for file in files.iter().filter(|file| file.modified < cutoff) {
            if let Err(err) = fs::remove_file(&file.path) {
                log_error(
                    "recording cleanup",
                    format!("{}: {}", file.path.display(), err),
                );
            }
        }
        files = list_recordings("");
    }

    if max_total_mb == 0 {
        return;
    }

    let max_total_bytes = max_total_mb.saturating_mul(1024 * 1024);
    let mut total_bytes = files.iter().map(|file| file.size_bytes).sum::<u64>();
    files.sort_by(|left, right| left.modified.cmp(&right.modified));

    for file in files {
        if total_bytes <= max_total_bytes {
            break;
        }
        if fs::remove_file(&file.path).is_ok() {
            total_bytes = total_bytes.saturating_sub(file.size_bytes);
        }
    }
}

pub fn delete_recording(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|err| format!("刪除錄音檔失敗: {}", err))
}

pub fn open_recordings_folder() -> Result<(), String> {
    fs::create_dir_all(paths::recordings_dir()).map_err(|err| err.to_string())?;
    open_path(&paths::recordings_dir())
}

pub fn play_recording(path: &Path) -> Result<(), String> {
    open_path(path)
}

fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", &path.display().to_string()]);
        command.creation_flags(CREATE_NO_WINDOW);
        command
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("開啟失敗: {}", err))
    }

    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("開啟失敗: {}", err))
    }
}
