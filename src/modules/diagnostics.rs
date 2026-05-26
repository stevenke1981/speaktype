use crate::modules::config::AppConfig;
use crate::modules::error::log_file_path;
use crate::modules::models::{self, MODEL_CATALOG};
use crate::modules::paths;
use crate::modules::utils::device::DeviceStatus;
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

pub fn export_diagnostic_bundle(
    config: &AppConfig,
    status: &DeviceStatus,
    audio_devices: &[String],
    recent_errors: &[String],
) -> Result<PathBuf, String> {
    let diagnostics_dir = paths::app_base_dir().join("diagnostics");
    fs::create_dir_all(&diagnostics_dir).map_err(|err| err.to_string())?;

    let bundle_dir = diagnostics_dir.join(format!(
        "diagnostic_{}",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    fs::create_dir_all(&bundle_dir).map_err(|err| err.to_string())?;

    write_summary(&bundle_dir, config, status, audio_devices)?;
    write_config_snapshot(&bundle_dir, config)?;
    write_recent_errors(&bundle_dir, recent_errors)?;
    write_model_report(&bundle_dir, config)?;
    copy_log_file(&bundle_dir)?;

    Ok(bundle_dir)
}

pub fn open_diagnostic_folder(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", &path.display().to_string()]);
        command.creation_flags(CREATE_NO_WINDOW);
        command
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("開啟診斷包資料夾失敗: {err}"))
    }

    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("開啟診斷包資料夾失敗: {err}"))
    }
}

fn write_summary(
    bundle_dir: &Path,
    config: &AppConfig,
    status: &DeviceStatus,
    audio_devices: &[String],
) -> Result<(), String> {
    let mut summary = String::new();
    summary.push_str("SpeakType diagnostic bundle\n");
    summary.push_str(&format!(
        "created_at = {}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    summary.push_str(&format!("version = {}\n", env!("CARGO_PKG_VERSION")));
    summary.push_str(&format!(
        "app_base_dir = {}\n",
        paths::app_base_dir().display()
    ));
    summary.push_str(&format!("config_dir = {}\n", paths::config_dir().display()));
    summary.push_str(&format!("models_dir = {}\n", paths::models_dir().display()));
    summary.push_str(&format!("logs_dir = {}\n", paths::logs_dir().display()));
    summary.push_str(&format!(
        "recordings_dir = {} (contents intentionally excluded)\n",
        paths::recordings_dir().display()
    ));
    summary.push_str(&format!("configured_model = {}\n", config.get_model_name()));
    summary.push_str(&format!(
        "configured_model_path = {}\n",
        config.get_model_path().display()
    ));
    summary.push_str(&format!("use_cuda = {}\n", config.use_cuda));
    summary.push_str(&format!("microphone_status = {}\n", status.microphone));
    summary.push_str(&format!("gpu_status = {}\n", status.gpu));
    summary.push_str(&format!("model_status = {}\n", status.model));
    summary.push_str("audio_devices =\n");
    for device in audio_devices {
        summary.push_str(&format!("  - {device}\n"));
    }

    fs::write(bundle_dir.join("summary.txt"), summary).map_err(|err| err.to_string())
}

fn write_config_snapshot(bundle_dir: &Path, config: &AppConfig) -> Result<(), String> {
    let content = toml::to_string_pretty(config).map_err(|err| err.to_string())?;
    fs::write(bundle_dir.join("config.toml"), content).map_err(|err| err.to_string())
}

fn write_recent_errors(bundle_dir: &Path, recent_errors: &[String]) -> Result<(), String> {
    let content = if recent_errors.is_empty() {
        "No recent GUI errors.\n".to_string()
    } else {
        recent_errors.join("\n\n")
    };
    fs::write(bundle_dir.join("recent_errors.txt"), content).map_err(|err| err.to_string())
}

fn write_model_report(bundle_dir: &Path, config: &AppConfig) -> Result<(), String> {
    let mut report = String::new();
    report.push_str(&format!("active_model = {}\n", config.get_model_name()));
    report.push_str(&format!(
        "active_path = {}\n\n",
        config.get_model_path().display()
    ));

    for entry in MODEL_CATALOG {
        let path = models::model_path_for_name(entry.name);
        report.push_str(&format!("[{}]\n", entry.name));
        report.push_str(&format!("label = {}\n", entry.label));
        report.push_str(&format!("url = {}\n", models::model_url(entry.file_name)));
        report.push_str(&format!("path = {}\n", path.display()));
        report.push_str(&format!("installed = {}\n", path.exists()));
        if path.exists() {
            match models::sha256_file(&path) {
                Ok(hash) => report.push_str(&format!("sha256 = {hash}\n")),
                Err(err) => report.push_str(&format!("sha256_error = {err}\n")),
            }
        }
        report.push('\n');
    }

    fs::write(bundle_dir.join("models.txt"), report).map_err(|err| err.to_string())
}

fn copy_log_file(bundle_dir: &Path) -> Result<(), String> {
    let Some(path) = log_file_path() else {
        return Ok(());
    };
    if path.exists() {
        fs::copy(path, bundle_dir.join("speaktype.log"))
            .map(|_| ())
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}
