# Future Roadmap

A queue of deliberate, well-scoped improvement tasks for the Track fork. Each entry
is written so a future session can pick it up cold (with the `building-whisper-rs-tauri`
skill context) and execute it safely. Work off this file when it's referenced; do not
apply ad-hoc "recommended diffs" without re-validating them against the live source.

---

## Task 1 — OS Keychain Integration for API Keys (MEDIUM severity)

**Why it matters.** API keys (OpenAI, Anthropic/Groq, OpenRouter, Ollama, Custom OpenAI)
are currently stored **plaintext in the SQLite database** (`database/models.rs` `Setting`
struct; written via `database/repositories/setting.rs`). The DB file lives in
`~/.config/Track/` (Linux/macOS) or `%APPDATA%\Track\` (Windows), protected only by
OS file permissions. A backup, a shared-profile machine, or a compromise of the app-data
dir leaks every key. This is the single highest-value privacy hardening on the roadmap.

Note: a prior automated review proposed a `keyring`-crate rewrite, but its code was
**invalid/uncompilable** and it stored empty-string placeholders that would silently
break key retrieval. That plan was rejected. Implement this *fresh*, carefully, from the
spec below — not from that old diff.

### Scope

- Move API-key storage out of the SQLite `settings` table into OS secure storage.
- Keep the DB schema unchanged for backward compatibility; existing keys migrate once.
- Cross-platform backends:
  - macOS — Keychain Access
  - Windows — Credential Manager
  - Linux — Secret Service over DBus (requires an active desktop session)
- Graceful fallback: if the OS secret store is unavailable (headless Linux, no DBus),
  fall back to the existing DB storage rather than crashing or losing the key.

### Design decisions

1. **Crate choice:** `keyring` (cross-platform umbrella, v2.x) with explicit backend
   selection. Do NOT hardcode a single platform crate; gate platform backends behind
   `#[cfg(target_os = ...)]` where needed.
2. **Service/namespace strategy:** use one service name per provider, e.g.
   `Track-<provider>` (and `Track-transcript-<provider>` for the transcript keys,
   which are currently separate columns in `transcript_settings`). Two distinct
   namespaces: summary keys and transcript keys.
3. **Empty-string placeholder trap:** never write `""` to the DB "for compatibility."
   The old rejected plan did this and it silently returned `Some("")` on keychain
   failure when the fallback ran. If keychain fails, the DB must hold the real key or
   the column must be `NULL`.
4. **Read path:** try keychain first; on keychain miss or error, fall back to the DB.
   Distinguish "not present" from "keychain error" so a real failure isn't masked as
   "no key."
5. **Migration:** on startup, once, check a migration flag (e.g. app-version key in the
   settings table); if not done, read each keyed column, write it to keychain, and
   `NULL` the columns. Idempotent — never re-migrate or drop keys on retry.

### Files that will change

- `frontend/src-tauri/Cargo.toml` — add keyring dependency (platform-gated).
- `frontend/src-tauri/src/keychain/mod.rs` (new) — `KeychainManager`.
- `frontend/src-tauri/src/keychain/migration.rs` (new) — one-time migration.
- `frontend/src-tauri/src/lib.rs` — register the `keychain` module; call migration on
  startup.
- `frontend/src-tauri/src/database/repositories/setting.rs` — route `save_api_key`,
  `get_api_key`, `delete_api_key` through the keychain manager with DB fallback; same
  for `save/get/delete_transcript_api_key`.
- `database/models.rs` — unchanged (struct stays for schema compat).

### Non-goals / do-not-do

- Do not encrypt the whole SQLite DB (heavy, breaks `sqlx` tooling, not needed).
- Do not store keys only in keychain with zero fallback — headless/CI/dev builds
  (including this N100 VM where Secret-Service is unavailable) would lose keys.
- Do not change the API surface: `SettingsRepository::save_api_key(pool, provider, key)`
  etc. must keep their signatures. All re-routing happens inside the repository layer.

### Verification

Linux dev box + Windows build box (`oit@10.141.9.147`):
1. `cargo check` / `cargo build` clean on Linux (headless — must exercise the DB
   fallback path, NO keychain).
2. `cargo build` + test on Win11 box in a real desktop session (Credential Manager).
3. Round-trip test: save → get returns same key; delete → get returns `None`.
4. Migration test: pre-seed a DB with plaintext keys, run migration once, confirm keys
   are in credential store and DB columns are `NULL`.

---

## Task 2 — Panic-point cleanup in non-test/production code (LOW-MEDIUM)

**Why.** A codebase scan found real `.unwrap()`/`.expect()` calls in production paths
that can crash the app on unexpected conditions. The prior review's *fix code* was
invalid Rust (e.g. `ok_or_else` on an already-`Result` value; `onboarding.rs:242` was
test-only, not production). Do this precisely, one panic at a time, and compile-verify
each change.

### Verified production panic points (from live source, Oct 2026)

| File | Line | Current code |
|---|---|---|
| `database/manager.rs` | 49 | `app_handle.path().app_data_dir().expect("failed to get app data dir")` |
| `database/manager.rs` | 125 | same `.expect(...)` in `is_first_launch` |
| `database/manager.rs` | 140 | same `.expect(...)` in `import_legacy_database` |
| `lib.rs` | 586 | `.block_on(...).expect("Failed to initialize database")` |
| `lib.rs` | 836 | `.build(generate_context!()).expect("error while building tauri application")` |
| `tray.rs` | 27 | `.icon(app.default_window_icon().unwrap().clone())` |
| `database/setup.rs` | 24 | `.emit("first-launch-detected", ()).expect("Failed to emit ...")` |
| `summary/processor.rs` | 10-14 | `Regex::new(...).unwrap()` inside `Lazy` statics (safe-ish: static regex, but consider `expect` with a message) |

### Rules
- Prefer `?` with `anyhow::Result` on fallible functions that already return `Result`.
- For `tray.rs` icon: use `if let Some(icon) = app.default_window_icon()` or a fallback,
  never invent a fake `Icon::Raw(...)` constructor (the rejected diff did).
- For non-critical emit (setup.rs:24): downgrade to `log::warn!` — a failed event emit
  should not crash startup.
- Leave `onboarding.rs:242` and all `*_test*`/`#[cfg(test)]` unwraps alone — test code
  panicking on unexpected data is intended.
- Compile-verify after each file: `cargo check` in `frontend/src-tauri`.

---

## Task 3 — README PRO-diarization line (DOCS, trivial)

**Flagged in HANDOFF.md §7 (open end):** `README.md` (L47, 226, 234) still markets
speaker diarization as *"planned for mid-June / Coming Soon"* in the **PRO** section,
but per-person speaker labels now work in Community via the sidecar pipeline
(`update_transcript_speaker`, `speaker` field plumbing, rename UI all proven on the
Win11 build). **This is a product/marketing decision** — update the copy if
"Community now includes diarization" is the intended position. Confirm with the
maintainer before committing.

---

*Written 2026-10-01. Source of truth: live repo at commit `7841b01`.*
