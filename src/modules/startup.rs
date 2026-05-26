use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "SpeakType";

pub fn set_launch_on_startup(enabled: bool, start_hidden_to_tray: bool) -> Result<(), String> {
    if enabled {
        enable_startup(start_hidden_to_tray)
    } else {
        disable_startup()
    }
}

pub fn startup_command_value(start_hidden_to_tray: bool) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|err| format!("無法取得程式路徑: {err}"))?;
    let mut value = format!("\"{}\"", exe.display());
    if start_hidden_to_tray {
        value.push_str(" --tray");
    }
    Ok(value)
}

fn enable_startup(start_hidden_to_tray: bool) -> Result<(), String> {
    let value = startup_command_value(start_hidden_to_tray)?;
    let mut command = Command::new("reg");
    command.args([
        "add", RUN_KEY, "/v", VALUE_NAME, "/t", "REG_SZ", "/d", &value, "/f",
    ]);
    run_reg(command, "設定 Windows 開機啟動失敗")
}

fn disable_startup() -> Result<(), String> {
    let mut command = Command::new("reg");
    command.args(["delete", RUN_KEY, "/v", VALUE_NAME, "/f"]);

    match run_reg(command, "停用 Windows 開機啟動失敗") {
        Ok(()) => Ok(()),
        Err(err) if err.contains("unable to find") || err.contains("找不到") => Ok(()),
        Err(err) => Err(err),
    }
}

fn run_reg(mut command: Command, context: &str) -> Result<(), String> {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .output()
        .map_err(|err| format!("{context}: {err}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    Err(format!("{context}: {detail}"))
}
