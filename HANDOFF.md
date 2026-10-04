# HANDOFF.md — Meeting-Helper Fork: Win11 Build Proven (Whisper-rs 0.16)

> Resume file. Read this to pick up the project without reloading the conversation.

**Last updated: 2026-10-03** — Phase 2/3 DONE: native diarization pipeline SHIPPED in app
(commit `aeecd62`) — `src-tauri/src/diarization/` + `apply_speaker_merge` +
`useRecordingStop.ts` trigger. meetily.exe 42.8MB BUILT + runtime-verified on Win11.
Prior (2026-10-03): §11 Phase 1b K-estimator VALIDATED (interview K=2 exact via
silhouette, board ~6→7) + 3.1 config RESOLVED (`config.yaml`, AgglomerativeClustering HAC,
no autotune/spectral → meetily self-estimates K). Build machine is WIN11 (MSVC 14.44 IS
installed, contrary to an older stale note). Fresh session: read whole file, jump to
§11/Phase 2-3 → "Open ends after the shipped integration" for the next-work list.

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
32dc563  Fix speaker type error in optimistic rename (null -> undefined)  [2026-10-01, NEW]
```

## 6B. WIN11 IS NOW THE BUILD MACHINE (2026-10-01 — verified)

The fork now **fully builds and packages on the Win11 box** — no more Proxmox VM builds. What changed:
- **Node 22.11.0 installed** at `C:\Users\OIT\node22\node-v22.11.0-win-x64` (winget was broken then → direct zip). `npm`/`pnpm` in that dir.
- **pnpm 12.8.1** installed globally (via npm, prefix = node22 dir). `pnpm install` → 643 deps, lockfile supply-chain verified ✓
- **Winget FIXED** on the box: it crashed with `0xC0000005` on install before. Root cause was the source-agreement EULA. `winget search notepad --accept-source-agreements` works, and a real install (`winget install Notepad++.Notepad++`) succeeds. Use `--accept-source-agreements --accept-package-agreements` on all winget commands.
- **`tauri build --no-bundle`** works end-to-end on the box → self-contained `meetily.exe` (44.4 MB at `C:\Users\OIT\meeting-helper-build\target\release\meetily.exe`). No localhost/server dependency.

### THE WORKING BUILD PIPELINE ON WIN11 (proven, use this)
Reusable `.bat` scripts at `C:\Users\OIT\` (also mirrored in `/home/kc/.hermes/cache/scratch/`):
1. `mh_frontend_install.bat` — sets node on PATH, runs `pnpm install` (only needed once / on dep changes)
2. `mh_frontend_build.bat` — `pnpm build` (Next.js → `out/`)
3. `mh_tauri_build.bat` — calls `vcvars64.bat` + sets cargo/git/LLVM/node PATH + `LIBCLANG_PATH`, then `pnpm tauri build --no-bundle`

**Critical gotchas** (learned this session, do not regress):
- **Kill the running meetily first** — the exe locks `target\release\meetily.exe`; `tauri build` fails with `os error 32` (file in use) if the app is running. `taskkill /IM meetily.exe /F` before rebuilding.
- The `--no-bundle` flag sidesteps signing-key requirements (see §8 SIGNING). For a real installer, drop it and provide signing keys.
- cmd PATH chaining with `set X=...& call %X%` does NOT expand inline — write `.bat` files locally, scp to box, execute by path (the proven remote-work pattern from §5).
- MSVC is at `C:\BuildTools\...` (vcvars64.bat at `C:\BuildTools\VC\Auxiliary\Build\vcvars64.bat`), NOT `C:\Program Files (x86)`. LLVM at `C:\Users\OIT\LLVM\LLVM`.

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

1. **SIGNING KEYS (for installer/updater)** — the full `tauri build` (NSIS/MSI installer + updater artifacts) needs `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (config has `createUpdaterArtifacts: true` + Windows `signCommand`). We used `--no-bundle` to sidestep this for the test exe. **TODO: obtain a signing key** (see §7 ⚠️ note appended below) before producing a distributable installer.
2. **README PRO-diarization line** — decide & update (see §7 ⚠️).
3. **GPU path** — built CPU/AVX2. Whisper-rs `cuda`/Vulkan features exist behind flags; not exercised on this fork build (the POC sidecar already uses GPU WhisperX on the RTX box).
4. **Push the sidecar handoff wiring** if not already in the fork — verify `apply_speakers.py` bridge is fully integrated into the Tauri app (it was verified standalone earlier; the `speaker: None` at VAD-stage confirms merge happens later).

### SIGNING KEYS — how to get started (for when you want a real installer)
- The build already verifies `TAURI_SIGNING_PRIVATE_KEY`/`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` via `build.ps1` (loads from `.env`, expects key in `.tauri\meetily.key`).
- **Two separate keys exist in Tauri:** (a) **code-signing cert** for Windows (SignTool / `signCommand`, needs a real cert from a CA like DigiCert) and (b) the **updater signing keypair** (generated by `tauri signer generate`, public key goes in `tauri.conf.json` `plugins.updater.pubkey` — currently a placeholder).
- For a locally distributed installer you can skip the Windows CA cert and just generate the updater keypair: `pnpm tauri signer generate`. For broader distribution you'll need a real code-signing certificate.
- The `pubkey` in `tauri.conf.json` (L115) is the minisign placeholder from upstream — replace it with your own keypair's public key when you obtain one.

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

---

## 11. NEXT WORK: NATIVE RUST DIARIZATION (Option B) — pick up here

**Session goal:** make meetily.exe auto-label speakers post-meeting, fully native (no Python).
Trigger point CONFIRMED: frontend `useRecordingStop.ts` after `storageService.saveMeeting()` returns
`meetingId` (L256-262) — that's when meeting + transcripts.json + audio.mp4 all exist. Fire
`start_diarization({audio_path, meeting_id})` there.

**Complete design:** `/home/kc/myApps/meeting-helper/SCOPE-native-diarization.md`
(read it first — §8 is the Phase-0 result; §6 has open decisions).

### Phase 0 DE-RISK — PASSED (2026-10-03). Summary:
- Models NOT license-locked. `pyannote/segmentation-3.0` on HF = **MIT**, gated='auto'
  (consent form, no fee) — user's HF token unlocks it. `wespeaker` embedding = **Apache-2.0**.
- **User HF token saved at `~/.hermes/secrets/hf_token` (mode 600, never echo in chat).**
  Verified auth 200 on segmentation README.
- Weights also live via screenpipe mirror (seg 5.9MB, wespeaker 29MB, HTTP 200).
- ⚠️ screenpipe repo = **commercial license (Screenpipe Commercial)** — do NOT copy their CODE.
  Weights are third-party MIT/Apache, OK to use. **User explicitly wants zero commercial code.**

### ⭐ Shortcut to evaluate FIRST: `thewh1teagle/pyannote-rs` (MIT, ~130★)
Existing mature Rust pyannote diarization: segmentation-3.0 + wespeaker embeddings on ONNX Runtime,
sliding-window, knf-rs filterbanks, cosine-similarity speaker compare. **Evaluate adopting this
as the foundation** before hand-writing the pipeline. Fill gaps using the Python sidecar logic
(merge overlap from `apply_speakers.py:62` is the proven shape).

### Test video (user-provided, verified download-able via yt-dlp)
**Board Meeting Example** — https://youtu.be/WBXJEJCsULw?si=joNIvJ2KNwgdDPhX
  - dur=569s (~9.5min) | uploader "Last Minute Meetings" | multi-speaker board meeting
  - Longer than the 54s interview (21/21 segments, 100% accurate); good training/test clip.

### ⭐ Phase 1a EVALUATION: pyannote-rs FOUNDATION — PASSED with caveats (2026-10-03)
**Verdict: adopt as foundation** — MIT, ~130★, builds clean, models verified, 569s PoC works.
Full eval in `poc-eval-pyannote-rs/` (scratch VM dir) + writeup below.

**Model-source question RESOLVED — there is NO gated ONNX to unlock:**
- `pyannote/segmentation-3.0` on HF = **pytorch_model.bin only** (resolve segmentation-3.0.onnx → 404
  even with token). Same for `pyannote/wespeaker-voxceleb-resnet34-LM`.
- pyannote-rs ships both .onnx as **public GitHub release artifacts** (v0.1.0):
  `segmentation-3.0.onnx` 5,983,836 B + `wespeaker_en_voxceleb_CAM++.onnx` 29,292,684 B.
- **Byte-verified**: both sha256 == screenpipe LFS pointers (seg `b78fc48...ba62e`,
  wespeaker `c46fad1...c54ef`). Release = screenpipe = authoritative, tamper-free.
- HF token NOT needed for native path (that's the python-sidecar pytorch path only).

**PoC on 569s board meeting** (WBXJEJCsULw, 16k mono resample): ran **~23s (~25x realtime, CPU)**,
exit 0, 96 segments, 6 distinct speakers. Verified in `target/release/examples/infinite` (max_speakers=6
example under-detects; `infinite` uses usize::MAX + search_speaker(0.0) fallback → the right pattern).

**⚠️ CAVEATS (why global clustering is MANDATORY, not optional):**
1. Assignment is **streaming nearest-neighbor** (cosine, threshold 0.5), NOT global clustering.
   → over-fragmentation: 53/73 runs are single-segment, avg run len 1.3 segs.
2. Micro-segments = noise: 26 segs <1s; some 0.19s/0.21s "turns" (impossible).
3. No min-duration merge, no same-speaker-adjacent merge, no max-consecutive fix.
4. Long under-split: one 43.4s run (433.72→477.17) and 26.7s/25.9s runs — single-speaker span too long.
Result: S2 owns 208s/569s (37%), plausible, but granularity unusable for per-turn labels as-is.
**Fix (Phase 1):** keep segmentation + embedding from pyannote-rs, REPLACE EmbeddingManager with global
clustering (embed all segs → agglomerative/spectral cluster → assign labels), add min-duration + merge
logic mirroring `apply_speakers.py:62`.

### Phase 1 build env (VM, for dev PoC)
- **rustup installed on VM** (`~/.cargo/bin`, rustc 1.99.0). Needs `libssl-dev + pkg-config` (openssl-sys).
- Build: `cargo build --release` (ort 2.0.0-rc.10 pulls onnxruntime prebuilt). ~1m07s.
- Models must sit where examples look (crate root, relative paths). Audio must be **16k mono** (pyannote
  standard; 48k stereo → ffmpeg resample/downmix first, boardmeeting_16k_mono.wav 18.2MB 569s).
- PoC results: `poc-eval-pyannote-rs/poc_results/boardmeeting_pyannote-rs_segments.txt` (96 segs).

### Open item before coding Phase 1
Gated `pyannote/segmentation-3.0` sibling list shows **no .onnx** — the actual ONNX likely lives in
a separate gated repo or the screenpipe LFS mirror. Verify exact gated onnx source URL (HF resolve
with token vs screenpipe LFS) **→ DONE: models = pyannote-rs GH release (== screenpipe LFS), public, no
token needed.**

### ⭐ Phase 1 RESULT — clustering bottleneck identified (2026-10-03, full writeup in `~/poc-eval-pyannote-rs/PHASE1_FINDINGS.md`)
Pipeline proven correct in structure, and **embeddings proven accurate — but auto-K (speaker count) is the open problem.**

**What works:** `get_segments()` + `EmbeddingExtractor` + drop<1s turns + K-means = viable on-device diarization (~25x realtime). Board meeting (569s): 6 speakers, 67 segs, 455s, 20.8s. Models verified public, no token.

**DECISIVE validation on the 54s interview (known 21/21 2-speaker truth):**
- Streaming baseline → 8 speakers (WRONG). Any threshold clustering → 7-8 (WRONG).
- **Force K=2** → clean 2-person alternating timeline, inter-cluster cosine **-0.171** → embeddings PERFECTLY separate the 2 people.
- ⇒ The hard part = **auto-estimating K.** No cosine threshold is stable across clips (0.5→6, 0.55→8, 0.6→12 on board meeting). Biggest-jump/gap heuristics also fail (suggested K=10).

**Next (auto-K):** silhouette/gap-statistic/spectral-eigengap, or use the transcript (meeting with N utterances) to bound K, or port the Python sidecar's optimized threshold. Interview embeddings (12 rows, 2 people) already dumped at `~/poc-eval-pyannote-rs/probe_embeddings.tsv` for testing K-estimators against known truth.

### ⭐ Phase 1b DECISION — K-estimator design LOCKED (2026-10-03, fresh session start point)

**Correction that reframes the problem:** our sidecar (`diarize.py:64-66`) has **NO custom threshold** — it passes
`num_speakers/min_speakers/max_speakers` straight to `pyannote/speaker-diarization-3.1`, so the "optimized threshold"
we thought we had is a *trained* hyperparameter inside pyannote, not our code. The solved answer = pyannote's clustering
(`src/pyannote/audio/pipelines/clustering.py`). Key lessons from their source the phase-1 threshold run was MISSING:

1. HAC cut at threshold → split clusters into **large** (≥ `min_cluster_size`, ~0.1×N) vs **small**.
2. **Re-assign every small cluster to the nearest large cluster by centroid** — this is what collapsed our spurious K.
3. **Autotune the threshold per file** (`use_autotune=True`): a fixed value is provably unstable (our 0.5→6/0.55→8/0.6→12).
4. Spectral path (Quan Wang / pyannote PR #995) refines affinity first: `CropDiagonal → GaussianBlur → RowWiseThreshold → Symmetrize`,
   then Laplacian + **eigengap** (Ratio/NormalizedDiff) for K — but is **poor on short embedding sequences → `spectral_min_embeddings=5` fallback**.

**LOCKED DESIGN (build this next session):** emulate pyannote HAC, per file:
1. embed turns → **drop <1s** (proven) → cosine affinity → Ward/centroid linkage.
2. **min_cluster_size prune → absorb orphan clusters into nearest large centroid** (the missing trick).
3. K = argmax over **gap-statistic** (primary) + **silhouette** (cross-check) on the surviving large clusters,
   **capped by transcript-distinct-utterance bound and an absolute max (~10)**.
4. Spectral **eigengap** only as a *fast-path oracle* when the leading gap is decisive AND `n_emb ≥ 5`; else fall back to (3).

**Validation targets (ground truth):** `~/poc-eval-pyannote-rs/probe_embeddings.tsv` → must return **K=2**;
569s board meeting → expect **~6**. `k_estimator` should be a new Rust example in `poc-eval-pyannote-rs/`.

**✅ Open item RESOLVED (2026-10-03):** fetched the live 3.1 config via curl with the HF
token. The file is **`config.yaml`**, NOT `pyannote_config.yml`. It declares
`clustering: AgglomerativeClustering` (HAC — NOT the library's VBx default) with
`{method: centroid, min_cluster_size: 12, threshold: 0.7045654963945799}`, and **no
`use_autotune`**. The 3.1 `AgglomerativeClustering.cluster()` (MIT, src/pyannote/audio/
pipelines/clustering.py) does: unit-normalize → linkage(centroid,euclidean) → fcluster →
min_cluster_size=min(12, round(0.1N)) → absorb small into nearest large centroid → renumber.
**autotune/spectral-eigengap are NOT in the shipped runtime path** — they're legacy trainer
machinery, not how 3.1 picks K. Copy of sources in `~/.hermes/cache/scratch/{clustering.py,
speaker_diarization.py, diarization_utils.py}`.

**Also (CORRECTED 2026-10-03):** Win11 box = Xeon E3-1270 v6 4c/8t, 32GB. The earlier
"reinstall WIPED clang/LLVM + MSVC — not buildable" note is now **outdated**: MSVC 14.44
toolset IS installed (cl.exe at `C:\Program Files (x86)\Microsoft Visual Studio\2022\
BuildTools\VC\Tools\MSVC\14.44.35207`), rustup stable-msvc + target present, and the
meetily.exe **release build succeeded on oit** (cargo auto-detects VS via vswhere). Keep
light dev builds on this VM; full release builds on oit.

### Tooling note
yt-dlp (2026.8.19) freshly installed via `~/.hermes/tools/uv-0.12.3-linux-x64/uv tool install yt-dlp`
→ lands at `~/.local/bin/yt-dlp` (may need `export PATH=$HOME/.local/bin:$PATH`). ffmpeg at
`~/.hermes/tools/ffmpeg-9.0.1-linux-x64`.

### ✅ Phase 1b DONE — k_estimator VALIDATED (2026-10-03)
`examples/k_estimator.rs` built + run against both ground-truth targets (full writeup in
`~/poc-eval-pyannote-rs/PHASE1_FINDINGS.md` §7):

| dataset | n_emb | truth | K_ESTIMATED | source |
|---|---|---|---|---|
| `probe_interview.tsv` (54s interview) | 12 | **2** | **2** | silhouette decisive peak (k=2:0.407 vs 0.295) |
| `probe_board_meeting.tsv` (569s board) | 70 | ~6 | 7 | silhouette plateau 6-7 (0.373/0.394) |

**What the live 3.1 source proved (corrects our earlier lock):**
- pyannote file is **`config.yaml`**, not `pyannote_config.yml`; clustering is
  **AgglomerativeClustering** (HAC centroid, thresh 0.7046, min_cluster_size 12), NOT the
  library's VBx default.
- 3.1's runtime has **NO autotune / no spectral-eigengap K-selection** — those are legacy
  `pyannote.pipeline` trainer features. The runtime only: cut at the fixed threshold →
  prune + absorb, then respects user-supplied num_speakers bounds.
- ⇒ meetily MUST estimate K itself. **Silhouette is the reliable estimator** here
  (nails interview, 6-7 on board). **Gap-statistic is degenerate with n<<dim** (monotone/
  boundary-biased). **Spectral eigengap reports ~no separation** on cosine-normalized
  wespeaker embeds without pyannote's affinity-refinement port.

**✅ Phase 2/3 DONE — native pipeline SHIPPED in app (commit `aeecd62`, 2026-10-03):**
The full pipeline is now wired INTO meetily.exe (no Python sidecar):
`src-tauri/src/diarization/{models,pipeline,mod}.rs` — downloads both ONNX models
(pyannote-rs v0.1.0 release artifacts, **no HF token**; verified 200: seg 5.98MB /
wespeaker 29.3MB, cached in app_data_dir/models/diarization), segment→drop<1s→
silhouette K→greedy-centroid-HAC-to-K→renumber→chronological segments. `apply_speaker_merge`
in transcript.rs; frontend trigger in `useRecordingStop.ts` (fires start_diarization with
folderPath/audio.mp4 + meetingId after saveMeeting). pyannote-rs pinned **0.3.4**
`features=["load-dynamic"]`; errors are eyre (wrap `.map_err(|e| anyhow!(...))`).
meetily.exe 42.8MB BUILT on Win11 (verified running 120s+; workspace-root target\release).

**Open ends after the shipped integration (next session):**
1. Board-meeting K tightening (optional): port pyannote's affinity refinement
   (CropDiagonal/GaussianBlur/RowWiseThreshold/Symmetrize) for the spectral path — K=7 vs
   truth ~6 is acceptable otherwise.
2. Transcript-bound K (meeting with N utterances) is still a heuristic — wire real
   utterance count if you want it tighter than `round(sqrt(n))+2, cap 10`.
3. **Runtime E2E on Win11 (NOT yet done):** record a real 2-person clip in the app, confirm
   transcripts.speaker gets populated after save; watch first-run model download (~35MB).
4. cargo tauri build (NSIS installer) needs `tauri-cli` installed first (`cargo install
   tauri-cli`) — plain `cargo build` already yields the runnable exe.

### UI speaker handling (already built — no new UI work for display + manual rename)
- Display: speaker field → blue `● SPEAKER_nn` chip (`VirtualizedTranscriptView.tsx:128`).
- Manual rename: click chip → inline input → "John"/"Sue" → Enter → persists
  (UI:VTV:159 → hook:usePaginatedTranscripts.ts:207 → cmd:api.rs:1015 → SQL:transcript.rs:109).
- **GAP (new, ~1-2d):** batch rename "rename SPEAKER_00 everywhere" — no `api_rename_speaker(meeting_id, from, to)` yet.
