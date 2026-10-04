use crate::api::{TranscriptSearchResult, TranscriptSegment};
use crate::diarization::pipeline::DiarizedSegment;
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

pub struct TranscriptsRepository;

impl TranscriptsRepository {
    /// Saves a new meeting and its associated transcript segments.
    /// This function uses a transaction to ensure that either both the meeting
    /// and all its transcripts are saved, or none of them are.
    pub async fn save_transcript(
        pool: &SqlitePool,
        meeting_title: &str,
        transcripts: &[TranscriptSegment],
        folder_path: Option<String>,
    ) -> Result<String, SqlxError> {
        let meeting_id = format!("meeting-{}", Uuid::new_v4());

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now();

        // 1. Create the new meeting
        let result = sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&meeting_id)
        .bind(meeting_title)
        .bind(now)
        .bind(now)
        .bind(&folder_path)
        .execute(&mut *transaction)
        .await;

        if let Err(e) = result {
            error!("Failed to create meeting '{}': {}", meeting_title, e);
            transaction.rollback().await?;
            return Err(e);
        }

        info!("Successfully created meeting with id: {}", meeting_id);

        // 2. Save each transcript segment with audio timing fields
        for segment in transcripts {
            let transcript_id = format!("transcript-{}", Uuid::new_v4());
            let result = sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .bind(&segment.speaker)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!(
                    "Failed to save transcript segment for meeting {}: {}",
                    meeting_id, e
                );
                transaction.rollback().await?;
                return Err(e);
            }
        }

        info!(
            "Successfully saved {} transcript segments for meeting {}",
            transcripts.len(),
            meeting_id
        );

        // Commit the transaction
        transaction.commit().await?;

        Ok(meeting_id)
    }

    /// Updates the speaker label of a single transcript segment.
    /// Returns false if no row matched (segment not found or wrong meeting).
    pub async fn update_transcript_speaker(
        pool: &SqlitePool,
        meeting_id: &str,
        transcript_id: &str,
        speaker: &str,
    ) -> Result<bool, SqlxError> {
        if meeting_id.trim().is_empty() || transcript_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id and transcript_id cannot be empty".to_string(),
            ));
        }

        let speaker_value = if speaker.trim().is_empty() {
            None
        } else {
            Some(speaker.trim().to_string())
        };

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let rows_affected = sqlx::query(
            "UPDATE transcripts SET speaker = ? WHERE id = ? AND meeting_id = ?",
        )
        .bind(speaker_value)
        .bind(transcript_id)
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

        if rows_affected.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }

        transaction.commit().await?;
        Ok(true)
    }

    /// Bulk-renames a speaker label across all transcript segments of a meeting.
    ///
    /// `UPDATE transcripts SET speaker = :to WHERE meeting_id = :meeting_id AND speaker = :from`
    /// — overwriting semantics, matching the one-at-a-time behavior: any segment currently
    /// labelled `from` becomes `to`, even if `to` already exists elsewhere in the meeting.
    ///
    /// Returns the number of transcript rows renamed.
    pub async fn rename_speaker(
        pool: &SqlitePool,
        meeting_id: &str,
        from: &str,
        to: &str,
    ) -> Result<u64, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let from_value = if from.trim().is_empty() {
            None
        } else {
            Some(from.trim().to_string())
        };
        let to_value = if to.trim().is_empty() {
            None
        } else {
            Some(to.trim().to_string())
        };

        let rows_affected = sqlx::query(
            "UPDATE transcripts SET speaker = ? WHERE meeting_id = ? AND speaker = ?",
        )
        .bind(&to_value)
        .bind(meeting_id)
        .bind(&from_value)
        .execute(pool)
        .await?;

        info!(
            "rename_speaker: renamed {} transcript rows from {:?} to {:?} for meeting {}",
            rows_affected.rows_affected(),
            from_value,
            to_value,
            meeting_id,
        );

        Ok(rows_affected.rows_affected())
    }

    /// Bulk-applies diarized speaker labels to transcript segments by audio time range.
    ///
    /// For each diarized segment `(start, end, speaker)`:
    ///   UPDATE transcripts SET speaker = ?
    ///   WHERE meeting_id = ?
    ///     AND audio_start_time BETWEEN ? AND ?
    ///     AND speaker IS NULL
    /// with a ~0.05s pad on each boundary (mirrors the proven `apply_speakers.py`
    /// overlap logic from the POC sidecar). Only fills segments that still have
    /// `speaker IS NULL` so it never overwrites a manual/user-set label.
    ///
    /// Returns the total number of transcript rows updated.
    pub async fn apply_speaker_merge(
        pool: &SqlitePool,
        meeting_id: &str,
        segments: &[DiarizedSegment],
    ) -> Result<u64, SqlxError> {
        const PAD: f64 = 0.05;
        let mut total: u64 = 0;
        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        for seg in segments {
            let start = seg.start - PAD;
            let end = seg.end + PAD;
            let result = sqlx::query(
                "UPDATE transcripts SET speaker = ?
                 WHERE meeting_id = ?
                   AND audio_start_time >= ?
                   AND audio_start_time <= ?
                   AND speaker IS NULL",
            )
            .bind(&seg.speaker)
            .bind(meeting_id)
            .bind(start)
            .bind(end)
            .execute(&mut *transaction)
            .await?;
            total += result.rows_affected();
        }

        transaction.commit().await?;
        info!(
            "apply_speaker_merge: updated {} transcript rows for meeting {} ({} diarized segments)",
            total,
            meeting_id,
            segments.len()
        );
        Ok(total)
    }

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?",
        )
        .bind(&search_query)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|(id, title, transcript, timestamp)| {
                let match_context = Self::get_match_context(&transcript, query);
                TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                }
            })
            .collect();

        Ok(results)
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    fn get_match_context(transcript: &str, query: &str) -> String {
        let transcript_lower = transcript.to_lowercase();
        let query_lower = query.to_lowercase();

        match transcript_lower.find(&query_lower) {
            Some(match_index) => {
                let start_index = match_index.saturating_sub(100);
                let end_index = (match_index + query.len() + 100).min(transcript.len());

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.push_str(&transcript[start_index..end_index]);
                if end_index < transcript.len() {
                    context.push_str("...");
                }
                context
            }
            None => transcript.chars().take(200).collect(), // Fallback to the start of the transcript
        }
    }
}
