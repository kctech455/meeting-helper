//! ONNX model acquisition for native diarization.
//!
//! Both models are PUBLIC GitHub release artifacts from thewh1teagle/pyannote-rs
//! v0.1.0 (they host ONLY pytorch_model.bin on HF — there is no gated ONNX to
//! unlock; verified 2026-10-03). Weights are third-party:
//!   - segmentation-3.0.onnx            (MIT, pyannote)
//!   - wespeaker_en_voxceleb_CAM++.onnx (Apache-2.0, wespeaker)
//! sha256 cross-verified against the screenpipe LFS pointers (seg b78fc48..,
//! wespeaker c46fad1..) at de-risk time.
//!
//! See the local-speech-diarization skill §native-rust for the full provenance.

use anyhow::{anyhow, Context, Result};
use log::{error, info};
use std::path::{Path, PathBuf};

pub const SEGMENTATION_MODEL: &str = "segmentation-3.0.onnx";
pub const EMBEDDING_MODEL: &str = "wespeaker_en_voxceleb_CAM++.onnx";

const BASE_URL: &str =
    "https://github.com/thewh1teagle/pyannote-rs/releases/download/v0.1.0";

/// Dir under the app models dir where diarization ONNX weights live.
pub fn diarization_dir(models_dir: &Path) -> PathBuf {
    models_dir.join("diarization")
}

/// Ensure both models are present, downloading if missing. Returns (seg, emb) paths.
pub async fn ensure_models(models_dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let dir = diarization_dir(models_dir);
    std::fs::create_dir_all(&dir).context("create diarization model dir")?;

    let seg = fetch_if_missing(&dir, SEGMENTATION_MODEL).await?;
    let emb = fetch_if_missing(&dir, EMBEDDING_MODEL).await?;
    Ok((seg, emb))
}

async fn fetch_if_missing(dir: &Path, filename: &str) -> Result<PathBuf> {
    let path = dir.join(filename);
    if path.exists() {
        info!("diarization model present: {}", path.display());
        return Ok(path);
    }
    let url = format!("{BASE_URL}/{filename}");
    info!("downloading diarization model {filename} from {url}");
    let resp = reqwest::get(&url)
        .await
        .context(format!("download {filename}"))?
        .error_for_status()
        .context(format!("HTTP error downloading {filename}"))?;

    // stream to disk
    let bytes = resp.bytes().await.context("read {filename} body")?;
    tokio::fs::write(&path, &bytes)
        .await
        .context(format!("write {filename}"))?;
    info!("saved {} ({} bytes)", path.display(), bytes.len());
    Ok(path)
}

/// Best-effort guard: if a model file is suspiciously small (<200 bytes) it's an LFS
/// pointer or a failed download — delete so the next run re-downloads.
pub fn verify_model_size(path: &Path) -> bool {
    match path.metadata() {
        Ok(m) if m.len() > 200 => true,
        Ok(_) => {
            if let Err(e) = std::fs::remove_file(path) {
                error!("failed to remove bad model {}: {}", path.display(), e);
            }
            false
        }
        Err(e) => {
            error!("cannot stat model {}: {}", path.display(), e);
            false
        }
    }
}

/// Convert anyhow result into a user-facing string for a Tauri command.
pub fn err_string(e: anyhow::Error) -> String {
    format!("{e:#}")
}
