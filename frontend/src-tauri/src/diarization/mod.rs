//! Native speaker diarization for meeting-helper (Option B — in-app, no Python).
//!
//! Trigger: `start_diarization` Tauri command, fired from the frontend right after
//! a meeting is saved (see `useRecordingStop.ts`). Pipeline lives in `pipeline.rs`;
//! model acquisition in `models.rs`; DB merge in the transcripts repository.
//!
//! Status: Phase 1/1b validated (K-estimator + global clustering); this module is
//! the in-app wiring of that validated pipeline.

pub mod models;
pub mod pipeline;

use crate::database::repositories::transcript::TranscriptsRepository;
use crate::diarization::models::err_string;
use crate::state::AppState;
use anyhow::{Context, Result};
use log::{info, warn};
use std::path::Path;
use std::sync::OnceLock;
use tauri::Manager;
use tokio::sync::Mutex;

/// In-memory guard so only one diarization runs at a time (model load is heavy).
static DIARIZATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub mod commands {
    use super::*;

    /// Fire-and-forget: decode the meeting audio, run the diarization pipeline,
    /// then merge labels into `transcripts.speaker`. Emits `diarization-progress`
    /// events (loading/predicting/embedding/clustering/applying/done/error).
    ///
    /// Caller provides `audio_path` (the final merged audio.mp4) and `meeting_id`.
    #[tauri::command]
    pub async fn start_diarization(
        app: tauri::AppHandle,
        state: tauri::State<'_, AppState>,
        audio_path: String,
        meeting_id: String,
    ) -> Result<String, String> {
        let _guard = DIARIZATION_LOCK.get_or_init(|| Mutex::new(())).lock().await;

        emit(&app, "diarization-progress", "loading");
        info!("start_diarization: audio={audio_path} meeting={meeting_id}");

        // 1. models dir (app_data_dir/models)
        let models_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("failed to resolve app data dir: {e}"))?
            .join("models");
        std::fs::create_dir_all(&models_dir).map_err(|e| format!("create models dir: {e}"))?;

        // 2. ensure ONNX models
        let (seg_model, emb_model) = models::ensure_models(&models_dir)
            .await
            .map_err(err_string)?;
        if !models::verify_model_size(&seg_model) || !models::verify_model_size(&emb_model) {
            return Err("diarization models failed integrity check".into());
        }

        // 3. decode audio to 16k mono i16
        emit(&app, "diarization-progress", "decoding");
        let decoded = decode_to_16k_i16(Path::new(&audio_path)).map_err(err_string)?;
        info!("decoded {} samples ({}s)", decoded.0.len(), decoded.1);

        // 4. run pipeline
        emit(&app, "diarization-progress", "predicting");
        let segments = tokio::task::spawn_blocking(move || {
            pipeline::run_pipeline(&decoded.0, decoded.1, &seg_model.to_string_lossy(), &emb_model.to_string_lossy())
        })
        .await
        .map_err(|e| format!("diarization task join: {e}"))?
        .map_err(err_string)?;

        if segments.is_empty() {
            warn!("diarization: no segments produced for meeting {meeting_id}");
            emit(&app, "diarization-progress", "done");
            emit(&app, "diarization-done", &meeting_id);
            return Ok("no speakers detected".into());
        }

        // 5. merge labels into transcripts.speaker
        emit(&app, "diarization-progress", "applying");
        let pool = state.db_manager.pool().clone();
        let count = TranscriptsRepository::apply_speaker_merge(
            &pool,
            &meeting_id,
            &segments,
        )
        .await
        .map_err(|e| format!("merge speakers: {e}"))?;
        info!("applied {count} speaker labels to meeting {meeting_id}");

        emit(&app, "diarization-progress", "done");
        emit(&app, "diarization-done", &meeting_id);
        Ok(format!("applied {count} speaker labels"))
    }

    /// Small helper to emit a progress event without failing the command.
    fn emit(app: &tauri::AppHandle, event: &str, stage: &str) {
        use tauri::Emitter;
        let _ = app.emit(event, serde_json::json!({ "stage": stage }));
    }

    /// Decode an audio file to 16k mono i16 samples. Returns (samples, sample_rate).
    fn decode_to_16k_i16(path: &Path) -> Result<(Vec<i16>, u32)> {
        let decoded = crate::audio::decoder::decode_audio_file(path)
            .with_context(|| format!("decode audio {}", path.display()))?;
        // to_whisper_format returns 16k mono f32
        let mono_f32 = decoded.to_whisper_format();
        let sr = 16000u32;
        // convert f32 (normalized ~[-1,1]) to i16
        let mut i16s = Vec::with_capacity(mono_f32.len());
        for s in mono_f32 {
            let cl = s.clamp(-1.0, 1.0);
            i16s.push((cl * 32767.0) as i16);
        }
        Ok((i16s, sr))
    }
}
