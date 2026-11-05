# Barangay Resident ID Registration System

**Offline-first desktop app for registering Barangay residents, printing CR100 ID cards, and exporting Excel — built with Rust + Tauri + SQLite + Alpine.js.**  
Target: **5,000–10,000 residents**, **Windows 10/11 primary** (Linux/WSL secondary), **100% offline**, portable on any local folder with zero cloud dependencies.

> **Version:** 1.0.0 · **Identifier:** `com.barangay.idsystem` · **Edition:** Tauri 1.5

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Folder Contract (Isolation Mandate)](#folder-contract)
3. [Prerequisites](#prerequisites)
4. [Installation & Run](#installation--run)
5. [Production Build](#production-build)
6. [Features](#features)
7. [Architecture](#architecture)
8. [Technology Stack](#technology-stack)
9. [Data Model & Database](#data-model--database)
10. [ID Card — CR100 3.88×2.63"](#id-card--cr100-388263)
11. [Print Output (HTML-only)](#print-output-html-only)
12. [Configuration](#configuration)
13. [Logs & Observability](#logs--observability)
14. [Backup / Restore / Excel](#backup--restore--excel)
15. [Testing](#testing)
16. [Scripts Reference](#scripts-reference)
17. [Security & Isolation Hardening](#security--isolation-hardening)
18. [Troubleshooting](#troubleshooting)
19. [Deployment](#deployment)
20. [Documentation vs Reality](#documentation-vs-reality)
21. [Known Limitations](#known-limitations)

---

## Quick Start

```bat
:: Windows — double-click run.bat OR Powershell
run.bat
:: or
powershell -ExecutionPolicy Bypass -File .\run.ps1
:: then use build.bat for installer
build.bat
```

WSL (dev only, needs GTK for cargo check):
```bash
cd <project-root>
npm install
npx tauri dev
```

---

## Folder Contract

**Isolation Mandate:** *All* data, tests, tmp, cache, logs, build outputs **MUST** stay inside the project folder. No writes to `%APPDATA%`, `%TEMP%`, drive root, or home. Verified by `tests/smoke.test.js` + `tests/regression.test.js`.

```
<project-root>\
 ├── BarangayResidentID.txt              # original spec
 └── barangay-id-system\                 # ← ALL PROGRAM FILES (portable)
      ├── barangay.ico / barangay.png    # app icon
      ├── src\                           # frontend (no build step)
      │    ├── index.html                # single-page app (Alpine.js)
      │    ├── js/app.js                 # Alpine x-data app() — CRUD, search, ID
      │    ├── js/api.js                 # Tauri invoke wrapper
      │    ├── js/logger.js              # frontend logger → backend file
      │    ├── css/style.css
      │    ├── vendor/alpine.min.js      # vendored (offline)
      │    ├── css/tailwind.css           # prebuilt static utilities (hand-maintained; guard: npm run check:css)
      │    ├── assets/BALUD.png, bagong-pilipinas.png (logos), Boy.jpg, Girl.jpg (placeholder art, unreferenced — kept per test L3 pin)
      │    ├── card-template.html          # single render path: CR100 98.6×66.8 print template
      ├── src-tauri\                     # Rust backend
      │    ├── Cargo.toml / Cargo.lock
      │    ├── tauri.conf.json           # scope $APP/photos|documents|config|backups + $RESOURCE only
      │    ├── build.rs                  # tauri_build
      │    ├── icons/                    # 32,128, icns, ico
      │    ├── assets/fallback/
      │    ├── src/main.rs               # pins TMP/TEMP/WEBVIEW2 to tmp/
      │    ├── src/db.rs                 # get_project_root() + get_app_data_dir() + validate_isolated_write_path()
      │    ├── src/logger.rs             # get_logs_dir() isolated
      │    ├── src/residents.rs          # CRUD + photo + ID generation MAX suffix
      │    ├── src/backup.rs             # wal_checkpoint + validated copy (no VACUUM injection)
      │    ├── src/export.rs             # Excel via rust_xlsxwriter
      │    ├── src/id_card.rs            # QR data-URL + date helpers (no PDF; print is HTML-only)
      │    └── target\                   # Rust build cache — isolated, .gitignore
      │    ├── residents.db (+ -shm/-wal)
      │    ├── photos/{uuid}.jpg (600×800)
      │    ├── backups/  (.gitkeep)
      ├── logs\                          # isolated daily logs
      │    ├── app.log.YYYY-MM-DD (tracing rolling daily)
      │    ├── frontend.log
      │    ├── run-*.log (PowerShell transcript / Bat tee)
      │    └── README.md
      ├── tmp\                           # OS temp pinned here (WEBVIEW2, sqlite spill)
      │    ├── webview2/                 # WEBVIEW2_USER_DATA_FOLDER
      │    └── .gitkeep
      ├── .npm-cache\                    # npm cache via .npmrc cache=./.npm-cache
      ├── node_modules\                  # isolated deps
      ├── tests\
      │    ├── smoke.test.js             # isolation checks (manifest, data/, tmp/, .npmrc)
      │    └── regression.test.js        # HIGH-fix + P0 regression checks (growing; see §33 for P0 pins)
      ├── run.ps1                        # CANONICAL dev launcher (PowerShell 5.1+, -Logs/-Fix/-NoInstall/-Help/-Audit; transcript to logs/run-*.log)
      ├── run.bat                        # thin wrapper -> run.ps1 %* (pins same env; minimal :NOPS hint+pause only if PowerShell absent)
      ├── build.ps1 / build.bat          # release build -> src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi/ + logs/build-*.log
      ├── view-logs.bat                  # thin wrapper -> run.ps1 -Logs + explorer logs\ (Explorer open is the double-click value; tail logic lives in run.ps1)
      └── README.md
```

**How isolation works:** `src-tauri/src/db.rs:get_project_root()` walks the drive root → exe parent → cwd looking for marker `barangay.ico` or `src-tauri/`; `get_app_data_dir()` only returns inside that root; `logger::get_logs_dir()` returns `Result` (OS-temp fallback + error log, never `panic!`) (no `ProjectDirs` AppData fallback). Every write path (`backup`, `export`, `id_card`) calls `validate_isolated_write_path()` which asserts `parent.starts_with(root)`. `main.rs` pins `TMP/TEMP/TMPDIR/WEBVIEW2_USER_DATA_FOLDER` to `tmp/` at startup. `.npmrc` `cache=./.npm-cache` + Tauri `fs.scope` tight to `$APP/photos|documents|config|backups` + `$RESOURCE` prevents escape.

---

## Prerequisites

### Windows (primary)

1. **Rust 1.75+** — https://rustup.rs — `rustc --version`
2. **Node.js LTS 18+** — https://nodejs.org — `node --version` + `npm --version`
3. **WebView2** — preinstalled on Win11; Win10: https://developer.microsoft.com/en-us/microsoft-edge/webview2/
4. **Visual Studio Build Tools** — `Desktop development with C++` workload (for `tauri build` MSI). Without it `tauri dev` still runs (debug).

### WSL/Linux (dev secondary)

GTK needed for `cargo check`: `libglib2.0-dev libgtk-3-dev pkg-config`. Build not required on Linux — app ships as Windows MSI.

---

## Installation & Run

```bat
cd <project-root>

:: 1. deps (isolated cache .\.npm-cache, target inside src-tauri\target)
npm install

:: 2. dev (auto-detects free 127.0.0.1:14xx via TAURI_CONFIG env — see logs)
:: Option A — cmd
run.bat
:: Option B — PowerShell (CANONICAL, transcript to logs/run-*.log)
powershell -ExecutionPolicy Bypass -File .\run.ps1
:: flags: .\run.ps1 -Logs (tail app.log.*+frontend.log+run/build, replaces view-logs.bat) | -Fix (deep clean) | -NoInstall (fail fast) | -Help | -Audit (warn-only, parity with build.ps1)
::          run.bat forwards all flags: run.bat -Logs / run.bat -Fix / run.bat -Audit
:: Option C — direct (must pin env first — launchers do this for you)
npx tauri dev
:: Tip: verbose backend
set RUST_LOG=debug && run.bat
```

* `devPath: "../src"` + `withGlobalTauri: true` — no Vite; static files served directly. Hot reload not needed.
* First run auto-creates `data/residents.db` via `init_db()` + indexes. Check `logs/app.log` if `residents.db` missing — likely `data\` write perms.

---

## Production Build

```bat
cd <project-root>
build.bat
:: or
powershell -ExecutionPolicy Bypass -File .\build.ps1
:: manual
npx tauri build
```

Output (isolated, never in `%TEMP%`):
```
src-tauri/target/release/bundle/msi/Barangay ID Registration System_1.0.1_x64_en-US.msi
src-tauri/target/release/bundle/nsis/...
```
Install on Win10/11 (per-user MSI — no admin rights needed). The installer creates a **desktop shortcut automatically** (stock Tauri WiX behavior) and installs beside your data:

```
%LOCALAPPDATA%\Barangay ID Registration System\
 ├── Barangay ID Registration System.exe
 ├── data\      (residents.db, photos, backups — created on first run)
 ├── Documents\  logs\  tmp\  config\
```

One folder, exe + data together. Details: `src-tauri/wix/main.wxs` is the official Tauri v1.6 WiX template with two surgical changes — `InstallScope perUser` (+`limited` privileges) and `INSTALLDIR` under `LocalAppDataFolder` instead of Program Files (read-only; the app refuses to start there by design). If a `<project-root>\data` folder already exists, the app keeps using it (portable/USB workflow unchanged).

> **Migrating from an old perMachine install:** uninstall the old MSI first (Windows can't upgrade perMachine → perUser in place), back up via Settings → Backup, then install the new MSI and restore.
> **Uninstall wipes the install folder including `data\`** (stock WiX `RemoveFolder` behavior) — always take a Backup first.
> **Fully offline:** the MSI embeds the complete WebView2 runtime installer (~+127MB, fetched once by the *build* machine and cached) — the officer PC needs no internet at install time. If the runtime is already present, the installer skips it automatically.

---

## Features

* **Dashboard** — stats `Total` / `Active This Year` / `Male` / `Female` + 10 most recent (client-sorted `date_registered`).
* **Register/Edit** — fields: name (last/first/middle/suffix), `birth_date*` (DD-MM-YYYY, stored as entered), `gender*` (Male/Female), civil_status, citizenship (default Filipino), occupation, religion, blood_type, height/weight, contact_number, email, address (house/street/**purok**/barangay/city/province), Gov IDs (PhilHealth/SSS/TIN/voter's), emergency contact (name/relation/number), remarks toggle `ACTIVE/RENEW/LOST/DAMAGE`, `valid_until` auto +1 year (no UI), photo (base64 8MB limit → decoded 6MB → resized 600×800 JPEG `{uuid}.jpg`).
* **Photo slot** — `data/photos/{uuid}.jpg` only; no runtime fallback (P2-11: `src/assets/Boy.jpg`/`Girl.jpg` are unreferenced by code — kept as canonical placeholder art per regression-test L3 pin, not rendered). Without an upload the slot stays empty and the template hides it.
* **ID generation** — `BRGY-YYYY-00001` via `MAX(CAST(substr(resident_id,-5) AS INTEGER))` per year (avoids COUNT reuse after soft-delete), orphan photo cleanup on duplicate.
* **Search** — `last_name LIKE ? OR first_name LIKE ? OR resident_id LIKE ? OR contact_number LIKE ?` + digit-stripped contact match (so `09171234567` finds stored `0917-123-4567`) + optional `address_purok = ?` + `is_active` toggle, `LIMIT 2000`, ordered `last_name,first_name`.
* **Soft delete** — `UPDATE is_active=0`, no hard delete; search can include inactive.
* **ID Card** — Preview → Print on CR100 via `card-template.html` (single render path) — see next section.
* **Excel** — `rust_xlsxwriter` 37 columns, header navy `#003366`, frozen pane row 1, widths per column, `is_active=1` only.
* **Backup/Restore** — `PRAGMA wal_checkpoint(TRUNCATE)` → `fs::copy` to validated `data/backups/residents_backup_{timestamp}.db` (+ `photos_backup_{file}` dir copy). Restore copies safety backup first.

---

## Architecture

```
index.html (Alpine) —api.js—> Tauri invoke —main.rs—> Rust commands
                                         |-> db.rs (SqlitePool 5, SQLite file)
                                         |-> residents.rs (validate, MAX, save_photo)
                                         |-> id_card.rs (QR data-URL via qrcode)
                                         |-> backup.rs / export.rs
                                         |-> logger.rs (tracing daily)
                                    data/residents.db (WAL) + photos/ + logs/
```

* **Frontend:** single `x-data="app()"` Alpine object (`currentView` dashboard/register/search/settings, `form`, `searchResults`, `selectedResident`, `toast`). `API` wrapper in `api.js` does `window.__TAURI__.invoke` + `dialog.save/open/ask`.
* **Backend:** `tokio` async, shared `Arc<Mutex<AppState { pool, app_data_dir }>>` serializes DB ops; `tauri::generate_handler!` exposes 14 commands.
* **Entry trace (Register):** `app.js:saveResident()` auto `generateID()` if empty → `validate_resident_input()` → size guard 8MB → `save_photo()` (image resize) → `INSERT` → toast → `loadDashboard()`. On duplicate `resident_id` the just-saved photo is removed.

---

## Technology Stack

| Layer | Tech | Version | Notes |
|---|---|---|---|
| Desktop | Tauri | 1.5 (cli 1.6.3) | `custom-protocol`, `fs-write/read`, `dialog-all`, `shell-open` |
| Language | Rust | `edition 2021` |
| Async | tokio | 1 full | |
| DB | sqlx + sqlite | 0.7 | `runtime-tokio`, 5 conns, WAL |
| QR (preview/print) | qrcode | 0.13 + image 0.24 + base64 0.21 | preview data-URL |
| Excel | rust_xlsxwriter | 0.78 | pure Rust, no libclang |
| Legacy PDF | printpdf / genpdf | removed | PDF generation deleted; print is HTML-only |
| UUID | uuid | 1 v4 | photo filenames |
| Time | chrono | 0.4 serde | |
| Frontend | Alpine.js | vendor 3.x | `src/vendor/alpine.min.js` |
| CSS | Tailwind | prebuilt static | `src/css/tailwind.css` hand-maintained; freshness guard `npm run check:css` (offline, no CDN) |
| Node | @tauri-apps/cli | 1.6.3 | dev only |
| Logs | tracing + appender | 0.1/0.2/0.3 | rolling daily |
| OS | WebView2 | Edge bundled | Win11 built-in |

*No Vite, no framework build.* Static `src/` is `devPath`/`distDir`.

---

## Data Model & Database

**Table `residents`** (single table, no FK):

```sql
id INTEGER PK AUTOINCREMENT,
resident_id TEXT UNIQUE NOT NULL,   -- BRGY-YYYY-#####
last_name TEXT NOT NULL, first_name TEXT NOT NULL, middle_name TEXT, suffix TEXT,
birth_date TEXT NOT NULL, birth_place TEXT, gender TEXT NOT NULL, -- Male/Female
civil_status TEXT, citizenship TEXT DEFAULT 'Filipino',
occupation TEXT, contact_number TEXT, email TEXT,
address_house TEXT, address_street TEXT, address_purok TEXT, -- AVOCADO/RIMAS/PASAYAN/BUBUNAO/ASLUM
address_barangay TEXT DEFAULT 'Barangay', address_city TEXT, address_province TEXT,
religion TEXT, blood_type TEXT, height TEXT, weight TEXT,
philhealth TEXT, sss TEXT, tin TEXT, voters_id TEXT,
emergency_contact_name TEXT, emergency_contact_relation TEXT, emergency_contact_number TEXT,
emergency_contact_resident_id TEXT, -- nullable link to residents.resident_id (FK checked in app, see below)
photo_filename TEXT, is_active INTEGER DEFAULT 1,
date_registered TEXT NOT NULL DEFAULT (datetime('now')),
date_updated TEXT, remarks TEXT, -- ACTIVE/RENEW/LOST/DAMAGE
valid_until TEXT, -- ISO YYYY-MM-DD, auto +365d, migration added via ALTER TABLE
last_pdf_path TEXT, last_pdf_generated_at TEXT, -- legacy PDF audit trail: DORMANT (never written, kept for old DBs)
-- indexes: idx_resident_name(last,first), idx_resident_id, idx_purok, idx_active, idx_valid_until
```

* **Source-of-truth gates (data integration):** `validate_resident_complete` guards card printing — required names, past `birth_date` (future dates rejected at input AND at print time), `Male/Female` gender, purok allowlist, non-empty `resident_id`. `contact_number` / `emergency_contact_number` normalize PH mobiles to `09XX-XXX-XXXX` display form at write time (`normalize_ph_contact`; accepts `09…`, `+639…`, `639…` with spaces/dashes/parens); legacy non-PH values keep the charset/length rule. `address_purok` is rejected unless one of `AVOCADO/RIMAS/PASAYAN/BUBUNAO/ASLUM` (mirrors the frontend dropdown). Missing card slots render `N/A` (`field_or_na`), never blank.
* **Emergency contact decision:** free-form name/relation/number is KEPT for non-resident contacts (validated: name 2–60 letters, number PH-mobile rule), plus optional `emergency_contact_resident_id` holding another resident's `resident_id`. When set, create/update verify the linked resident exists (FK gate; error otherwise). A DB-level `FOREIGN KEY` clause was deliberately not added — SQLite FK enforcement on the existing single-table DB would risk legacy-row migration failures; the app gate + nullable column give the same integrity without it. Full FK constraint is a documented future step if the schema ever moves to multi-table.
* **Signature decision:** the card keeps a blank signature line for WET signing post-printing (no `signature` column, no digital capture). Rationale: barangay workflow signs physical cards; adding base64 signature storage/embedding is a one-column change (`signature_path` + embed in `draw_front`) if ever requested.
* **Legacy audit columns:** `last_pdf_path` / `last_pdf_generated_at` are DORMANT since PDF removal — never written, kept in struct + schema so existing DBs keep mapping with no migration.

* `data/residents.db-shm` / `-wal` are active WAL artifacts — never delete while app runs.
* `is_active` soft delete; `get_all_residents` returns only active.
* Photo stored outside DB as `data/photos/{uuid}.jpg`.

---

## ID Card — CR100 3.88×2.63"

* **Size:** `98.6 × 66.8 mm` (`CR100_W/H`).
* **Front:** navy header (Republic / Province / Municipality / Barangay), gold accent, white logo wells (placeholder `LOGO`), title, 1×1 photo card (inner white), signature line, 3-row grid: (Last/First/Middle) → (DOB/Gender/Civil) → (Purok/Contact/Blood), navy ID pill + white `VALID DD-MM-YYYY` chip, punong `[PUNONG BARANGAY NAME]` + year chips, micro-footer. Faint `BALUD` watermark (38pt) + guilloche dots.
* **Back:** navy `IN CASE OF EMERGENCY…` bar, Name/Relationship/Address (PUROK…, house/street second line if present)/Contact, yellow validity box, 17 mm QR (white bordered), footer `If found…`.
* **QR data (JSON):** `{"id","name":"Last, First Middle","dob":"DD-MM-YYYY","purok","contact","valid","status":"remarks"}` rendered via `qrcode` at 200 px (`get_qr_data_url`), fallback 1×1 PNG on error.
* **Fonts:** the card template requests Source Serif Pro/Inter with locally bundled TTFs (offline, no CDN). Screen preview and print share the exact CR100 geometry (98.6×66.8 seats, photo 20.9×26.9, QR pad 14.6) so WYSIWYG holds.

---

## Print Output (HTML-only)

No PDFs are generated. The single render path is `src/card-template.html` filled by `src/js/app.js:buildCardPayload` (photo + `get_qr_data_url` + civic config), printed on CR100 98.6×66.8mm:

* **Preview** — detail modal `Preview Card` opens the filled template (100% / Fit-width toggle) — WYSIWYG with print.
* **Print** — detail modal `Print Card (CR100)` prints front then back via hidden iframe (`@page { size: 98.6mm 66.8mm; margin: 0 }`).
* **Bulk** — Settings `Print All` loops active residents into one paginated HTML doc (`page-break-after`) with a live JS counter.
* **File when needed** — choose Save-as-PDF in the browser print dialog. `Documents/PDF/` is legacy and is never created.

---

## Configuration

* `src-tauri/tauri.conf.json` — `build.devPath: "../src"`, `distDir: "../src"`, `withGlobalTauri: true`, `allowlist.fs.scope: ["$APP/photos/**","$APP/documents/**","$APP/config/**","$APP/backups/**","$RESOURCE/**"]` (tight), `csp: default-src 'self'; img-src 'self' asset: http://asset.localhost data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; connect-src 'self' ipc: http://ipc.localhost` (no `unsafe-eval`; prebuilt CSS needs only `unsafe-inline` for `<style>` blocks).
* `.npmrc` — `cache=./.npm-cache` (isolated).
* Settings (Barangay/City/Province) — `localStorage barangaySettings` (frontend only, not DB).
* `.gitignore` — ignores `data/residents.db*`, `data/photos/*`, `data/backups/*`, `tmp/`, `logs/*.log`, `src-tauri/target/`, `node_modules/`, `.npm-cache/`; keeps `.gitkeep` files. PDF-era `data/id_cards/` + `Documents/PDF/` entries removed (P2-11 — dirs the app never creates). `tmp/.gitignore` ignores `*` (keeps `.gitkeep`) — never `node_modules/target` (those live elsewhere).
* **Card artwork ground truth** — canonical: `src/assets/BALUD.png` + `src/assets/bagong-pilipinas.png` (logos), P2-11: `src/assets/Boy.jpg`/`Girl.jpg` are unreferenced by code (no runtime fallback renders them) but kept as canonical placeholder art — pinned by `tests/regression.test.js` L3, mirrored in `src-tauri/assets/fallback/`. Removed: `src/barangay.ico` (byte-identical dup of root). Root `barangay.ico` is canonical: project-root marker + bundled resource (`tauri.conf.json` resources). `Barangay-ID.pub` + `Barangay-ID.jpg` at root are legacy MS Publisher/design sources, unused by the app, kept for reference only.
* **`valid_until` storage** — canonical ISO `YYYY-MM-DD` in DB (legacy `DD-MM-YYYY` rows auto-migrated on startup); DOB/QR `valid` display converts to `DD-MM-YYYY` at every edge (card template, Excel column). The black validity bar is `THIS ID IS VALID UNTIL {MONTH} {YYYY}` built by frontend `buildCardPayload` — never raw `DD-MM-YYYY`. The QR payload is shared via backend `get_qr_data_url`. Photo seat 20.9×26.9 and QR pad 14.6 are fixed in the template.

---

## Logs & Observability

All logs stay inside `logs\` (isolated, never `%TEMP%`/`%APPDATA%` outside tree). `logger.rs:get_logs_dir()` panics if not resolvable.

| File | Source | Rotate |
|---|---|---|
| `app.log.YYYY-MM-DD` | Rust `tracing` daily rolling | `tracing_appender::rolling::daily(logs,"app.log")` |
| `frontend.log` | `src/js/logger.js → log_frontend` invoke | append `frontend [level] msg` + `tracing::info!` |
| `run-YYYYMMDD-HHMMSS.log` | `run.bat` tee / `run.ps1` transcript | per launch |
| `localStorage barangay_log_buffer` | frontend 500-entry ring | downloadable JSON via Settings → Download Frontend Logs |

View: `logs/app.log.2026-09-09`, `logs/frontend.log`. Verbose: `set RUST_LOG=debug && run.bat`.

---

## Backup / Restore / Excel

* **Backup:** `Dashboard → Backup` or `Settings → Backup` → `data/backups/residents_backup_{timestamp}.db` (+ `photos_backup_{file}` dir). Implementation: `wal_checkpoint(TRUNCATE)` → `fs::copy` validated inside-tree (no string-interpolated `VACUUM INTO`).
* **Restore:** `Settings → Restore` → pick file inside tree (validated `starts_with(root)`), safety copy to `data/backups/safety_backup_{ts}.db` before `fs::copy(source, data/residents.db)`. Restart app after restore (pool holds old handles).
* **Excel:** `Dashboard/Settings → Export` or detail modal `Export to Excel` → validated inside-tree `*.xlsx`; 37 columns, navy header, thin borders, frozen row 1.

---

## Testing

Isolated to `tests\` — must stay inside project.

```bat
node tests/smoke.test.js       :: isolation checks: manifest, data/, tmp/, .npmrc
node tests/regression.test.js  :: regression checks: modal x-if, purok, scope tight, ID MAX, validation, orphan cleanup, wal_checkpoint, QR fallback (+ §33 P0 pins)
node tests/golden-card.test.js :: field-map parity for the single HTML render path + QR/date helpers (fixtures: normal, long-name edge, legacy DD-MM-YYYY valid_until)
npm test                        :: runs all three via node --test
```

* `cargo test --manifest-path src-tauri/Cargo.toml` — WSL without GTK `glib` will fail (expected Windows-primary); `cargo metadata` still verifies manifest.
* Dependency audit (warn-only, isolated caches): `powershell -ExecutionPolicy Bypass -File .\build.ps1 -Audit` → `cargo audit` (install hint if missing) + `npm audit --audit-level=moderate`. CI runs `npm audit --audit-level=high` and `rustsec/audit-check` on Windows.

---

## Scripts Reference

**Portable standard (2026-09-10 → updated 2026-09-10 R5/R6):** every launcher pins `NPM_CONFIG_CACHE=./.npm-cache`,
`CARGO_TARGET_DIR=./src-tauri/target`, `TMP/TEMP/TMPDIR=./tmp`,
`WEBVIEW2_USER_DATA_FOLDER=./tmp/webview2` **before** any npm/cargo call, so no
writes escape to `%APPDATA%`/`%TEMP%`. `run.ps1`/`build.ps1` require PowerShell 5.1+;
`.bat` files are thin wrappers (`powershell -ExecutionPolicy Bypass -File .\*.ps1 %*`)
with a legacy cmd fallback. `run.ps1` is canonical — `.bat` wrappers only forward `%*` and set the same env pins.

| Script | Purpose | Isolation |
|---|---|---|
| `run.ps1` *(canonical)* | prereq check (node/npm/rustc with clear msgs) → auto `npm ci`→`install` if `tauri.cmd` missing (reproducible via `package-lock.json`, covers R5 `scheduled.rs`/`photo_gc`/DOB helpers) → `npx tauri dev`; flags `-Logs` (tails `app.log.*`+`frontend.log`+latest `run/build-*.log` and exits) `-Fix` (deep clean `node_modules`+`.npm-cache` then `npm ci`→`install`) `-NoInstall` `-Help` `-Audit` (warn-only cargo audit + npm audit, parity with `build.ps1 -Audit`); `Start-Transcript` to `logs/run-*.log` | `logs/run-*.log` + env pins above |
| `run.bat` | **thin wrapper** → `run.ps1 %*` (same env pins before `powershell`); minimal `:NOPS` hint+pause only if PowerShell absent; forwards all flags | same pins, `logs/run-*.log` |
| `build.ps1` *(canonical)* | prereq check → `npm ci`→`install` if needed → `rustup target add` → `npx tauri build --target x86_64-pc-windows-msvc` → `bundle/msi/`+`bundle/nsis/`; transcript | `logs/build-*.log` + `src-tauri/target` |
| `build.bat` | **thin wrapper** → `build.ps1 %*` (same env pins); legacy `:NOPS` fallback only if PowerShell absent | same pins |
| `view-logs.bat` | **thin wrapper** → `run.ps1 -Logs` + `explorer logs\` (Explorer open is the double-click value; tail logic lives in `run.ps1`) | read-only, no env writes |


`main.rs` pins `TMP/TEMP/TMPDIR/WEBVIEW2_USER_DATA_FOLDER` to `tmp/` at startup (`tmp/webview2/`).

---

## Security & Isolation Hardening

* `validate_isolated_write_path()` on every `backup/export/id_card` write — canonicalizes parent and asserts `starts_with(root)`.
* `fs.scope` tight to `$APP/photos|documents|config|backups` + `$RESOURCE` (no `$DOWNLOAD/$DOCUMENT/$DESKTOP/$TEMP` leak).
* `resident.validate_resident_input()` — required `last/first/birth/gender`, gender `Male|Female`, lengths 100/20/254/32, email `contains('@')`, resident_id required. R3 normalizes DOB to `YYYY-MM-DD` (`normalize_birth_date`) accepting `DD-MM-YYYY`, `DD/MM/YYYY`, `YYYY-MM-DD`, validates real date & not future.
* Photo size guard 8 MB base64 / 6 MB decoded + JPEG re-encode 600×800.
* All `sqlx` queries use `bind()` (no injection); backup no longer interpolates SQL.
* ID `MAX(substr(...,-5))` per year avoids `COUNT` reuse; R2 retries `MAX→INSERT` once on UNIQUE `resident_id` violation (`BEGIN IMMEDIATE` + re-query MAX) and cleans orphan photo on each failure.
* R4 Excel import guards: header/width desync asserted (`37` cols), header count mismatch warns & rejects severely malformed sheets, row `len()>headers` truncated, missing columns padded as empty, logged via `tracing::warn!`.
* R5 Scheduled maintenance: `src-tauri/src/scheduled.rs` — `scheduled_backup` (`wal_checkpoint(TRUNCATE)` → `data/backups/scheduled_*.db`, isolated) and `photo_gc` (orphan scan under `data/photos`, logs count, deletes only with flag) wired via `main.rs` + frontend 24h `setInterval` + Settings Scan/Clean buttons. 100% offline.

---

## Troubleshooting

| Symptom | Cause / Fix |
|---|---|
| `residents.db` missing | Auto-created by `init_db()`; ensure `data\` writable (check `logs/app.log`). |
| Photo blank | `data/photos/` missing or `base64` decode failed (see `logs/app.log`); no runtime fallback — re-upload the resident photo. |
| Card prints blank | `data/photos/` missing or decode failed (see `logs/app.log`); re-upload the photo, then re-print. |
| Stale `Barangay-ID.pdf` at root | Legacy pre-removal artifact; safe to delete — print is HTML-only now. |
| `http://127.0.0.1:1430` in `logs/app.log` | Normal `tauri dev` dev-server (injected `TAURI_CONFIG devPath http://127.0.0.1:1430`); production uses `tauri://localhost`, no http. |
| `cargo check` on WSL fails `glib-2.0` | Expected — Windows MSI build needs Windows; WSL only for JS tests. |
| Restore not visible | Restart app — pool holds old WAL handles. |
| Installer not in `%TEMP%` | By design isolated in `src-tauri/target/release/bundle/`. |

---

## Deployment

* Copy the project folder to USB — data travels with it (portable).
* MSI installs per-user to LocalAppData (no admin; see wix/main.wxs) but data check still prefers `<project-root>\data` if mounted (via `get_project_root()`), else install-adjacent `data\`.
* For air-gapped: vendored `src/vendor/alpine.min.js` + prebuilt `src/css/tailwind.css` — no CDN needed.

---

## Documentation vs Reality

* `README` previously claimed `genpdf` + `xlsxwriter 0.6` — actual `rust_xlsxwriter 0.78` (pure Rust, no libclang). Now corrected.
* `.npmrc` previously documented `tmp=./tmp` — actual only `cache=./.npm-cache`; `tmp` pin is via `main.rs` env, now documented.
* `tauri.conf.json` scope is the granular `$APP/photos|documents|config|backups` + `$RESOURCE` list — tests assert tightness (old `$DOWNLOAD` expectation removed, `VACUUM INTO '` replaced by safe copy + validation).

---

## Known Limitations

* No auth/roles (single local operator); no hard-delete UI (soft `is_active=0` only).
* Enhancement backlog (P2-13, documented not implemented): duplicate-person guard wired (same name + DOB raises 'Possible duplicate' on create/update/import — `duplicate_person_gate`), no scheduled `VACUUM` (freed pages reused automatically; file never shrinks — run `VACUUM` manually via sqlite3 if size matters).
* `BEGIN IMMEDIATE` + retry loop now guards `MAX→INSERT` (R2) — cross-process duplicate is retried once with fresh MAX; orphan cleanup on each failure.
* Restore copies while pool alive — now reopens pool in-process (no restart needed for DB file) but UI should reload.
* CSP keeps `unsafe-inline` for `<style>`/Alpine attributes; `unsafe-eval` removed (R6 completed in M1 pass, pinned by tests).

---

## License / Offline

Zero runtime network deps. All vendor, DB, print-template, Excel, logging run fully offline. Tauri WebView2 is OS-provided.
