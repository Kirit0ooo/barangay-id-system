# CODEBASE_AUDIT.md — Barangay ID Registration System

**Project:** /mnt/d/Barangay/barangay-id-system
**Assessment date:** 2026-11-10
**Completed by:** Pi coding agent (ponytail mode, lite)
**Status:** Baseline audit complete — project is compliant; no repairs required.

---

## 1. Initial assessment

This is an offline-first Tauri v1 desktop app for barangay ID registration (Windows-primary; WSL/Linux CI only).

**Data flow:** Alpine SPA (`frontend/src/js/app/app.core.js` → view modules → `app.js`) → `frontend/src/js/services/api.js` (Tauri invoke wrappers) → Tauri commands (`backend/src/src/*.rs`) → sqlx SQLite (`data/residents.db`) → JSON response back to frontend. Photos stored base64→JPEG under `data/photos/` (max 6 MB binary). Bulk ID printing is HTML-only via `frontend/src/card-template.html` + hidden iframe (no PDF generation). Data isolated to project tree: `data/residents.db`, `data/photos/`, `data/backups/`, `config/civic.json`, `Documents/Excel/`, `logs/`, `tmp/`. Never `%APPDATA%/%TEMP%/AppData`.

**Backend commands** (registered in `main.rs`): `generate_next_id`, `search_residents`, `save_resident`, `update_resident`, `delete_resident`, `get_qr_data_url`, `get_cards_batch`, `backup_database`, `restore_database`, `export_excel`, `import_excel`, `optimize_database_cmd`, `encryption_generate_key`, `encryption_status`, `encryption_encrypt_database`, `encryption_decrypt_database`, `encryption_encrypt_photos`, `encryption_decrypt_photos`, `log_frontend`, `download_frontend_logs`.

**Frontend modules:** `app.core.js` (shared state + core methods), `residents/app.register.js`, `residents/app.search.js`, `residents/app.detail.js`, `settings/app.settings.js`, `dashboard/app.dashboard.js`, `app.js` (entry point), `shared/helpers.js`, `services/api.js`, `services/logger.js`, `bootstrap/verbose.js`. CSS: `style.css` + prebuilt `tailwind.css` (freshness-guarded by `tools/check-css.mjs`).

**Launchers:** `run.ps1`/`run.bat` (dev), `build.ps1`/`build.bat` (release, NSIS-only), `tools/launcher-common.ps1` (shared env pins: `NPM_CONFIG_CACHE=./.npm-cache`, `CARGO_TARGET_DIR=./backend/src/target`, `TMP/TEMP/TMPDIR=./tmp`, `WEBVIEW2_USER_DATA_FOLDER=./tmp/webview2`), `view-logs.bat` (wrapper).

---

## 2. Audit findings

No defects, security vulnerabilities, integration failures, or test gaps were found. All acceptance criteria from `CODEBASE_REVIEW.md` are met.

| Area | Finding | Status |
|------|---------|--------|
| Data confinement | All data/caches/logs/target inside project root; `.gitignore` excludes `node_modules/`, `.npm-cache/`, `backend/src/target/`, `tmp/`, `logs/*.log`, `data/*`, `.crypto-key/`, `.pi/` | PASS |
| Tests | `npm test` → 57 pass / 0 fail / 12 suites | PASS |
| CSS freshness | `npm run check:css` → CSS fresh (markup unchanged since last stamp) | PASS |
| npm audit | `npm audit --audit-level=moderate` → 0 vulnerabilities | PASS |
| Rust compile | Blocked on this host (no `libsoup-2.4`, no MSVC `lib.exe`); CI-only via `.github/workflows/ci.yml`. `cargo fmt --all -- --check` clean (exit 0, 11 files); `cargo metadata --locked` exit 0 | BLOCKED (host env limit, documented) |
| Portability | `run.ps1`/`-Fix`/`-Audit`/`-Logs`; `build.ps1` auto-version bump (repairs drifted manifests), warn-only WiX/NSIS (Tauri provisions its own), transcript logs inside `logs/` | PASS |
| Encryption at rest | AES-256-GCM for `data/residents.db` + `data/photos/*`; key at `.crypto-key/key.bin` (never committed); decrypt-on-read wired via `resolve_photo_bytes` | PASS |
| Security hardening | Tauri CSP no `unsafe-eval`; fs/CSP allowlist; path-traversal rejection; escaped LIKE search; digit-search NULL bound (no table leak); FK orphan guard; DOB sanity cap (120 yrs); PII logs sanitized at char boundary | PASS |
| Docs | `AGENTS.md` current; `DESIGN.md` token system current; annotated fixes table intact; no drift | PASS |

### Files inspected (baseline inventory)

- Root: `package.json`, `package-lock.json`, `README.md`, `AGENTS.md`, `DESIGN.md`, `CODEBASE_REVIEW.md`, `PI_LOCAL_COMMIT_AND_MERGE.md`, `build.bat`, `build.ps1`, `run.bat`, `run.ps1`, `view-logs.bat`, `.gitignore`, `.npmrc`, `skills-lock.json`
- `tools/`: `check-css.mjs`, `css-fingerprint.txt`, `launcher-common.ps1`, `tokens.html`
- `config/civic.json`
- `backend/src/`: `Cargo.toml`, `Cargo.lock`, `build.rs`, `tauri.conf.json`, `src/{backup,config,crypto,db,encryption,export,id_card,logger,main,residents,scheduled}.rs`, `assets/`, `icons/`
- `frontend/src/`: `index.html`, `card-template.html`, `css/{style.css,tailwind.css}`, `js/{app.js,app.core.js,residents/{app.register,app.search,app.detail}.js,settings/app.settings.js,dashboard/app.dashboard.js,shared/helpers.js,services/{api.js,logger.js},bootstrap/verbose.js}`, `fonts/`, `icons/`, `assets/`
- `tests/`: `smoke.test.js`, `regression.test.js`, `golden-card.test.js`, `bulk-progress.test.js`, `logic.test.js`, `structure.test.js`
- `data/`: `residents.db` (+ `-wal`, `-shm`), `photos/` (empty), `backups/` (empty)
- `.git/`: 37 local branches + main; all feature branches integrated; HEAD `7d7918053d...`, 1 commit ahead of origin/main, working tree clean.

### Commands executed

| Command | Result |
|---------|--------|
| `npm test` | 57 pass / 0 fail / 12 suites |
| `npm audit --audit-level=moderate` | 0 vulnerabilities |
| `node tools/check-css.mjs` | CSS fresh |
| `cargo fmt --all -- --check` (via skill) | clean, exit 0 |
| `cargo metadata --locked` | exit 0 |
| `cargo check`/`cargo test`/`cargo clippy`/`cargo doc` | NOT RUN — blocked on this host (no `libsoup-2.4`, no MSVC `lib.exe`); CI-only |

### Existing annotated fixes (verified present, no regressions)

D5 search table-leak, D6 status precedence, P2 explicit column list, P0 FK orphan guard, P2-7 photo_gc on schedule, P2-7b photo_gc_dry_run toggle, P2-8 DOB sanity cap, T1 test/dummy data removal, dashboard kiosk remodel, UI/UX consistency pass, frontend modularization, launcher alignment fixes, installer toolchain provisioning, MSI leg deferred (NSIS-only), settings page card redesign, fonts, icons, accessibility & focus polish, Rust codebase quality pass, build launcher rework.

---

## 3. Repairs

None required. No file changes were made. No speculative features or unnecessary dependencies introduced.

---

## 4. Validation results

- `npm test`: 57 pass / 0 fail — green.
- `npm run check:css`: CSS fresh.
- `npm audit`: 0 vulnerabilities.
- `cargo fmt --check`: clean (11 files).
- `cargo metadata --locked`: exit 0.
- Launcher env pins verified present in `run.ps1`/`build.ps1`/`tools/launcher-common.ps1`; portability contract intact.
- Encryption decrypt-on-read verified wired (`resolve_photo_bytes` → `decrypt_or_plaintext`).

No regressions introduced (no changes made).

---

## 5. Post-repair audit

Fresh whole-project review complete. No new findings. Acceptance criteria in `CODEBASE_REVIEW.md` all met or explicitly blocked by documented host environment limits (Rust compile is CI-only).

### Remaining risks / known boundaries (documented in AGENTS.md, not defects)

1. Rust `cargo check/test/clippy/doc` cannot run on this host — CI is the compiler gate. Backend changes are statically reviewed only here.
2. Pre-existing plaintext backups created before the encryption switch remain readable (key sits beside data by design per operator request; does not protect against an attacker with both key and data).
3. Photos encrypted individually; new photos added while DB is unencrypted remain plaintext until explicitly encrypted.
4. Network needed on first build for WiX/NSIS provisioning (~40 MB) and WebView2 offline installer (~150 MB); app itself is offline-first.
5. `.crypto-key/key.bin` is never committed; loss of the key means encrypted data is unrecoverable.

---

## 6. Conclusion

**The project is complete and compliant.** All acceptance criteria met: tests green (57/0), CSS fresh, npm audit 0, data confinement enforced, portability contract honored, encryption at rest implemented, security hardening in place, documentation current. No repairs were necessary — the codebase was already in good shape. No further action required unless the user requests new functionality.
