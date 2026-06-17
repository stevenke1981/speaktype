use crate::modules::paths;
use chrono::Local;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug)]
pub enum SpeakTypeError {
    Audio(String),
    Config(String),
    Engine(String),
    Io(std::io::Error),
    Model(String),
    Network(String),
    Transcription(String),
}

impl fmt::Display for SpeakTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Audio(msg) => write!(f, "audio error: {msg}"),
            Self::Config(msg) => write!(f, "config error: {msg}"),
            Self::Engine(msg) => write!(f, "engine error: {msg}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Model(msg) => write!(f, "model error: {msg}"),
            Self::Network(msg) => write!(f, "network error: {msg}"),
            Self::Transcription(msg) => write!(f, "transcription error: {msg}"),
        }
    }
}

impl std::error::Error for SpeakTypeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SpeakTypeError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, SpeakTypeError>;

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
    Some(paths::logs_dir().join("speaktype.log"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaktype_error_display() {
        let err = SpeakTypeError::Config("missing field".into());
        assert_eq!(err.to_string(), "config error: missing field");
    }

    #[test]
    fn speaktype_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: SpeakTypeError = io_err.into();
        assert!(matches!(err, SpeakTypeError::Io(_)));
    }

    #[test]
    fn speaktype_error_source_for_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = SpeakTypeError::Io(io_err);
        assert!(err.source().is_some());
    }

    #[test]
    fn speaktype_error_source_is_none_for_non_io() {
        let err = SpeakTypeError::Audio("no mic".into());
        assert!(err.source().is_none());
    }

    #[test]
    fn log_file_path_returns_some() {
        let path = log_file_path();
        assert!(path.is_some());
        assert!(path.unwrap().ends_with("speaktype.log"));
    }
}
