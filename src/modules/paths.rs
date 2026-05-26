use std::path::PathBuf;

const APP_DIR_NAME: &str = "SpeakType";

pub fn app_base_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
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
