# HANDOFF.md — Meeting-Helper Fork: Win11 Build Proven (Whisper-rs 0.16)

> Resume file. Read this to pick up the project without reloading the conversation.

**Last updated: 2026-10-01** — Whisper-rs bump 0.13.2 → 0.16.0; fork now FULLY builds on Win11: Node 22+pnpm installed, `tauri build --no-bundle` produces a self-contained `meetily.exe` (embedded frontend, no localhost dependency) that RUNS (verified alive + responding, no :3118 listener). Fixed the fork's staged speaker type-error (`null`→`undefined`, commit 32dc563). Winget fixed via `--accept-source-agreements`. Build machine is now WIN11, not this VM. Fresh session: read whole file, jump to §8/SIGNING for open ends.

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
