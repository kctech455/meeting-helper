# SCOPE — Native Speaker Diarization for meetily.exe (Option B)

> Goal: make the Tauri app **automatically** label each transcript segment with a speaker
> after a meeting concludes — running entirely inside `meetily.exe`, no Python dependency.
> Restore the deleted pyannote module and wire it to the existing `speaker` column + rename UI.

**Status: SCOPED (not started).** Authored 2026-10-03. Companion to HANDOFF.md §8 item 4.

---

## 0. The trigger (your logic, verified & corrected)

Your instinct was right: diarization is too slow for live recording (pyannote on an hour of
audio = minutes, and the Win11 test box's Quadro P1000 can't even run CUDA — CPU-only), so it
**must** run after a meeting ends. The correction: there is no single "recording ends" event —
you must hook the **last** one.

The real pipeline of moments, in order:
1. User hits Stop → Rust `stop_recording` (`recording_commands.rs:676`) — does NOT save DB,
   hands `folder_path`/`meeting_name` to frontend via `recording-stopped` event.
2. Frontend `handleRecordingStop` (`useRecordingStop.ts`) flushes remaining transcripts,
   then `storageService.saveMeeting()` persists the meeting and returns **`meetingId`** (L256-262).
3. ✅ **THE TRIGGER POINT** — right after `saveMeeting` succeeds. At this instant:
   - The meeting row exists in SQLite (has `meeting_id`).
   - `transcripts.json` is already written to the meeting folder.
   - The final merged audio file exists at `folder_path`/`audio.mp4`
     (`recording_saver.rs:252/376` — "merge checkpoints into final audio.mp4").
4. Fire the diarization job with `(audio.mp4 path, meeting_id)` → returns async task id.

### Trigger choice
Hook in `useRecordingStop.ts` immediately after `const meetingId = responseData.meeting_id;` (L262).
Call a new Tauri command `start_diarization({ audio_path, meeting_id })` and let it run in the
background (don't block the stop/navigation flow). Emit progress back on an event
`diarization-progress` (stage/progress: loading models → predicting segments → embeddings →
clustering → applying → done).

### Why NOT the Rust stop path or a DB trigger
- Rust stop releases the mic but DB isn't saved yet — diarizing there would race the frontend save.
- A SQLite trigger can't invoke long-running Rust; keep the orchestration in the frontend/Rust boundary.

---

## 1. The backend to restore (from the deleted pyannote module)

Commit `0f638ae "Removed external dependencies"` deleted `frontend/src-tauri/src/audio/src/pyannote/`
(5 files, ~538 lines). All of it is recoverable via `git show 0f638ae^:...`. It was:
- `models.rs` (127 ln) — `get_or_download_model()`: downloads `segmentation-3.0.onnx` +
  `wespeaker_en_voxceleb_CAM++.onnx` from HuggingFace to a cache dir.
- `embedding.rs` (35 ln) — `EmbeddingExtractor` (ONNX, wespeaker voice-embedding).
- `identify.rs` (80 ln) — `EmbeddingManager` (cosine-similarity speaker identification, windowed).
- `segment.rs` (284 ln) — windowed (10s) segmentation: `create_speech_segment` computes an
  embedding, `get_speaker_from_embedding` labels it, `handle_new_segment` merges same-speaker runs;
  `get_segments()` is the entry point.
- `session.rs` (12 ln) — model session wrapper.

Key finding to preserve in scope: this was built for **live/streaming** speaker ID
(10s windows, running `EmbeddingManager`). For batch post-meeting diarization you will likely need
a **global clustering pass** (embed all segments, cluster, then assign), not just sequential
nearest-neighbor — document that as a design decision.

---

## 2. Dependencies

- **ORT (ort crate)** — ONNX Runtime. `models.rs`/`embedding.rs` already use it; restore the same
  crate. Pin a version already in the tree to avoid a second bindgen-style fight.
- **ndarray** — used for window math.
- **HF model download** — restore the download helper (reqwest or huggingface-hub crate). Models
  are ~tens of MB; cache in `%APPDATA%/meetily/models`.
- **Token/license note:** upstream pulled these ONNX models off HuggingFace; pyannote now gates
  some behind a license. Verify these two specific ONNX artifacts are freely downloadable before
  committing to this path — this is the same risk that blocked the Python sidecar. (The Python POC
  was stalled on HF token + pyannote license.) **Must be de-risked in Phase 0** (see §7).

---

## 3. The merge (populate the `speaker` column)

The fork already has everything needed to *store + display* labels — what it lacks is the thing
that *writes* them:

- Migration `20251110000001_add_speaker_field.sql` → `transcripts.speaker TEXT` ✅ exists.
- `create_transcript_segments` (`common.rs:51`) sets `speaker: None` with the comment:
  "Speaker label assigned later by the diarization merge (apply_speakers.py)" — the seam is ready.
- Repo + Tauri command + rename UI already wired (`fae6750`, `32dc563`).

New Rust merge command (parallel of `apply_speakers.py --db`): for each diarized segment
`(start, end, label)`, `UPDATE transcripts SET speaker=? WHERE audio_start_time BETWEEN ? AND ?`
(±0.05s pad, only where `speaker IS NULL`, scoped `AND meeting_id=?`). Reuse the exact
overlap logic from `apply_speakers.py:62` — it's already proven.

---

## 4. What changes in the app (the full diff surface)

### Rust (new / restored)
- Restore `pyannote/` module (5 files) into `src-tauri/src/audio/src/pyannote/`.
- `mod.rs` — add `mod pyannote;` (currently no `mod stt`, no `mod pyannote`; `stt.rs` is orphaned).
- Fix `stt.rs` compiler errors (it references `crate::pyannote`, `Speaker` types, `PyannoteModel`,
  `EmbeddingManager` — restore those names exactly).
- New `diarization/` module:
  - `start_diarization(meeting_id, audio_path)` command.
  - Batch pipeline: load ONNX models → decode audio → predict speaker segments → embed → cluster →
    merge into DB.
  - Progress events + cancellation token + a done-flag so the UI can refresh.
  - Non-blocking: spawn on `tokio`, return `task_id` immediately.

### Frontend (small)
- `useRecordingStop.ts`: after `saveMeeting`, call `start_diarization` if a toggle is on.
- New settings toggle: "Auto-diarize after recording" (default on/off — decide).
- `meeting-details` page: listen for `diarization-done`, refetch transcript (labels appear via the
  existing speaker field), show a small progress banner while running.

### Docs/assets
- Update `README.md` PRO diarization lines (the flagged §7 ⚠️ — this feature finally makes them true).
- `docs/` — note native vs Python path.

---

## 5. Rough phases

| Phase | Work | Est. |
|---|---|---|
| **0 — De-risk** | Verify the 2 ONNX models download freely (license), restore `pyannote/` from git, confirm it compiles with current crate versions. **Gate; abort if models are license-locked.** | 2-4 d |
| 1 — Batch diarization core | Implement offline `get_segments()` over a full `audio.mp4` (global clustering), standalone test against a known multi-speaker clip. | 3-5 d |
| 2 — Merge + trigger | `start_diarization` command, overlap-merge into `transcripts.speaker`, frontend trigger after `saveMeeting`, progress events. | 2-3 d |
| 3 — UI polish | Settings toggle, progress banner, auto-refetch, error/retry UX. | 2-3 d |
| 4 — Win11 build | Rebuild `meetily.exe`, verify end-to-end on the box; update README/Cargo.lock. | 1-2 d |
| **Total** | | **~2 weeks**, up to 4 with clustering + model/license risk |

---

## 6. Open decisions (need answers before build starts)
1. **Auto-cluster vs. sequential-identify** — the restored code does the latter; batch post-meeting
   likely wants the former. Pick one (auto-cluster recommended).
2. **Settings toggle default** — on or off at install.
3. **Model source** — HF direct (restore) vs vendored-on-install. Ties to Phase 0 license result.
4. **Batch rename** — do we add "rename SPEAKER_00 → John everywhere" (recommended) or rely on
   per-segment click-to-rename for v1?

## 6.5 UI speaker handling (clarified 2026-10-03)

Two flows ALREADY exist — no UI work needed for the core ask:
- **Display:** merge writes `speaker` token → blue `● SPEAKER_00` chip on the segment
  (`VirtualizedTranscriptView.tsx:128`). The `speaker` field is the only thing that changes;
  display is render-only and already handles both labeled/unlabeled states.
- **Per-segment manual rename:** click chip → inline input → type "John" → Enter → persists.
  Full chain verified: UI `VirtualizedTranscriptView.tsx:159` → hook `usePaginatedTranscripts.ts:207`
  (`api_update_transcript_speaker`, optimistic) → command `api.rs:1015` → SQL `transcript.rs:109`.
  Blank → clears label (null). Commit `32dc563` is the fix that made this work.

GAP (new feature, ~1-2 days, recommended): **batch rename**. No "rename this speaker everywhere"
exists; for a 40-segment SPEAKER_00 you'd otherwise click 40 chips. Add `api_rename_speaker(meeting_id,
from, to)` → single `UPDATE transcripts SET speaker=? WHERE speaker=? AND meeting_id=?`, surfaced as
a "Rename speaker" action. Mirrors the POC's `.speakers.json` mapping idea.
Deferred niceties: per-speaker chip colors (all blue today), persistent "SPEAKER_00 = John" mapping
so renames survive refresh.

---

## 7. The two real risks (call these out in any plan)
1. **Pyannote model licensing** — the very blocker that stalled the Python sidecar. If
   `segmentation-3.0.onnx` / `wespeaker_en_voxceleb_CAM++.onnx` are now license-gated, native
   diarization has no model and Option B collapses back to A. **De-risk first, before writing code.**
2. **Crate-version landmines (bindgen déjà vu)** — `ort` + `ndarray` across MSVC; restore from the
   pre-`0f638ae` Cargo.toml and re-verify on Win11, not just the VM. Reuse the `.bat`-over-SSH
   remote-build pattern from `building-whisper-rs-tauri`.

## 8. PHASE 0 RESULT — models VERIFIED NOT license-locked (2026-10-03)

De-risk PASSED. Native diarization is viable; the Python-sidecar blocker does NOT recur here.

| Model | Source | License | Status |
|---|---|---|---|
| `segmentation-3.0.onnx` | HF `pyannote/segmentation-3.0` | **MIT** (repo gated='auto', consent form) | ✅ downloadable w/ HF token |
| `wespeaker_en_voxceleb_CAM++.onnx` | Google/HF wespeaker | **Apache-2.0** | ✅ free |

- User's HF token saved at `~/.hermes/secrets/hf_token` (mode 600) — unlocks the gated
  segmentation model; verified auth 200 on the README, consent already accepted.
- Both weights verified live via mirror `screenpipe/screenpipe` (renamed from mediar-ai) at
  `crates/screenpipe-audio/models/pyannote/` — segmentation 5.9MB, wespeaker 29MB, HTTP 200.
  ⚠️ screenpipe repo itself is **Screenpipe Commercial License** — copying their CODE (not weights)
  into a commercial product is the landmine; the model *weights* are third-party MIT/Apache and fine.

### ⭐ Major shortcut discovered: `thewh1teagle/pyannote-rs` (MIT, ~130★)
A mature existing **Rust** implementation of exactly this: pyannote segmentation + wespeaker
embeddings on ONNX Runtime, sliding-window audio, cosine-similarity speaker comparison, knf-rs
filterbanks. Could collapse Option B (2-4 weeks of hand-building the ONNX pipeline) into mostly
**integration/vendoring**. Evaluate adopting it before hand-writing pyannote.rs.

### ✅ Open item RESOLVED (2026-10-03): exact ONNX source verified + pyannote-rs Phase-1a eval
**There is NO gated ONNX to unlock** — verified end-to-end in `poc-eval-pyannote-rs/`:
- `pyannote/segmentation-3.0` on HF hosts **pytorch_model.bin only**; every `resolve/<name>.onnx`
  → 404 with token. Same for `pyannote/wespeaker-voxceleb-resnet34-LM`. The HF token is irrelevant
  to the native path (it only unlocks the pytorch sidecar).
- pyannote-rs publishes both .onnx as **public GitHub release artifacts** (v0.1.0):
  `segmentation-3.0.onnx` (5,983,836 B) + `wespeaker_en_voxceleb_CAM++.onnx` (29,292,684 B).
- **Byte-verified tamper-free**: sha256 of both == screenpipe LFS pointers (`b78fc48...ba62e` seg,
  `c46fad1...c54ef` wespeaker). Release == screenpipe mirror == authoritative.

**Foundation verdict: ADOPT pyannote-rs, with one architectural change.**
- ✅ MIT, ~130★, builds clean on rustc 1.99 (needs libssl-dev/pkg-config), ~25x realtime CPU.
  `infinite` example (usize::MAX speakers + `search_speaker(e,0.0)` fallback) is the right base —
  NOT `max_speakers(6)` (under-detects).
- ✅ PoC on the 569s board meeting: 23s, 96 segs, 6 speakers, exit 0. Output saved at
  `poc-eval-pyannote-rs/poc_results/boardmeeting_pyannote-rs_segments.txt`.
- ⚠️ **Streaming nearest-neighbor assignment causes over-fragmentation** (53/73 single-segment
  runs, 26 segs<1s, micro 0.19s turns) + one 43.4s under-split. → §6.1 decision is now settled:
  **global clustering is MANDATORY**. Keep `get_segments()` + `EmbeddingExtractor`, replace
  `EmbeddingManager` with cluster-then-assign, add min-duration + merge (mirror
  `apply_speakers.py:62`).
- Audio must be **16k mono** (resample 48k stereo first).

### Phase 1a RESULT (2026-10-03): auto-K is now the gating risk (was "license risk")
De-risk shift: the **open decision #1 "auto-cluster vs sequential" is decided** — auto-cluster
works only with a KNOWN speaker count. Full writeup: `~/poc-eval-pyannote-rs/PHASE1_FINDINGS.md`.
- Embeddings are accurate (54s interview: force K=2 → clean 2-speaker timeline, sep -0.171).
- Threshold clustering fails (7-8 speakers on a 2-person clip); no stable threshold across clips.
- **THE open problem = estimating K.** Options: silhouette/gap/spectral eigengap, use transcript
  utterance-count to bound K, or port the Python sidecar's tuned threshold.
