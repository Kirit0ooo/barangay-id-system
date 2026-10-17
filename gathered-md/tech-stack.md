# Project Context

> Derived by cold scan of the codebase (map-codebase), 2026-10-15. Source of truth = code; README claims were cross-checked.

## Stack

- **Desktop shell:** Tauri 1.5 (`@tauri-apps/cli` ^1.5 dev-only), Windows 10/11 primary (WebView2), MSI+NSIS bundles, identifier `com.barangay.idsystem`, version 1.0.1
- **Backend:** Rust 2021, `tokio` (full), `sqlx` 0.7 + SQLite (WAL, 5 conns), `serde`/`serde_json`
- **Backend libs:** `rust_xlsxwriter` 0.78 (export), `calamine` 0.24 (import), `qrcode` 0.13 + `image` 0.24 (QR data-URL, photo EXIF/resize/JPEG), `base64` 0.21, `uuid` v4 (photo filenames), `chrono`, `anyhow`, `tracing` + `tracing-appender` (daily rotation)
- **Dead dep:** `thiserror` declared in `src-tauri/Cargo.toml` but never used (0 references) — errors are `Result<T, String>` at the IPC boundary
- **Frontend:** Alpine.js 3 (vendored at `src/vendor/alpine.min.js`, no build step), static HTML (`src/index.html`, 648 lines), hand-maintained prebuilt Tailwind CSS (`src/css/tailwind.css`, freshness-fingerprinted by `tools/check-css.mjs`)
- **No Vite, no bundler, no TypeScript, no linter.** `src/` is both `devPath` and `distDir`.
- **Tests:** `node --test` (zero test deps, offline) + inline Rust `#[cfg(test)]` (~34 tests)
- **Tooling:** `run.ps1/build.ps1` launchers, `.github/workflows/ci.yml` (4 jobs)

## Architecture

**Pattern:** two-tier IPC command architecture — no service layer. Tauri `#[tauri::command]` functions *are* the business-logic layer; each Rust module owns one domain and registers its commands in `src-tauri/src/main.rs`.

```
index.html (Alpine x-data="app()")
  └─ js/api.js  — thin invoke wrapper, camelCase args (Tauri auto-maps to snake_case Rust params)
      └─ Tauri IPC (window.__TAURI__.invoke, withGlobalTauri)
          └─ main.rs invoke_handler (29 commands)
              ├─ residents.rs (1820 ln, 12 cmds) — CRUD, validation, photo pipeline, search, dashboard stats
              ├─ export.rs (1186 ln, 2 cmds)     — Excel export/import
              ├─ config.rs (361 ln, 4 cmds)      — civic.json config, open_path
              ├─ db.rs (919 ln, 1 cmd)           — pool init, path isolation, schema/migrations, optimize
              ├─ backup.rs (315 ln, 2 cmds)      — backup/restore (WAL checkpoint + atomic rename)
              ├─ scheduled.rs (304 ln, 2 cmds)   — scheduled backup, photo GC
              ├─ id_card.rs (271 ln, 2 cmds)     — QR data-URL, batch card payloads
              └─ logger.rs (216 ln, 0 cmds)      — tracing init, log retention caps
```

- **State:** `Arc<Mutex<AppState { pool, app_data_dir }>>` managed by Tauri; sqlx pool handles cloned out, `BEGIN IMMEDIATE` used explicitly (not `sqlx::begin()`) to serialize ID generation and concurrent writes.
- **Data:** `data/residents.db` (WAL) + `data/photos/*.jpg` + `config/civic.json` + `logs/`.
- **Isolation mandate (keystone):** every filesystem path resolves through `db::get_project_root()` + `db::validate_isolated_write_path()`; all writes stay inside the project folder. Logs, tmp, and even `WEBVIEW2_USER_DATA_FOLDER` are pinned project-local at startup (`main.rs`). Hardcoded `D:\Barangay\...` and `/mnt/d/...` candidates exist as last-resort fallbacks (existence-checked, never created).
- **Print flow:** HTML-only. `card-template.html` + `buildCardPayload()` → hidden iframe `srcdoc` → browser print (CR100 98.6×66.8mm). PDF generation was deleted; `get_documents_pdf_dir` remains as a legacy shim returning an explanatory error.
- **Frontend shape:** single Alpine object `app()` (1280 lines in `js/app.js`) holding all views (dashboard/register/search/detail/settings), form state, toast, and print/bulk-print orchestration. Pure helpers extracted to `js/helpers.js`; logging to `js/logger.js` (localStorage buffer + IPC to file).

## Conventions (Observed)

- **Error handling:** backend commands return `Result<T, String>` — human-readable strings, no error codes/enums. Internals use `anyhow` (`bail!`/`anyhow!`), converted with `map_err` at the boundary. Non-test code has essentially no `unwrap()`/`panic!` (one `.expect("helper emits ISO")` in a pure helper); startup failure logs + `exit(1)` instead of panicking. Frontend: `try/catch` everywhere (41 blocks), errors surfaced via `showToast(msg, 'error')` + `Logger.error`; form state restored from snapshot on save failure. No `alert()`.
- **API shape:** RPC over Tauri invoke (not REST). JS sends camelCase keys (`perPage`, `includeInactive`, `photoData` via `#[serde(rename)]`); responses serialize as snake_case (`r.last_name`, `resident_id`) — JS reads snake_case fields directly. One deliberate exception: `ResidentInput.photoData`.
- **Type system:** Rust strict, `cargo clippy -- -D warnings` + `rustfmt --check` in CI, 0 `unsafe`. Frontend is untyped vanilla JS — guarded instead by structural tests (`tests/structure.test.js` DOM-stack parser), golden-parity tests (`tests/golden-card.test.js` with grep-pins drift guard), and regression tests pinning past fixes.
- **Transactions:** manual `BEGIN IMMEDIATE` / `COMMIT` / `ROLLBACK` via `sqlx::query` for write paths needing serialization (ID gen with UNIQUE-retry, update, emergency-contact FK checks). Backup/restore: `PRAGMA wal_checkpoint(TRUNCATE)`, copy-then-rename atomicity, pool close + reopen.
- **Validation:** two-track. Backend authoritative (`validate_resident_complete`, `normalize_ph_contact`, `normalize_valid_until`, `parse_any_date_to_iso` in `residents.rs`); frontend mirrors contact rules in `saveResident()` for UX (commented as mirroring backend).
- **Data conventions:** soft delete via `is_active=0`; resident IDs `BRGY-YYYY-NNNNN`; dates ISO `YYYY-MM-DD` internally, `DD-MM-YYYY` display (`formatDateDDMMYYYY`); PH mobile normalized `09XX-XXX-XXXX`; photos resized + EXIF-oriented + re-encoded JPEG (6MB input cap, uuid filename).
- **Logging:** `tracing` daily rotation (`logs/app.log.YYYY-MM-DD`, 30-file retention) + `logs/frontend.log` (5MB cap, level sanitize, newline strip); frontend also buffers 500 entries in localStorage; verbose gate via `?verbose=1` / `localStorage` / `BARANGAY_VERBOSE`.
- **Offline-first:** no network calls anywhere (no reqwest, no CDN); `fetch()` only for local `card-template.html`. CI runs `npm audit` + `rustsec/audit-check`.
- **Comments carry history:** findings are tagged inline (`P0-3`, `F10`, `H-1`, `C3`, `M5`, `L-7`…) tied to past audits — grep these tags to find the reasoning.

## Signals / Active Considerations

- **Debt hotspots:** `residents.rs` (1820 lines: validation + photo pipeline + CRUD + search + dashboard) and `app.js` (1280-line Alpine object: all views + print + bulk + settings) are the two gravitational centers. Any new feature will touch one of them.
- **Dead dep:** `thiserror` in `Cargo.toml` unused — remove on next dependency pass.
- **Validation drift risk:** contact/date rules duplicated JS↔Rust. Backend is authoritative; golden-card tests pin the display formulas but the contact-rule mirror is only comment-guarded.
- **Dormant PDF residue:** `last_pdf_path` / `last_pdf_generated_at` columns + struct fields, `per_resident_pdf_filename()`, `get_documents_pdf_dir` shim — intentionally kept for schema compat, never written. Don't "clean up" without a migration plan.
- **Hardcoded path fallbacks:** `D:\Barangay\barangay-id-system\...` and `/mnt/d/...` in `db.rs`/`logger.rs` last-resort candidates — Windows/WSL-specific by design; portability beyond those hosts needs `get_project_root()` to resolve another way.
- **Allowlist over-broad:** `tauri.conf.json` has `window.all: true` with no JS/Rust usage found; deferred ruling in `.superpowers/sdd/phase2-hardening/progress.md` (needs Windows runtime to verify narrowing).
- **No transactions in export import path:** `export.rs` uses `begin()` in 4 places (import); resident CRUD relies on `BEGIN IMMEDIATE` instead — two transactional idioms coexist.
- **Test strategy is string/structure-level on the frontend** (no DOM runtime, no headless browser) — UI regressions that don't change DOM structure or formulas can slip through.
- **Conventions to follow:** no new deps without offline justification; keep all writes inside the isolation boundary; keep `npm test` (6 suites) and `npm run check:css` green; tag fixes with finding IDs in comments.
