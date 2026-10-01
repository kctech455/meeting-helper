# HANDOFF.md — Meeting-Helper Fork: Win11 Build Proven (Whisper-rs 0.16)

> Resume file. Read this to pick up the project without reloading the conversation.

**Last updated: 2026-10-01** — Whisper-rs bump 0.13.2 → 0.16.0, fork compiles on Win11 (debug + release exit 0), release `meetily.exe` verified, Cargo.lock synced, docs audited. Fresh session: read the whole file, then jump to §9 for the open ends.

---

## 1. ONE-LINE STATUS

The staged fork (`kctech455/meeting-helper`) — specifically the `update_transcript_speaker` Tauri command, per-person `speaker` field plumbing, and rename UI — **compiles and builds on the Win11 box with a real Rust toolchain**. This was the piece the original handoff flagged as *"unverifiable without a Rust toolchain."* The hard blocker (bindgen opaque `whisper_full_params` on MSVC) is solved, not worked around. Release binary exists and is verified.

---

## 2. WHAT THIS SESSION PROVED

| Claim | Status |
|---|---|
| `update_transcript_speaker` + `speaker` plumbing + rename UI compile | ✅ **Proven** — debug `Finished dev profile in 1m 00s` exit 0 |
| Release build | ✅ **Proven** — `Finished release profile in 5m 18s` exit 0 |
| Release artifact | ✅ **Verified** — `meetily.exe` 42.4 MB in workspace `target\release` (01:41 same day) |
| Docs / Cargo.lock consistent with build | ✅ **Audited** — lock synced, one marketing line flagged (see §7) |

---

## 3. PROJECT LOCATIONS

```
/home/kc/myApps/meeting-helper/                  # FORK ROOT (kctech455/meeting-helper, GitHub public)
  frontend/src-tauri/          # Rust/Tauri core
    Cargo.toml                 # whisper-rs pinned 0.16.0 (macos/win/linux)
    src/whisper_engine/whisper_engine.rs   # migrated to 0.16 API
    src/database/repositories/meeting.rs   # speaker: t.speaker
    src/database/models.rs                 # Transcript.speaker: Option<String>
    src/audio/common.rs        # VAD TranscriptSegment speaker: None + merge comment
    src/audio/import.rs        # test initializers got speaker: None
    src/api/api.rs             # MeetingTranscript/TranscriptSegment already had speaker
  Cargo.lock                   # SYNCED to whisper-rs 0.16.0 / sys 0.15.0 (commit 508f9fe)
  HANDOFF.md                   # THIS FILE

/home/kc/myApps/meeting-diarization-poc/       # POC project root (sidecar, original HANDOFF here)
  src/diarize.py, watchdog.py, settings.py     # WhisperX + pyannote sidecar pipeline
```

Win11 box: `oit@10.141.9.147`, clone `C:\Users\OIT\meeting-helper-build` (shallow), SSH key `~/.ssh/win11_diar`.

---

## 4. THE CORE FIX (root cause chain)

1. **Fork pinned whisper-rs 0.13.2** → its sys crate (0.11.1) vendored whisper.cpp 1.7.1 with an opaque-params API. Bumped to **0.16.0** (sys **0.15.0**, whisper.cpp 1.8.3, bindgen 0.72) — commits `9fcdb36` → `3398a9f`.
2. **bindgen opaque `whisper_full_params`** — bindgen 0.71 emitted `pub struct … { _address: u8 }` (size-1 stub) while asserting real size 296 → `size_of - 296` overflow panic. **bindgen 0.72 (via the bump) generates the correct full struct.** This was the multi-round blocker; it is NOT a missing `#define` in whisper.h (struct is fully inline at ~487, `clang -fsyntax-only` parses clean).
3. **`WHISPER_DONT_GENERATE_BINDINGS` trap** — older sys crates ship **Linux-generated** bindings with glibc types (`_G_fpos_t`, `_IO_FILE`) that don't exist on MSVC. Regenerating via libclang is the only valid path; the shipped-bindings escape hatch is unusable on Windows.
4. **whisper-rs 0.16 API renames** — fixed in fork source: `set_suppress_non_speech_tokens`→`set_suppress_nst`, `full_get_segment_text_lossy(i)`→`get_segment(i).to_str_lossy()`, `full_get_segment_t0/t1`→`segment.start_timestamp()/end_timestamp()`, `full_n_segments()` no longer `?`-able (`c_int`), `new_with_params` needs `.as_ref()` (Cow→Path).
5. **Missing `speaker` in 3 initializers** (meeting.rs, common.rs, import.rs ×2) — staged change completed. `api.rs` was already correct.

---

## 5. WIN11 ENVIRONMENT (winget is broken → all direct downloads)

Installed: rust toolchain (cargo at `C:\Users\OIT\.cargo\bin`, rustfmt via rustup), LLVM 23.1.2 (`C:\Users\OIT\LLVM\LLVM`, **libclang.dll** for bindgen), CMake from VS BuildTools (`C:\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin`), MSVC toolchain.

Build .bat PATH additions (must be in every build script):
- `C:\Users\OIT\.cargo\bin`
- `C:\BuildTools\...\CMake\CMake\bin`
- `C:\Program Files\Git\cmd` (needed for whisper.cpp CMake `GIT_EXE`)
- `LIBCLANG_PATH=C:\Users\OIT\LLVM\LLVM\bin` — bindgen needs **libclang.dll**, not clang.dll; set inside the .bat.

**Remote-work pattern (proven, use it):** never inline complex `cmd /c "..."` over SSH (local bash mangles `%PATH%`/`$`/`%%` and breaks LIBCLANG_PATH → bindgen `invalid: []`). Write a `.bat` locally, `scp` it to the box, execute by path. Also: `tail` is NOT on cmd's PATH — no `| tail` in remote .bat (exit 255). Release artifacts land in **workspace-level** `C:\Users\OIT\meeting-helper-build\target\release\`, NOT the crate subdir.

---

## 6. COMMITS ON origin/main (meeting-helper)

```
7a80e03  Add speaker field to TranscriptSegment (Rust+TS)         [staged, pre-session]
fae6750  Wire per-person speaker label: DB, repo, command, rename UI  [staged, pre-session]
9fcdb36  Bump whisper-rs 0.13.2 -> 0.15.1
3398a9f  whisper-rs 0.15.1 -> 0.16.0  (pair sys 0.15.0 — the bindgen fix)
3302819  Fix whisper-rs 0.16 API renames + speaker initializers
508f9fe  Sync Cargo.lock to whisper-rs 0.16.0 / sys 0.15.0
```

---

## 7. DOCS AUDIT (2026-10-01)

- ✅ **Cargo.lock** — was stale (0.13.2/0.11.1 vs Cargo.toml 0.16.0); **synced** from the built/verified resolution, commit `508f9fe`.
- ✅ `docs/BUILDING.md`, `docs/GPU_ACCELERATION.md`, `docs/building_in_linux.md` — no version/step affected by the bump; feature flags (`raw-api`, `cuda`) unchanged.
- ✅ `frontend/API.md` — high-level architecture doc, no per-command reference to stale.
- ✅ `frontend/README.md` L10 "Speaker diarization support" — accurate, matches implemented capability.
- ✅ `CLAUDE.md` — "speaker" refs are audio output devices (speakers.rs), unrelated.
- ⚠️ **FLAGGED, NOT EDITED:** `README.md` (L47, 226, 234) still markets speaker diarization as *"planned for mid-June / Coming Soon"* in the **PRO** section. Per-person labels now work in Community via the sidecar pipeline. **Product/marketing decision** — update if Community-now-includes-diarization is the intended position.

---

## 8. NEXT STEPS (open ends)

1. **`tauri build` (installer bundle)** — Rust side is fully proven, but bundling needs Node/npm (`tauri-cli`) on the box. Not yet attempted.
2. **README PRO-diarization line** — decide & update (see §7 ⚠️).
3. **GPU path** — built CPU/AVX2. Whisper-rs `cuda`/Vulkan features exist behind flags; not exercised on this fork build (the POC sidecar already uses GPU WhisperX on the RTX box).
4. **Push the sidecar handoff wiring** if not already in the fork — verify `apply_speakers.py` bridge is fully integrated into the Tauri app (it was verified standalone earlier; the `speaker: None` at VAD-stage confirms merge happens later).

---

## 9. QUICK RECOVERY COMMANDS

```bash
# SSH to box (BatchMode, quick timeout)
ssh -o BatchMode=yes -o ConnectTimeout=20 -i ~/.ssh/win11_diar oit@10.141.9.147

# Pull latest on box
"C:\Program Files\Git\cmd\git.exe" pull origin main   # in meeting-helper-build

# Rebuild debug (has LIBCLANG_PATH + PATH baked in)
cmd /c "C:\Users\OIT\rebuild_fixes.bat"               # or build_release.bat

# Verify release exe
for %f in ("C:\Users\OIT\meeting-helper-build\target\release\meetily.exe") do @echo %~zf
```

Working .bat scripts are at `C:\Users\OIT\` on the box and `/home/kc/.hermes/cache/scratch/` locally (`rebuild_fixes.bat`, `build_release.bat`, `verify_release_exe.bat`, `pull_build_016.bat`, etc.).

---

## 10. SKILL (procedural memory saved)

`software-development/building-whisper-rs-tauri` — captures: bindgen opaque-struct failure + fix (0.71→0.72), glibc-poisoned shipped-bindings trap, LIBCLANG_PATH/libclang.dll, the .bat-over-SSH remote-build pattern, workspace-level target dir, and the full whisper-rs 0.13→0.16 API map. Load it before any future whisper-rs Windows build.
