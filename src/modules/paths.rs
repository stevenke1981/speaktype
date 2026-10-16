use std::path::PathBuf;

const APP_DIR_NAME: &str = "SpeakType";

pub fn app_base_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SPEAKTYPE_HOME") {
        return PathBuf::from(dir);
    }
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join(APP_DIR_NAME)
}

pub fn config_dir() -> PathBuf {
    app_base_dir().join("config")
}

pub fn data_dir() -> PathBuf {
    app_base_dir().join("data")
}

pub fn models_dir() -> PathBuf {
    app_base_dir().join("models")
}

pub fn recordings_dir() -> PathBuf {
    app_base_dir().join("recordings")
}

pub fn logs_dir() -> PathBuf {
    app_base_dir().join("logs")
}
