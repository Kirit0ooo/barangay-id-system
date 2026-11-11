# CODEBASE_AUDIT.md — Barangay ID Registration System

**Project:** /mnt/d/Barangay/barangay-id-system
**Assessment date:** 2026-11-11
**Completed by:** Pi coding agent
**Specification:** CODEBASE_REVIEW.md (full workflow: inventory → audit → repair → validate → reassess → report)
**Status:** COMPLETE — one real defect found and fixed; all acceptance criteria now met.

---

## 1. Initial inspection (baseline)

This is an offline-first Tauri v1 desktop app for barangay ID registration (Windows-primary; WSL/Linux CI only).

**Data flow:** Alpine SPA (`frontend/src/js/app/app.core.js` → view modules → `app.js`) → `frontend/src/js/services/api.js` (Tauri invoke wrappers) → Tauri commands (`backend/src/src/*.rs`) → sqlx SQLite (`data/residents.db`) → JSON response back to frontend. Photos stored base64→JPEG under `data/photos/` (max 6 MB binary). Bulk ID printing is HTML-only via `frontend/src/card-template.html` + hidden iframe (no PDF generation). Data isolated to project tree: `data/residents.db`, `data/photos/`, `data/backups/`, `config/civic.json`, `Documents/Excel/`, `logs/`, `tmp/`. Never `%APPDATA%/%TEMP%/AppData`.

**Backend commands** (registered in `main.rs`): `generate_next_id`, `search_residents`, `save_resident`, `update_resident`, `delete_resident`, `get_qr_data_url`, `get_cards_batch`, `backup_database`, `restore_database`, `export_excel`, `import_excel`, `optimize_database_cmd`, `encryption_generate_key`, `encryption_status`, `encryption_encrypt_database`, `encryption_decrypt_database`, `encryption_encrypt_photos`, `encryption_decrypt_photos`, `log_frontend`, `download_frontend_logs`.

**Frontend modules:** `app.core.js` (shared state + core methods), `residents/app.register.js`, `residents/app.search.js`, `residents/app.detail.js`, `settings/app.settings.js`, `dashboard/app.dashboard.js`, `app.js` (entry point), `shared/helpers.js`, `services/api.js`, `services/logger.js`, `bootstrap/verbose.js`. CSS: `style.css` + prebuilt `tailwind.css` (freshness-guarded by `tools/check-css.mjs`).

**Launchers:** `build.ps1`/`build.bat` (release, NSIS-only), `tools/launcher-common.ps1` (shared env pins). Dev launchers `run.ps1`/`run.bat` removed (2026-11-10).

**Git baseline:** master, HEAD e6985f8 ("Merge branch 'fix/card-preview-design-system'"), clean working tree, 1 commit ahead of origin/main. No stashes.

---

## 2. Audit findings

### 2.1 Baseline validation (pre-repair)

| Command | Result |
|---------|--------|
| `npm test` | 59 pass / **1 fail** / 13 suites — RED (regression.test.js) |
| `npm run check:css` | CSS fresh |
| `npm audit --audit-level=moderate` | 0 vulnerabilities |
| `cargo fmt --all -- --check` | clean (exit 0, 11 files) |
| `cargo metadata --locked` | exit 0 |
| `cargo check`/`test`/`clippy`/`doc` | NOT RUN on this host — blocked (no `libsoup-2.4`, no MSVC `lib.exe`); CI-only |

### 2.2 Findings by severity

**CRITICAL — none.**

**HIGH — none.**

**MEDIUM — 1 finding (FIXED):**

| # | ID | File(s) | Finding | Fix |
|---|----|---------|---------|-----|
| M1 | LAUNCHER-NONASCII | `build.ps1` | `build.ps1` contained two non-ASCII characters in comments: `·` (U+00B7 middle dot) and `—` (U+2014 em dash). The regression test `all launchers ASCII-only (PS 5.1 codepage-safe)` requires every char in `build.ps1` and `build.bat` to be < 0x80, because non-ASCII broke PowerShell 5.1 parsing once. | Replaced both with ASCII hyphens (`-`) in `build.ps1:126-127`. Verified zero non-ASCII remain. |

**LOW — none.**
**INFORMATIONAL — none.**

### 2.3 Investigated but NOT defects (files do not exist in this tree)

During triage, several suspected surfaces were checked and confirmed absent — no action needed:

- `tools/scan-scripts.sh` / `tools/check_ocpp_adapter.sh` — not present (`tools/` holds only `check-css.mjs`, `css-fingerprint.txt`, `launcher-common.ps1`, `tokens.html`). No OCPP adapter scripts exist.
- `frontend/src/js/core/index.js` — not present; frontend modules live under `app/` per AGENTS.md. No empty auth stub.
- `frontend/src/js/bridge/` — not present; no bridge UI surface.
- `config.rs` references to `verify_base_url` / `VerifyTrust` — not present; config already carries `emergency_defaults` and `photo_gc_dry_run` consistently with `config/civic.json`.

### 2.4 Data confinement

All data, caches, logs, temp files, and build output remain inside the project root. `.gitignore` excludes `node_modules/`, `.npm-cache/`, `backend/src/target/`, `tmp/`, `logs/*.log`, `data/*`, `.crypto-key/`, `.pi/`. No data written to `/tmp`, `~/`, or AppData. — PASS

---

## 3. Repair executed

**File changed:** `build.ps1` (2 character substitutions, comments only — no logic change)

- `build.ps1:126`: `' · Identifier: ...' — the` → `'- Identifier: ...' - the`
- `build.ps1:127`: `followed by ' ·', not '**'` → `followed by ' -', not '**'`

No data deleted, no config overwritten, no dependencies added, no speculative features.

---

## 4. Validation results (post-repair)

| Command | Result |
|---------|--------|
| `npm test` | **60 pass / 0 fail / 13 suites** — GREEN (regression check now passes) |
| `npm run check:css` | CSS fresh |
| `npm audit --audit-level=moderate` | 0 vulnerabilities |
| `grep -nP '[^\x00-\x7F]' build.ps1 build.bat` | no matches — launchers are ASCII-only |
| `cargo fmt --check` | clean (unchanged; build.ps1/build.bat not Rust) |
| `cargo metadata --locked` | exit 0 (unchanged) |

---

## 5. Post-repair audit

Fresh whole-project review complete after the fix:
- No regressions: all 60 tests pass (was 59/1 pre-fix).
- CSS fresh; npm audit 0.
- The single MEDIUM finding (launcher non-ASCII) is resolved.
- No new findings introduced by the one-character comment edit.
- Acceptance criteria in CODEBASE_REVIEW.md all met or explicitly blocked by documented host limits (Rust compile is CI-only).

---

## 6. Remaining risks / known boundaries (documented in AGENTS.md, not defects)

1. Rust `cargo check/test/clippy/doc` cannot run on this host — CI is the compiler gate. Backend changes are statically reviewed only here.
2. Pre-existing plaintext backups (created before the encryption switch) remain readable.
3. Photos encrypted individually; new photos added while DB is unencrypted remain plaintext until explicitly encrypted.
4. Network needed on first build for WiX/NSIS provisioning (~40 MB) and WebView2 offline installer (~150 MB); app itself is offline-first.
5. `.crypto-key/key.bin` is never committed; loss of the key means encrypted data is unrecoverable.

---

## 7. Conclusion

**Workflow complete.** One real defect found (non-ASCII characters in `build.ps1` comments breaking the PS 5.1 codepage-safe regression check) and fixed with a two-character comment edit. All acceptance criteria now met: tests 60/0/13 green, CSS fresh, npm audit 0, data confinement intact, launchers ASCII-safe. No speculative features or unnecessary dependencies introduced. No further action required.
