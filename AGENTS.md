# AGENTS.md — Barangay ID Registration System - Permanent Guidance

Applies to all sessions in this workspace.

## Planning & tracking
- Plan non-trivial work with `/plan` before editing; keep `/todos` current as work progresses.
- Break large efforts into small, reviewable tasks and mark them done only when verified.

## Editing discipline
- Make small, reviewable edits; never reformat unrelated code.
- Prefer reading existing files and patterns over inventing new ones.
- Use checkpoints (`/rewind` available) before risky refactors so we can recover.

## Quality gates
- Run the project's tests/lint/typecheck after changes; do not declare done while checks are red.

## Communication
- Ask a clarifying question when a requirement is ambiguous instead of guessing.


This file instructs every AI agent working in this project. Read it before making any change.

## ⚠️ PRIMARY RULE — Directory confinement (absolute)

**All data, tests, temporary files, cache, build artifacts, logs, generated files, runtime state, `.env` backups, and anything else related to this program must be stored inside this directory folder:**

```
/mnt/d/Barangay/barangay-id-system/
```

- Nothing related to this program may be relocated, copied, or written to any unrelated system location (e.g. `/tmp`, `~/`, `AppData`, TEMP, or any drive outside this folder).
- `.env` backups and any other secret/config backups must live inside this directory (e.g. `config/` or `data/`), never outside it.
- Generated artifacts (caches, logs, temp files) must remain inside `tmp/`, `logs/`, `src-tauri/target/`, `data/`, or `Documents/` — all under this project root — and must not leak elsewhere.
- Do not delete, rename, move, or rewrite any code or project file until you have completed an initial assessment and architectural understanding of the entire program.

---

## How the program works (annotated)

This is an offline-first Tauri v1 desktop application for barangay ID registration. It runs entirely on the local machine (Windows-primary; WSL/Linux for CI only).

### Data flow
```
Frontend (Alpine SPA, src/index.html → src/js/app.js)
      → src/js/api.js (Tauri invoke wrappers)
      → Tauri commands (src-tauri/src/*.rs)
      → sqlx SQLite (data/residents.db)
      → JSON response back to frontend
```
- All IPC is asynchronous and parameterized (no string-interpolated SQL).
- Photos are stored base64→JPEG under `data/photos/`; max 6 MB binary (~8 MB base64).
- Bulk ID printing is HTML-only: fills `src/card-template.html` per row via a hidden iframe (browser Print dialog / Save-as-PDF optional). **No PDF generation exists anywhere** (removed in a prior phase).
- Data is isolated to the project tree: `data/residents.db`, `data/photos/`, `data/backups/`, `config/civic.json`, `Documents/Excel/`, `logs/`, `tmp/`. Never AppData/TEMP.

### Backend commands (src-tauri/src/main.rs registers these)
| Command | Purpose |
|---------|---------|
| `generate_next_id` | Atomic `BRGY-YYYY-XXXXX` under `BEGIN IMMEDIATE` (F5: prevents concurrent duplicate IDs) |
| `search_residents` | Escaped LIKE search; digit-stripped contact branch; soft-deletes hidden by default (F2, D5, P0-3) |
| `save_resident` / `update_resident` | Create/update with validation + orphan-photo cleanup |
| `delete_resident` | Soft-delete (is_active=0) with FK orphan guard (emergency_contact_resident_id) |
| `get_qr_data_url` | Mint plain 7-key JSON QR (unsigned-by-design); completeness gate via `validate_resident_complete` |
| `get_cards_batch` | Batch print data (max 200 IDs per IPC) |
| `backup_database` / `restore_database` | Atomic copy-to-tmp-then-rename; WAL checkpoint; 30-day retention; photo GC |
| `export_excel` / `import_excel` | 37-column workbook write + strict import |
| `optimize_database_cmd` | PRAGMA optimize + incremental vacuum (Settings → Database Health) |
| `encryption_generate_key` | Generate 256-bit AES key at `.crypto-key/key.bin` (fails if key exists) |
| `encryption_status` | Report key exists?, DB plaintext/encrypted?, photo counts plaintext/encrypted |
| `encryption_encrypt_database` | Encrypt `data/residents.db` at rest (AES-256-GCM); requires DB not in active use |
| `encryption_decrypt_database` | Decrypt `data/residents.db.enc` back to `data/residents.db` |
| `encryption_encrypt_photos` | Encrypt all plaintext photos in `data/photos/` |
| `encryption_decrypt_photos` | Decrypt all encrypted photos in `data/photos/` |

### Encryption at rest (2026-10-16)

File-level AES-256-GCM encryption for `data/residents.db` and `data/photos/*`:

| Component | File | Detail |
|-----------|------|--------|
| Crypto module | `src-tauri/src/crypto.rs` | AES-256-GCM encrypt/decrypt, key file ops, dir-level encrypt/decrypt, unit tests |
| Tauri commands | `src-tauri/src/encryption.rs` | 6 commands: generate key, status, encrypt/decrypt DB, encrypt/decrypt photos |
| Key storage | `.crypto-key/key.bin` | 32-byte random key, created on first `encryption_generate_key`, never rotated |
| File format | `[magic(8) "BRGYENC1"][version(1)][nonce(12)][ciphertext+GCM tag]` | 9-byte header + nonce + ciphertext+GCM tag, random nonce per file |
| API wrappers | `src/js/api.js` | `encryptionGenerateKey`, `encryptionStatus`, `encryptionEncryptDatabase`, `encryptionDecryptDatabase`, `encryptionEncryptPhotos`, `encryptionDecryptPhotos` |
| Settings methods | `src/js/app.settings.js` | UI methods calling each API wrapper with toast feedback |
| Photo decrypt-on-read | `src-tauri/src/residents.rs` `resolve_photo_bytes` | Wired `crate::crypto::load_key` + `CryptoKey::decrypt_or_plaintext` (was `#[allow(dead_code)]`, zero callers) so `get_photo_base64`/`get_resident_photo`/`id_card` print decrypted photos instead of broken images after `encryption_encrypt_photos`. Falls back to raw bytes when no key exists. |

**Threat model:** Protects data from being read by anyone who copies files without also copying `.crypto-key/`. Does NOT protect against an attacker who has both the key file and the data (key sits beside data by design per operator request).

**Known boundary gaps:** Pre-existing plaintext backups (created before encryption switch) remain readable. Photos encrypted individually; any new photos added while DB is unencrypted remain plaintext until explicitly encrypted.

### Frontend modules
| File | Role |
|------|------|
| `src/index.html` | Hub-and-spoke welcome dashboard (service tiles) + register/search/settings/detail views; `barangay.ico` favicon + `barangay.png` nav logo |
| `src/js/app.js` | Alpine SPA: hub dashboard with big service tiles; register/search/settings/detail/bulk-print/import/backup/restore; auto-returns to hub after save/delete/renew/cancel; single `applyTheme()` writer |
| `src/js/helpers.js` | Pure helpers (dates, contacts, purok options, `residentStatus`, `parseValidUntil`, `todayISO`); dependency-free so tests can load it |
| `src/js/api.js` | Tauri invoke wrappers for all commands |
| `src/js/logger.js` / `verbose.js` | Frontend logging to localStorage buffer + backend `log_frontend` invoke |
| `src/css/style.css` | Scoped components; kiosk dashboard styles (`.kiosk-head`, `.kiosk-stats`, `.kiosk-stat`, `.kiosk-tiles`, `.kiosk-tile`, `.kiosk-status-group`) + legacy hub/dashboard classes; opt-in `html[data-theme="red-gradient"]` body rule |
| `src/css/tailwind.css` | Prebuilt static utilities; freshness-guarded by `tools/check-css.mjs` (fingerprint over index.html + all src/js/*.js except vendored libs, derived at runtime) |
| `src/card-template.html` | CR100 print template; `textContent` for text + image allowlist (`SAFE_IMG` drops `javascript:`) |

### Hub-and-spoke user workflow (non-tech-savvy officers)

The app opens on a **Welcome hub** with three big service tiles instead of top-level navigation tabs:

| Tile | Action |
|------|--------|
| **New Resident Registration** | Opens the register form. Fill → Add photo → Save → auto-return to hub |
| **Resident List & Search** | Opens searchable list. Filter All / Active / Renew / Lost / Damage / Inactive. Tap a resident → View / Edit / Renew / Mark inactive / Print card → auto-return to hub |
| **Settings** | Barangay info, backup, restore, Excel export/import, photo cleanup, logs → auto-return to hub |

Every spoke view has a **← Back to Home** button that returns to the hub. On success (save, delete, renew) the app also returns to the hub automatically after a short toast. On validation error, the app stays on the current screen so no data is lost.

### Default branding
- App icon / favicon: `barangay.ico` at project root (linked in `<head>`)
- Nav logo: `src/barangay.png`
- Card-template logos (seal/emblem): unchanged in `src/card-template.html`

---

## Annotated fixes already applied (do not regress)

| Fix | File | What changed | Why |
|-----|------|--------------|-----|
| **D5 — Search table-leak** | `src-tauri/src/residents.rs:211-221` + `:1159` | Extracted pure `fn digit_search_pattern(query) -> Option<String>` returning `None` for digit-free (name-only) queries; bound as SQL `NULL` (falsy) so the contact `OR` branch drops out instead of matching every row via `"%%"` | Name-only queries were leaking the whole table (capped at SEARCH_RESULT_LIMIT=2000) |
| **D6 — Status precedence** | `src/js/helpers.js` `residentStatus(r, isExpired)` | Operator remarks (LOST/RENEW/DAMAGE) now outrank generic expiry; order: Inactive → remarks → Expired → ACTIVE | An expired card with an operator-set LOST/RENEW/DAMAGE was rendering as merely "Expired" |
| **P2 — Explicit column list** | `src-tauri/src/residents.rs` `RESIDENT_COLUMNS` | All queries list columns explicitly instead of `SELECT *` | Schema drift (future DROP of `last_pdf_*`) is caught at compile-time; legacy `email` column never selected |
| **P0 — FK orphan guard** | `src-tauri/src/residents.rs` `delete_resident` | Before soft-deleting, checks `emergency_contact_resident_id` and legacy `emergency_contact_number` references | Prevents deactivating a resident still referenced as an emergency contact |
| **P2-7 — photo_gc on schedule** | `src-tauri/src/scheduled.rs` `scheduled_backup` | Added `photo_gc(state.clone(), Some(!dry_run)).await` after retention block so orphan photos are auto-deleted after each scheduled backup | Photo dir never accumulates unreferenced files between manual GC runs |
| **P2-7b — photo_gc_dry_run toggle** | `src-tauri/src/config.rs` `CivicConfig` + `set_civic_config` + `load_civic_config`; `src-tauri/src/scheduled.rs`; `config/civic.json` | Added `#[serde(default)] photo_gc_dry_run: bool` field (default false). `scheduled_backup` reads config and passes `!dry_run` to `photo_gc` so dry_run=true scans+logs only. `set_civic_config` accepts the bool via partial JSON update. Tests added. | Operators who prefer manual-only GC can set `photo_gc_dry_run: true` in config/civic.json without hand-editing |
| **P2-8 — DOB sanity cap** | `src-tauri/src/residents.rs` `normalize_birth_date` | Rejects birth dates more than 120 years before today, plus existing future-date rejection | A living person cannot be over 120 at registration; prevents data-entry errors |
| **T1 — Remove test/dummy data** | `data/residents.db`, `data/photos/`, `data/backups/` | Deleted all 3 dummy resident rows (BRGY-2026-00001/002/003 with TEST/TEST/TEST and biblical names), removed all orphan photos, removed plaintext backups containing dummy data | Eliminated test fixtures from live database; all remaining residents are real registrations |

---

## Dashboard kiosk remodel (2026-10-18)

Remodeled the Welcome dashboard into a kiosk layout (`src/index.html` + `src/css/style.css`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Layout | `src/index.html:42-174` | Replaced `<details class="breakdown">` collapsible section with always-visible kiosk dashboard; no collapsible/expandable sections |
| Header | `src/index.html` | `.kiosk-head` header with serif `DASHBOARD` title (`--font-display`) |
| Stat row | `src/index.html` | `.kiosk-section--registry` wraps `.kiosk-stats` grid with 3 stat cards (Total Residents / By Gender / Registrations This Year); `.kiosk-section__head` header with title + live total badge; cards have `.kiosk-stat:hover` lift effect; gender card shows inline bar chart + male/female ratio |
| Card status | `src/index.html` | `.kiosk-section--status` wraps `.kiosk-status-group` with 4 color-coded status cards (Active/Renew/Lost/Damage); `.kiosk-section__head` header; grid gap-12 for comfortable card separation |
| Section CSS | `src/css/style.css` | `.kiosk-section`, `.kiosk-section__head`, `.kiosk-section__title` rules; `.kiosk-stat` padding 22px/20px, 2px border, radius 16px, hover lift with border-color transition + box-shadow |
| Gap utilities | `src/css/tailwind.css` | Added `gap-8` (2rem), `gap-10` (2.5rem), `gap-12` (3rem); card status grid uses `gap-12` |
| Gauge | removed | The SVG balance gauge and `stat-hero` layout were deleted outright (no `gaugeArcs()`/`gaugeGap()` in `app.dashboard.js`); gender split is now the `.kiosk-stat__bar` inline bar + Male/Female counts |
| Service tiles | `src/index.html` | `.kiosk-tiles` grid with oversized touch tiles (`.kiosk-tile`, min-height 120px, 72px icons, 40px arrow) |
| Icons | `src/index.html` | PNG glyphs under `src/icons/` referenced via `<img src="icons/*.png" />` (person, users, calendar, check, refresh, badge, alert, document, search, settings). Sizes via `width`/`height` CSS (24px stat, 32px tile) — fixed-color PNGs, no `currentColor` theming. |
| CSS | `src/css/style.css` | `.kiosk-stat__icon img` and `.kiosk-tile__icon img` use `width`/`height`/`object-fit:contain` for fixed sizing — no `font-size`/`line-height` Unicode scaling. |

Validation: `npm test` 57/0/12, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Standard project coding workflows

### Run tests
```
npm test
```
- 6 suites: smoke, regression, golden-card, bulk-progress, logic, structure
- Must be **57 pass / 0 fail** before declaring any change complete.
- `tests/logic.test.js` evaluates `src/js/helpers.js` in a Function scope (real source, not a copy).
- `tests/regression.test.js` reads project files by path — if you move a markdown/doc file, update the reference there too.

### CSS freshness check
```
npm run check:css
```
- `tools/check-css.mjs` fingerprints `index.html` + every `src/js/*.js` (vendored libs excluded, derived at runtime); must report **CSS fresh**.
- If you change `index.html` or any `src/js/*.js` markup/classes, run `npm run stamp:css` to update `tools/css-fingerprint.txt` (only if you intentionally changed CSS-relevant markup; zero new CSS classes → just restamp).

### Rust backend (CI-only locally)
- `cargo test`, `cargo clippy`, `cargo fmt --check` **cannot run on this host** (no libsoup-2.4/GTK; documented env limit). Rust verification is CI-only via `.github/workflows/ci.yml` (`cargo test --manifest-path src-tauri/Cargo.toml`).
- `cargo fmt --check` is red on 5 pre-existing files you did not touch (`backup.rs`, `config.rs`, `id_card.rs`, `logger.rs`, `scheduled.rs`). Leave alone to avoid an unrelated bulk reformat.
- `npm audit` → must be **0 vulnerabilities**.

### When you move any project file
1. Update all references (tests, imports, AGENTS.md paths).
2. Re-run `npm test` and `npm run check:css`.
3. Confirm nothing broke.

### Do not
- Move program data or resources to unrelated system locations.
- Overwrite/delete user data (`data/residents.db`, `data/photos/`, `data/backups/`, `config/civic.json`) unless explicitly required, well-understood, and safe.
- Introduce speculative features or unnecessary dependencies without clear technical justification.
- Leave temporary debugging code or unnecessary artifacts behind.
- Include sensitive data (API keys, tokens, passwords) in plaintext in the project.

### File locations (all inside project root)

| Kind | Location |
|------|----------|
| Source (frontend) | `src/` |
| Source (backend) | `src-tauri/src/` |
| Tests | `tests/` |
| Build scripts / tools | `tools/` |
| Config | `config/civic.json` |
| User database | `data/residents.db` |
| Encryption key | `.crypto-key/key.bin` — 32-byte AES-256 key; sensitive, never commit to version control |
| Photos | `data/photos/` |
| Backups | `data/backups/` |
| Logs | `logs/` |
| Temp / cache | `tmp/` |
| Rust build output | `src-tauri/target/` |
| Gathered docs | `gathered-md/` |
| Project docs (keep in place) | `README.md`, `Documents/README.md`, `Documents/Verify/README.md` |
| Agent instructions | `AGENTS.md` (this file) |

## Keeping AGENTS.md current

This file is a living document. Whenever the program changes, update AGENTS.md in the same session:

- New commands, modules, or file locations → add to the tables above.
- New fixes → add a row to the "Annotated fixes" table.
- New workflows or commands → add to "Standard project coding workflows".
- Directory rule or confinement scope changes → edit the PRIMARY RULE at the top.
- UI polish → update "Frontend modules" table and note changed CSS classes here.

Never let AGENTS.md drift from the actual code; an outdated AGENTS.md misleads the next agent.

## Register polish (2026-10-16)

Applied visual polish to the "Register New Resident" view (`src/index.html` + `src/css/style.css`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Header polish | `src/index.html:146-158` | `register-head` class + `register-eyebrow` + `register-sub` hint; `register-clear` buttons; flex-1 title column |
| Section animation | `src/index.html:196,222,239` | Added `register-section` class to Personal Info / Address / Emergency sections so CSS entry animation (`register-enter`) fires |
| Section icons | `src/index.html:199,225,242` | Added `register-section__icon` SVG glyphs beside section numbers (person icon for Personal Info, location pin for Address, phone for Emergency) |
| Required badge | `src/index.html:205` | `* required` badge: `hidden sm:block` (hidden on mobile to avoid crowding), `text-slate-500` contrast, `whitespace-nowrap` |
| ID row layout | `src/index.html:183` | Wrapped Resident ID input + Generate button in `register-idrow` for stacked-mobile / side-by-side-sm layout |
| Actions bar | `src/index.html:273-279` | Added `register-actions__note` hint + `register-actions__btns` wrapper for sticky mobile actions |
| CSS | `src/css/style.css` | Added `.register-section__icon` rule; added `.register-photo__preview`, `.register-photo__hint`, `.register-remove` photo-area rules; wired up existing `.register-head`, `.register-section`, `.register-idrow`, `.register-actions__btns`, `.register-photo__cta` rules so animations, sticky actions, photo states, and remove button all render |

Validation: `npm test` 57/0/12, `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Welcome dashboard polish (2026-10-16)

Applied frontend-create principles to the welcome hub (`src/index.html:41-66` + `src/css/style.css:1022-1130`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Header layout | `src/index.html:41-48` | Flex row: Welcome heading + motto on left, `hub-pill` Dashboard chip on right; removed `sm:text-[28px]` from h2 to match other pages' plain `text-2xl`; added `.hub-motto` CSS rule matching `register-sub` styling |
| Hub tiles | `src/index.html:49-66` | Per-tile icon accent colors (navy/gold), added `hub-tile__arrow` → indicator with hover reveal, `hub-tile__steps` moved after arrow on register tile, per-tile `hub-tile__icon--register/search/settings` classes |
| CSS | `src/css/style.css` | Added `.hub-pill` civic chip, per-tile icon background colors, `.hub-tile__arrow` hover-reveal animation, fixed `margin-top:auto` conflict between `.hub-tile__steps` and `.hub-tile__chips`, added visible bevel to `.hub-tile` (gradient white→#F7F9FC background so white ::before top/left highlight shows, :active pressed state with translateY + shadow reduction), added `.hub-motto` subtitle rule matching `register-sub` styling |

## Dashboard stat cards & gauge polish (2026-10-16)

Applied frontend-create principles to stat cards, stat hero, and gauge (`src/index.html:123-148` + `src/css/style.css:244-367`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Gauge color tokens | `src/index.html` | Replaced hardcoded `#eef2f7`/`#cbd5e1`/`#2563eb`/`#ec4899` gauge SVG colors with civic tokens `--border`/`--border-strong`/`--color-primary`/`--color-accent` |
| Gauge dot/fill tokens | `src/css/style.css` | Replaced hardcoded `#2563eb`/`#ec4899` gauge dot and fill colors with `--color-primary`/`--color-accent` and matching shadow RGBA values |
| Gauge pct text colors | `src/css/style.css` | Aligned `.gauge-chip--male .gauge-chip__pct` → `--color-primary` (navy) and `.gauge-chip--female .gauge-chip__pct` → `--color-accent` (gold), fixing the previous blue/orange mismatch with arcs |
| Stat card group label | `src/index.html:123-124` | Wrapped four status cards in `.stat-card-group` with `.stat-card-group__label` "Card Status" eyebrow |
| Stat card hover | `src/css/style.css` | Added `filter:brightness(1.15)` on icon hover for all four status card variants |
| CSS | `src/css/style.css` | Added `.stat-card-group` and `.stat-card-group__label` rules |

Validation: `npm test` 57/0/12, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Section header uniformity (all pages) (2026-10-16)

Made all four page section headers structurally uniform (`src/index.html:41-66,157-170,298-304,398-404,524-532`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Back button position | `src/index.html:526-530` | Detail page: moved `hub-back` from right side to left side, matching Register/Search/Settings |
| Redundant class removed | `src/index.html:167` | Register page: removed misused `register-clear` class from edit-mode Back button (was not a Clear button) |
| Consistent classes | All pages | All headers now use `hub-back` button class + `view-title` h2 class + hub-back on left |

Validation: `npm test` 57/0/12, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Frontend modularization (2026-10-16)

Split monolithic `src/js/app.js` (1262 lines) into 6 focused classic-script modules, zero runtime behavior change:

| File | Lines | Role |
|------|-------|------|
| `src/js/app.core.js` | ~190 | `window.appState` shared state + core methods (init, theme, civic config, settings, toast) |
| `src/js/app.register.js` | ~130 | Register view: ID generation, photo upload, save/update, clear form, edit mode |
| `src/js/app.search.js` | ~80 | Search view: searchResidents, applySearchCategory, pager, fetchAllResidentsPaged |
| `src/js/app.settings.js` | ~100 | Settings view: backup, restore, export, import, photo GC, optimize |
| `src/js/app.detail.js` | ~260 | Detail view: view/close, delete, renew, card payload, print, preview, bulk print |
| `src/js/app.dashboard.js` | ~70 | Dashboard view: status label, gauge arcs, expiry countdown |
| `src/js/app.js` | ~20 | Entry point: `function app() { return window.appState; }` + Alpine bootstrap |

**Design decisions:**
- `window.appState` (not `window.app`) — avoids collision where `function app()` would overwrite the state object at script load time
- Classic `<script>` (no ES modules) — `x-data="app()"` resolves the global function; no `type="module"` conversion needed, no CORS issues under Tauri's custom protocol
- Load order matters: `app.core.js` first, then view modules, then `app.js` entry point
- Tests updated to concatenate all app modules for path-based string checks (tests/bulk-progress.test.js, tests/golden-card.test.js, tests/regression.test.js, tests/structure.test.js)

Validation: `npm test` 57/0/12, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities, `node --check` clean on all 7 JS files.

## Launcher alignment fixes (2026-10-17)

Applied robustness fixes to the Windows PowerShell/batch launchers, zero logic changes:

| Fix | File | Detail |
|-----|------|--------|
| Inline fallbacks | `run.ps1`, `build.ps1` | When `tools/launcher-common.ps1` is missing, define stub `Set-PortableEnvPins`/`Restore-EnvPins`/`Invoke-SafeNative` so unconditional calls never hard-crash under `Set-StrictMode -Version Latest` |
| Flag docs | `build.bat` | Added `-EnableVerboseLogging` to the documented forwarded-flags header and `:NOPS` echo line (build.ps1 already accepts it via `%*`) |
| MSVC gate | `build.ps1` | vswhere.exe existence alone no longer passes — now verifies VC.Tools.x86.x64 toolset is actually installed, matching run.ps1's strict check |
| WiX gate | `build.ps1` | Checks `WiX Toolset v3.11\bin\candle.exe` path first; `candle.exe` on PATH is accepted only if `-version` reports v3.11 (v4+ rejected, Tauri v1 incompatible) |
| Stale comment | `tools/launcher-common.ps1` | `Ensure-DataDir` comment corrected: both run.ps1 and build.ps1 call it (was "run.ps1 only") |
| -Logs narrative | `run.ps1` | `.SYNOPSIS` reworded from "replaces view-logs.bat" to "supersedes view-logs.bat logic; wrapper remains" since view-logs.bat still exists as a live Explorer wrapper |

Validation: `npm test` 57/0/12, `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Settings page card redesign (2026-10-17)

Applied visual polish to the Settings page cards (`src/index.html` + `src/css/style.css`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Card wrapper | `src/index.html:398,424,474,488,507` | All 5 cards now use `.card card--section` with bordered surface; About card uses `.card--about` dark slate surface |
| Section headers | `src/index.html` | All 5 section heads now use `.section-head` with inline SVG icon + `.section-head__title`
| Action chip grid | `src/index.html:430-495` | Data & Maintenance (`.settings-grid` at :430), Printing (`.settings-grid` at :480), System Logs (`.settings-grid settings-grid--log` at :495); each chip uses `.settings-chip` with consistent icon+label layout |
| Settings actions | `src/index.html:411-416` | Barangay Information actions moved to `.settings-actions` with `.settings-btn--save/--reset/--theme` subclasses |
| Progress bar | `src/index.html:463` | Replaced undefined `bg-gray-200`/`bg-blue-600` with `.settings-progress__track` + `.settings-progress__fill` rules in style.css |
| Checkbox accent | `src/index.html:416,435` | Replaced undefined `text-blue-600 focus:ring-blue-500` with `.settings-checkbox` (accent-color var) — theme toggle at :416, backup toggle at :435 |
| Photo grid | `src/index.html:447` | Fixed bare `grid-cols-3` → `sm:grid-cols-3` so 3 columns render at ≥640px (1 column below) — bare `grid-cols-3` was inert since it is not in the hand-built tailwind.css subset |
| CSS | `src/css/style.css` | Added `.card--section`, `.card--about`, `.settings-grid`, `.settings-grid--log`, `.settings-chip`, `.settings-actions`, `.settings-progress__track`, `.settings-progress__fill`, `.settings-checkbox`, `.about-grid`, `.about-kv__key`, `.about-kv__value`, `.about-note`, `.section-head__icon`, `.settings-chip svg`, `.log-info`, `.log-info__label`, `.log-info__text` |

Validation: `npm test` 57/0/12, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Fonts & bundled type (2026-10-17)

All typefaces are bundled locally — no CDN, no external fonts:

| Font | File | Used in |
|------|------|----------|
| Inter (400/500/600/700) | `src/fonts/Inter-{Regular,Medium,SemiBold,Bold}.ttf` | SPA `--font-sans`, card `--font-body`/`--font-data` |
| Source Serif Pro (600) | `src/fonts/SourceSerifPro-SemiBold.ttf` | `--font-display` (dashboard title, card titles) |
| JetBrains Mono (400) | `src/fonts/JetBrainsMono-Regular.ttf` | `--font-mono` |
| OFL license text | `src/fonts/OFL.txt` | SIL OFL 1.1 notice for the three families |

- `src/css/style.css`: `@font-face` declarations reference `../fonts/` (style.css lives in `src/css/`).
- `src/card-template.html`: `@font-face` declarations reference `fonts/` (template lives at `src/` root).
- Replaced `"Work Sans"` with `"Inter"` in card-template.html header comment and README.md body-font line.
- CSP: `tauri.conf.json` has no `font-src`, so bundled fonts load under `default-src 'self'` (no CSP change needed).

Validation: `npm test` 57/0/12, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Icons (2026-10-17)

Dashboard glyphs are PNG files under `src/icons/` referenced via `<img src="icons/*.png" />` in `src/index.html` (person, users, calendar, check, refresh, badge, alert, document, search, settings). These are static image assets — they do not respond to `currentColor` theming but keep theming consistent across the navy/gold token system since each PNG is a fixed-color render. Five inline `<svg>` section-head icons on the Settings cards were replaced with PNG `<img>` tags on 2026-10-18; PNG icons are the intended source for all dashboard glyphs.

Validation: `npm test` 57/0/12, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Accessibility & focus polish (2026-10-18)

Zero logic changes — accessibility hardening only:

| Change | File | Detail |
|--------|------|--------|
| Birth date aria pairing | `src/index.html:202` | `aria-describedby="bdate-help"` on input + matching `id="bdate-help"` on help paragraph |
| Valid until aria pairing | `src/index.html:475` | `aria-describedby="valid-until-help"` on input + matching `id="valid-until-help"` on help paragraph |
| Resident ID aria pairing | `src/index.html:179` | Pre-existing `aria-describedby="resident-id-help"` — verified pairing with help paragraph |
| Focus management | `src/js/app.core.js` | Added `focusView()` method that scopes to the visible view container (offsetParent !== null, since all views coexist under x-show) and focuses the first focusable element: dashboard→.kiosk-tile, register→#resident-id, search→#search-input, settings→.hub-back, detail→.hub-back. Wired via `this.$watch('currentView', () => this.focusView())` in `init()`. |

Validation: `npm test` 57/0/12, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.
