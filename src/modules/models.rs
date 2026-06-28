use crate::modules::paths;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Result as IoResult};
use std::path::PathBuf;

pub const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCatalogEntry {
    pub name: &'static str,
    pub file_name: &'static str,
    pub label: &'static str,
    pub approx_size: &'static str,
    pub recommendation: &'static str,
}

pub const MODEL_CATALOG: &[ModelCatalogEntry] = &[
    ModelCatalogEntry {
        name: "large-v3-turbo",
        file_name: "ggml-large-v3-turbo.bin",
        label: "large-v3-turbo",
        approx_size: "約 1.6 GB",
        recommendation: "品質優先，建議 NVIDIA GPU",
    },
    ModelCatalogEntry {
        name: "medium",
        file_name: "ggml-medium.bin",
        label: "medium",
        approx_size: "約 1.5 GB",
        recommendation: "平衡品質與速度，GPU/高階 CPU",
    },
    ModelCatalogEntry {
        name: "small",
        file_name: "ggml-small.bin",
        label: "small",
        approx_size: "約 488 MB",
        recommendation: "速度優先，CPU 也較可用",
    },
];

pub fn catalog_entry(name: &str) -> ModelCatalogEntry {
    MODEL_CATALOG
        .iter()
        .copied()
        .find(|entry| entry.name == name)
        .unwrap_or(MODEL_CATALOG[0])
}

pub fn model_url(file_name: &str) -> String {
    format!("{MODEL_BASE_URL}/{file_name}")
}

pub fn model_path_for_name(model_name: &str) -> PathBuf {
    paths::models_dir().join(catalog_entry(model_name).file_name)
}

/// Remove stale `.bin.part` temporary download files from the models directory.
/// These are left behind when a model download is interrupted.
pub fn cleanup_stale_temp_files() {
    if let Ok(dir) = std::fs::read_dir(paths::models_dir()) {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("part")
                && path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map_or(false, |stem| stem.ends_with(".bin"))
            {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

pub fn sha256_file(path: &PathBuf) -> IoResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}
