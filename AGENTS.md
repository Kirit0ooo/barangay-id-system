# AGENTS.md — Barangay ID Registration System - Permanent Guidance

Applies to all sessions in this workspace.

## Planning & tracking
- Plan non-trivial work with `/plan` before editing; keep `/todos` current as work progresses.
- Break large efforts into small, reviewable tasks and mark them done only when verified.
- When every todo/sideroom-board item is done and green, clear the board so no item is left dangling. If any item is not done or any check is not green, return to that todo/sideroom item and execute it properly and robustly before declaring the work complete. Do not mark an item done unless its verification actually passed.

## Editing discipline
- Make small, reviewable edits; never reformat unrelated code.
- Prefer reading existing files and patterns over inventing new ones.
- Use checkpoints (`/rewind` available) before risky refactors so we can recover.

## Quality gates
- Run the project's tests/lint/typecheck after changes; do not declare done while checks are red.

## Communication
- Ask a clarifying question when a requirement is ambiguous instead of guessing.

## Git & branching
- Branch before every change. Never commit directly to `master`.
  - Name format: `<type>/<short-slug>` — `fix/launcher-ensure-datadir`, `feat/kiosk-stat-card`, `docs/agents-md-branching`.
  - `fix` for bug fixes, `feat` for new behavior, `docs` for documentation only, `chore` for tooling/infra.
- Branch from `master`, keep it short-lived, and delete it after merge.
- Do not rebase or force-push shared branches. Merge completed feature branches into `master` with `--no-ff`.
- Before merging, run `npm test` and `npm run check:css`; both must be green.
- If a change spans multiple files, one branch per logical change. Do not bundle unrelated tweaks into a single branch.

This file instructs every AI agent working in this project. Read it before making any change.

## ⚠️ PRIMARY RULE — Directory confinement (absolute)

**All data, tests, temporary files, cache, build artifacts, logs, generated files, runtime state, `.env` backups, and anything else related to this program must be stored inside this directory folder:**

```
/mnt/d/Barangay/barangay-id-system/
```

- Nothing related to this program may be relocated, copied, or written to any unrelated system location (e.g. `/tmp`, `~/`, `AppData`, TEMP, or any drive outside this folder).
- `.env` backups and any other secret/config backups must live inside this directory (e.g. `config/` or `data/`), never outside it.
- Generated artifacts (caches, logs, temp files) must remain inside `tmp/`, `logs/`, `backend/src/target/`, `data/`, or `Documents/` — all under this project root — and must not leak elsewhere.
- Do not delete, rename, move, or rewrite any code or project file until you have completed an initial assessment and architectural understanding of the entire program.

---

## How the program works (annotated)

This is an offline-first Tauri v1 desktop application for barangay ID registration. It runs entirely on the local machine (Windows-primary; WSL/Linux for CI only).

### Data flow
```
Frontend (Alpine SPA, frontend/src/js/app/app.core.js → view modules → frontend/src/js/app/app.js)
      → frontend/src/js/services/api.js (Tauri invoke wrappers)
      → Tauri commands (backend/src/src/*.rs)
      → sqlx SQLite (data/residents.db)
      → JSON response back to frontend
```
- All IPC is asynchronous and parameterized (no string-interpolated SQL).
- Photos are stored base64→JPEG under `data/photos/`; max 6 MB binary (~8 MB base64).
- Bulk ID printing is HTML-only: fills `frontend/src/card-template.html` per row via a hidden iframe (browser Print dialog / Save-as-PDF optional). **No PDF generation exists anywhere** (removed in a prior phase).
- Data is isolated to the project tree: `data/residents.db`, `data/photos/`, `data/backups/`, `config/civic.json`, `Documents/Excel/`, `logs/`, `tmp/`. Never AppData/TEMP.

### Backend commands (backend/src/src/main.rs registers these)
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
| Crypto module | `backend/src/src/crypto.rs` | AES-256-GCM encrypt/decrypt, key file ops, dir-level encrypt/decrypt, unit tests |
| Tauri commands | `backend/src/src/encryption.rs` | 6 commands: generate key, status, encrypt/decrypt DB, encrypt/decrypt photos |
| Key storage | `.crypto-key/key.bin` | 32-byte random key, created on first `encryption_generate_key`, never rotated |
| File format | `[magic(8) "BRGYENC1"][version(1)][nonce(12)][ciphertext+GCM tag]` | 9-byte header + nonce + ciphertext+GCM tag, random nonce per file |
| API wrappers | `frontend/src/js/services/api.js` | `encryptionGenerateKey`, `encryptionStatus`, `encryptionEncryptDatabase`, `encryptionDecryptDatabase`, `encryptionEncryptPhotos`, `encryptionDecryptPhotos` |
| Settings methods | `frontend/src/js/app/settings/app.settings.js` | UI methods calling each API wrapper with toast feedback |
| Photo decrypt-on-read | `backend/src/src/residents.rs` `resolve_photo_bytes` | Wired `crate::crypto::load_key` + `CryptoKey::decrypt_or_plaintext` (was `#[allow(dead_code)]`, zero callers) so `get_photo_base64`/`get_resident_photo`/`id_card` print decrypted photos instead of broken images after `encryption_encrypt_photos`. Falls back to raw bytes when no key exists. |

**Threat model:** Protects data from being read by anyone who copies files without also copying `.crypto-key/`. Does NOT protect against an attacker who has both the key file and the data (key sits beside data by design per operator request).

**Known boundary gaps:** Pre-existing plaintext backups (created before encryption switch) remain readable. Photos encrypted individually; any new photos added while DB is unencrypted remain plaintext until explicitly encrypted.

### Frontend modules
| File | Role |
|------|------|
| `frontend/src/index.html` | Hub-and-spoke welcome dashboard (service tiles) + register/search/settings/detail views; `barangay.ico` favicon + `barangay.png` nav logo |
| `frontend/src/js/app/app.core.js` | `window.appState` shared state + core methods (init, theme, civic config, settings, toast); loaded first |
| `frontend/src/js/app/residents/app.register.js` | Register view: ID generation, photo upload, save/update, clear form, edit mode |
| `frontend/src/js/app/residents/app.search.js` | Search view: searchResidents, applySearchCategory, pager, fetchAllResidentsPaged |
| `frontend/src/js/app/settings/app.settings.js` | Settings view: backup, restore, export, import, photo GC, optimize |
| `frontend/src/js/app/residents/app.detail.js` | Detail view: view/close, delete, renew, card payload, print, preview, bulk print |
| `frontend/src/js/app/dashboard/app.dashboard.js` | Dashboard view: status label, gauge arcs, expiry countdown |
| `frontend/src/js/app/app.js` | Entry point (~20 lines): returns `window.appState`; SPA logic is distributed across the six view modules loaded before it |
| `frontend/src/js/shared/helpers.js` | Pure helpers (dates, contacts, purok options, `residentStatus`, `parseValidUntil`, `todayISO`); dependency-free so tests can load it |
| `frontend/src/js/services/api.js` | Tauri invoke wrappers for all commands |
| `frontend/src/js/services/logger.js` / `bootstrap/verbose.js` | Frontend logging to localStorage buffer + backend `log_frontend` invoke; `verbose.js` bootstraps `BARANGAY_VERBOSE` before `logger.js` |
| `frontend/src/css/style.css` | Scoped components; kiosk dashboard styles (`.kiosk-head`, `.kiosk-stats`, `.kiosk-stat`, `.kiosk-tiles`, `.kiosk-tile`, `.kiosk-status-group`); design tokens + semantic aliases in `:root` (see repo-root `DESIGN.md`) |
| `frontend/src/css/tailwind.css` | Prebuilt static utilities; freshness-guarded by `tools/check-css.mjs` (fingerprint over index.html + all frontend/src/js/*.js except vendored libs, derived at runtime) |
| `frontend/src/card-template.html` | CR100 print template; `textContent` for text + image allowlist (`SAFE_IMG` drops `javascript:`) |

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
- Card-template logos (seal/emblem): unchanged in `frontend/src/card-template.html`

---

## Annotated fixes already applied (do not regress)

| Fix | File | What changed | Why |
|-----|------|--------------|-----|
| **D5 — Search table-leak** | `backend/src/src/residents.rs:211-221` + `:1159` | Extracted pure `fn digit_search_pattern(query) -> Option<String>` returning `None` for digit-free (name-only) queries; bound as SQL `NULL` (falsy) so the contact `OR` branch drops out instead of matching every row via `"%%"` | Name-only queries were leaking the whole table (capped at SEARCH_RESULT_LIMIT=2000) |
| **D6 — Status precedence** | `frontend/src/js/helpers.js` `residentStatus(r, isExpired)` | Operator remarks (LOST/RENEW/DAMAGE) now outrank generic expiry; order: Inactive → remarks → Expired → ACTIVE | An expired card with an operator-set LOST/RENEW/DAMAGE was rendering as merely "Expired" |
| **P2 — Explicit column list** | `backend/src/src/residents.rs` `RESIDENT_COLUMNS` | All queries list columns explicitly instead of `SELECT *` | Schema drift (future DROP of `last_pdf_*`) is caught at compile-time; legacy `email` column never selected |
| **P0 — FK orphan guard** | `backend/src/src/residents.rs` `delete_resident` | Before soft-deleting, checks `emergency_contact_resident_id` and legacy `emergency_contact_number` references | Prevents deactivating a resident still referenced as an emergency contact |
| **P2-7 — photo_gc on schedule** | `backend/src/src/scheduled.rs` `scheduled_backup` | Added `photo_gc(state.clone(), Some(!dry_run)).await` after retention block so orphan photos are auto-deleted after each scheduled backup | Photo dir never accumulates unreferenced files between manual GC runs |
| **P2-7b — photo_gc_dry_run toggle** | `backend/src/src/config.rs` `CivicConfig` + `set_civic_config` + `load_civic_config`; `backend/src/src/scheduled.rs`; `config/civic.json` | Added `#[serde(default)] photo_gc_dry_run: bool` field (default false). `scheduled_backup` reads config and passes `!dry_run` to `photo_gc` so dry_run=true scans+logs only. `set_civic_config` accepts the bool via partial JSON update. Tests added. | Operators who prefer manual-only GC can set `photo_gc_dry_run: true` in config/civic.json without hand-editing |
| **P2-8 — DOB sanity cap** | `backend/src/src/residents.rs` `normalize_birth_date` | Rejects birth dates more than 120 years before today, plus existing future-date rejection | A living person cannot be over 120 at registration; prevents data-entry errors |
| **T1 — Remove test/dummy data** | `data/residents.db`, `data/photos/`, `data/backups/` | Deleted all 3 dummy resident rows (BRGY-2026-00001/002/003 with TEST/TEST/TEST and biblical names), removed all orphan photos, removed plaintext backups containing dummy data | Eliminated test fixtures from live database; all remaining residents are real registrations |

---

## Dashboard kiosk remodel (2026-10-18)

Remodeled the Welcome dashboard into a kiosk layout (`frontend/src/index.html` + `frontend/src/css/style.css`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Layout | `frontend/src/index.html:42-174` | Replaced `<details class="breakdown">` collapsible section with always-visible kiosk dashboard; no collapsible/expandable sections |
| Header | `frontend/src/index.html` | `.kiosk-head` header with serif `DASHBOARD` title (`--font-display`) |
| Stat row | `frontend/src/index.html` | `.kiosk-section--registry` wraps `.kiosk-stats` grid with 3 stat cards (Total Residents / By Gender / Registrations This Year); `.kiosk-section__head` header with title + live total badge; cards have `.kiosk-stat:hover` lift effect; gender card shows inline bar chart + male/female ratio |
| Card status | `frontend/src/index.html` | `.kiosk-section--status` wraps `.kiosk-status-group` with 4 color-coded status cards (Active/Renew/Lost/Damage); `.kiosk-section__head` header; grid gap-12 for comfortable card separation |
| Section CSS | `frontend/src/css/style.css` | `.kiosk-section`, `.kiosk-section__head`, `.kiosk-section__title` rules; `.kiosk-stat` padding 22px/20px, 2px border, radius 16px, hover lift with border-color transition + box-shadow |
| Gap utilities | `frontend/src/css/tailwind.css` | Added `gap-8` (2rem), `gap-10` (2.5rem), `gap-12` (3rem); card status grid uses `gap-12` |
| Gauge | removed | The SVG balance gauge and `stat-hero` layout were deleted outright (no `gaugeArcs()`/`gaugeGap()` in `app.dashboard.js`); gender split is now the `.kiosk-stat__bar` inline bar + Male/Female counts |
| Service tiles | `frontend/src/index.html` | `.kiosk-tiles` grid with oversized touch tiles (`.kiosk-tile`, min-height 120px, 72px icons, 40px arrow) |
| Icons | `frontend/src/index.html` | PNG glyphs under `src/icons/` referenced via `<img src="icons/*.png" />` (person, users, calendar, check, refresh, badge, alert, document, search, settings). Sizes via `width`/`height` CSS (24px stat, 32px tile) — fixed-color PNGs, no `currentColor` theming. |
| CSS | `frontend/src/css/style.css` | `.kiosk-stat__icon img` and `.kiosk-tile__icon img` use `width`/`height`/`object-fit:contain` for fixed sizing — no `font-size`/`line-height` Unicode scaling. |

Validation: `npm test` 57/0/6, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## UI/UX consistency pass (2026-10-19)

Visual-consistency polish across all five views — CSS only, zero logic changes. Design system persisted to repo-root `DESIGN.md` (tokens, states, do/don'ts); future UI work must follow it.

| Change | File | Detail |
|--------|------|--------|
| Dead CSS removal | `frontend/src/css/style.css` | Removed ~300 lines of unreferenced rules: `.hub-tile*`/`.hub-grid`/`.hub-pill` (old hub), `.dashboard-head/tiles/tile*`, `.dashboard-stats`, `.stat-card--secondary/ratio/male/female/total/gender/year`, `.stat-card__bar*/ratio*`, `.stat-card-group*`, `.breakdown*`, `.stat-accent`, `.kiosk-head__gear`, `.view-sub`, `.recent-card` leftovers |
| Duplicate merge | `frontend/src/css/style.css` | `.empty-state*` (two competing defs → one), `.action-chip` (two defs → one), `.form-field__label/help/req` (tail dupes removed; help 11→12px to match the winning dupe), duplicate `body{}` block, duplicate `.hub-back:hover`, duplicate `.kiosk-stat:hover` |
| Kiosk value sizing | `frontend/src/css/style.css` | Added missing base `.kiosk-stat__value` (32px/800/tabular-nums) and `.kiosk-stat__meta` (12px muted) — desktop stat numbers previously inherited body size; mobile 40px rule unchanged |
| Token aliases | `frontend/src/css/style.css` | Added `--color-accent-ink` (dark gold for AA text/fills on light) + `--color-damage`; kiosk lost/damage hover borders and female ratio label/value/bar now use tokens instead of raw hex — fixes gold-on-white contrast |
| Required-asterisk contrast | `frontend/src/css/style.css` | `.form-field__req` keeps `--color-error` (the gold dupe that overrode it was removed); red required marks now AA on white |
| `:root` tidy | `frontend/src/css/style.css` | Fixed `--font-display`/`--text-xs` indentation; `.kiosk-head__title` double `letter-spacing` deduped |
| Kiosk tile arrows | `frontend/src/index.html` + `frontend/src/css/style.css` | Replaced the `→` text char with `src/icons/arrow.svg` `<img>` (20px); hover flips glyph white via `filter:brightness(0) invert(1)` since `currentColor` inside an `<img>` can't inherit page color |

Validation: `npm test` 57/0/6, `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Standard project coding workflows

### Run tests
```
npm test
```
- 6 suites: smoke, regression, golden-card, bulk-progress, logic, structure
- Must be **57 pass / 0 fail** before declaring any change complete.
- `tests/logic.test.js` evaluates `frontend/src/js/shared/helpers.js` in a Function scope (real source, not a copy).
- `tests/regression.test.js` reads project files by path — if you move a markdown/doc file, update the reference there too.

### CSS freshness check
```
npm run check:css
```
- `tools/check-css.mjs` fingerprints `index.html` + every `frontend/src/js/*.js` (vendored libs excluded, derived at runtime); must report **CSS fresh**.
- If you change `index.html` or any `frontend/src/js/*.js` markup/classes, run `npm run stamp:css` to update `tools/css-fingerprint.txt` (only if you intentionally changed CSS-relevant markup; zero new CSS classes → just restamp).

### Rust backend (CI-only locally)
- `cargo check`, `cargo test`, `cargo clippy` and `cargo doc` **cannot run on this host** (documented env limit, re-verified 2026-11-06). Host target: `soup2-sys` build script fails on missing `libsoup-2.4` (and `webkit2gtk-4.0` is also absent, though `gtk+-3.0` 3.24.52 and `glib-2.0` 2.88.4 are present). `--target x86_64-pc-windows-msvc`: `libsqlite3-sys` fails looking for the MSVC librarian `lib.exe`. Rust compilation and tests stay CI-only via `.github/workflows/ci.yml` (`cargo test --manifest-path backend/src/Cargo.toml`) or a Windows-side `build.bat`.
- `cargo fmt --all -- --check` **does** run on this host (rustfmt needs no native libraries) and is **clean** as of 2026-11-06: exit 0 with zero output, all 11 files passing individually. This supersedes the earlier note that 5 files were fmt-red. Keep it clean, and never bulk-reformat unrelated files.
- `cargo metadata --locked` **does** run and pass on this host (manifest and lockfile integrity). Use it when the compiler cannot run.
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
| Source (backend) | `backend/src/src/` |
| Tests | `tests/` |
| Build scripts / tools | `tools/` |
| Config | `config/civic.json` |
| User database | `data/residents.db` |
| Encryption key | `.crypto-key/key.bin` — 32-byte AES-256 key; sensitive, never commit to version control |
| Photos | `data/photos/` |
| Backups | `data/backups/` |
| Logs | `logs/` |
| Temp / cache | `tmp/` |
| Rust build output | `backend/src/target/` |
| Gathered docs | `gathered-md/` |
| Project docs (keep in place) | `README.md`, `Documents/README.md`, `Documents/Verify/README.md` |
| Agent instructions | `AGENTS.md` (this file) |
| Design system | `DESIGN.md` (tokens, states, do/don'ts — read before styling anything) |

## Keeping AGENTS.md current

This file is a living document. Whenever the program changes, update AGENTS.md in the same session:

- New commands, modules, or file locations → add to the tables above.
- New fixes → add a row to the "Annotated fixes" table.
- New workflows or commands → add to "Standard project coding workflows".
- Directory rule or confinement scope changes → edit the PRIMARY RULE at the top.
- UI polish → update "Frontend modules" table and note changed CSS classes here.

Never let AGENTS.md drift from the actual code; an outdated AGENTS.md misleads the next agent.

## Register polish (2026-10-16)

Applied visual polish to the "Register New Resident" view (`frontend/src/index.html` + `frontend/src/css/style.css`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Header polish | `frontend/src/index.html:146-158` | `register-head` class + `register-eyebrow` + `register-sub` hint; `register-clear` buttons; flex-1 title column |
| Section animation | `frontend/src/index.html:196,222,239` | Added `register-section` class to Personal Info / Address / Emergency sections so CSS entry animation (`register-enter`) fires |
| Section icons | `frontend/src/index.html` | `.register-section__icon` PNG glyphs beside section titles (person.png → Personal Information, address.png → Address, contact.png → Emergency Contact, card_stats.png → Card Status; ID.png in the 1×1 photo well). Number discs (1/2/3) and inline SVGs retired 2026-10-19; Card Status gained a proper `.form-section__head`. |
| Required badge | `frontend/src/index.html:205` | `* required` badge: `hidden sm:block` (hidden on mobile to avoid crowding), `text-slate-500` contrast, `whitespace-nowrap` |
| ID row layout | `frontend/src/index.html:183` | Wrapped Resident ID input + Generate button in `register-idrow` for stacked-mobile / side-by-side-sm layout |
| Actions bar | `frontend/src/index.html:273-279` | Added `register-actions__note` hint + `register-actions__btns` wrapper for sticky mobile actions |
| CSS | `frontend/src/css/style.css` | Added `.register-section__icon` rule; added `.register-photo__preview`, `.register-photo__hint`, `.register-remove` photo-area rules; wired up existing `.register-head`, `.register-section`, `.register-idrow`, `.register-actions__btns`, `.register-photo__cta` rules so animations, sticky actions, photo states, and remove button all render |

Validation: `npm test` 57/0/6, `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Welcome dashboard polish (2026-10-16)

Applied frontend-create principles to the welcome hub (`frontend/src/index.html:41-66` + `frontend/src/css/style.css:1022-1130`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Header layout | `frontend/src/index.html:41-48` | Flex row: Welcome heading + motto on left, `hub-pill` Dashboard chip on right; removed `sm:text-[28px]` from h2 to match other pages' plain `text-2xl`; added `.hub-motto` CSS rule matching `register-sub` styling |
| Hub tiles | `frontend/src/index.html:49-66` | Per-tile icon accent colors (navy/gold), added `hub-tile__arrow` → indicator with hover reveal, `hub-tile__steps` moved after arrow on register tile, per-tile `hub-tile__icon--register/search/settings` classes |
| CSS | `frontend/src/css/style.css` | Added `.hub-pill` civic chip, per-tile icon background colors, `.hub-tile__arrow` hover-reveal animation, fixed `margin-top:auto` conflict between `.hub-tile__steps` and `.hub-tile__chips`, added visible bevel to `.hub-tile` (gradient white→#F7F9FC background so white ::before top/left highlight shows, :active pressed state with translateY + shadow reduction), added `.hub-motto` subtitle rule matching `register-sub` styling |

## Dashboard stat cards & gauge polish (2026-10-16)

Applied frontend-create principles to stat cards, stat hero, and gauge (`frontend/src/index.html:123-148` + `frontend/src/css/style.css:244-367`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Gauge color tokens | `frontend/src/index.html` | Replaced hardcoded `#eef2f7`/`#cbd5e1`/`#2563eb`/`#ec4899` gauge SVG colors with civic tokens `--border`/`--border-strong`/`--color-primary`/`--color-accent` |
| Gauge dot/fill tokens | `frontend/src/css/style.css` | Replaced hardcoded `#2563eb`/`#ec4899` gauge dot and fill colors with `--color-primary`/`--color-accent` and matching shadow RGBA values |
| Gauge pct text colors | `frontend/src/css/style.css` | Aligned `.gauge-chip--male .gauge-chip__pct` → `--color-primary` (navy) and `.gauge-chip--female .gauge-chip__pct` → `--color-accent` (gold), fixing the previous blue/orange mismatch with arcs |
| Stat card group label | `frontend/src/index.html:123-124` | Wrapped four status cards in `.stat-card-group` with `.stat-card-group__label` "Card Status" eyebrow |
| Stat card hover | `frontend/src/css/style.css` | Added `filter:brightness(1.15)` on icon hover for all four status card variants |
| CSS | `frontend/src/css/style.css` | Added `.stat-card-group` and `.stat-card-group__label` rules |

Validation: `npm test` 57/0/6, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Section header uniformity (all pages) (2026-10-16)

Made all four page section headers structurally uniform (`frontend/src/index.html:41-66,157-170,298-304,398-404,524-532`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Back button position | `frontend/src/index.html:526-530` | Detail page: moved `hub-back` from right side to left side, matching Register/Search/Settings |
| Redundant class removed | `frontend/src/index.html:167` | Register page: removed misused `register-clear` class from edit-mode Back button (was not a Clear button) |
| Consistent classes | All pages | All headers now use `hub-back` button class + `view-title` h2 class + hub-back on left |

Validation: `npm test` 57/0/6, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Frontend modularization (2026-10-16)

Split monolithic `frontend/src/js/app.js` (1262 lines) into 6 focused classic-script modules, zero runtime behavior change:

| File | Lines | Role |
|------|-------|------|
| `frontend/src/js/app/app.core.js` | ~190 | `window.appState` shared state + core methods (init, theme, civic config, settings, toast) |
| `frontend/src/js/app/residents/app.register.js` | ~130 | Register view: ID generation, photo upload, save/update, clear form, edit mode |
| `frontend/src/js/app/residents/app.search.js` | ~80 | Search view: searchResidents, applySearchCategory, pager, fetchAllResidentsPaged |
| `frontend/src/js/app/settings/app.settings.js` | ~100 | Settings view: backup, restore, export, import, photo GC, optimize |
| `frontend/src/js/app/residents/app.detail.js` | ~260 | Detail view: view/close, delete, renew, card payload, print, preview, bulk print |
| `frontend/src/js/app/dashboard/app.dashboard.js` | ~70 | Dashboard view: status label, gauge arcs, expiry countdown |
| `frontend/src/js/app/app.js` | ~20 | Entry point: `function app() { return window.appState; }` + Alpine bootstrap |

**Design decisions:**
- `window.appState` (not `window.app`) — avoids collision where `function app()` would overwrite the state object at script load time
- Classic `<script>` (no ES modules) — `x-data="app()"` resolves the global function; no `type="module"` conversion needed, no CORS issues under Tauri's custom protocol
- Load order matters: `app.core.js` first, then view modules, then `app.js` entry point
- Tests updated to concatenate all app modules for path-based string checks (tests/bulk-progress.test.js, tests/golden-card.test.js, tests/regression.test.js, tests/structure.test.js)

Validation: `npm test` 57/0/6, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities, `node --check` clean on all 7 JS files.

## Launcher alignment fixes (2026-10-17)

Applied robustness fixes to the Windows PowerShell/batch launchers, zero logic changes:

| Fix | File | Detail |
|-----|------|--------|
| Inline fallbacks | `run.ps1`, `build.ps1` | When `tools/launcher-common.ps1` is missing, define stub `Set-PortableEnvPins`/`Restore-EnvPins`/`Invoke-SafeNative` so unconditional calls never hard-crash under `Set-StrictMode -Version Latest` |
| Flag docs | `build.bat` | Added `-EnableVerboseLogging` to the documented forwarded-flags header and `:NOPS` echo line (build.ps1 already accepts it via `%*`) |
| MSVC gate | `build.ps1` | vswhere.exe existence alone no longer passes — now verifies VC.Tools.x86.x64 toolset is actually installed, matching run.ps1's strict check |
| WiX gate | `build.ps1` | ~~Checked `WiX Toolset v3.11\bin\candle.exe` path first; `candle.exe` on PATH accepted only if `-version` reported v3.11~~ **superseded 2026-11-05** - WiX/NSIS are now warn-only because Tauri v1 provisions its own toolset (see "Installer toolchain provisioning" below) |
| Stale comment | `tools/launcher-common.ps1` | `Ensure-DataDir` comment corrected: both run.ps1 and build.ps1 call it (was "run.ps1 only") |
| -Logs narrative | `run.ps1` | `.SYNOPSIS` reworded from "replaces view-logs.bat" to "supersedes view-logs.bat logic; wrapper remains" since view-logs.bat still exists as a live Explorer wrapper |

Validation: `npm test` 57/0/6, `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Installer toolchain provisioning (2026-11-05)

`build.bat` refused to build on a machine with no system-wide WiX/NSIS. Root cause: Tauri v1 **never uses a system WiX/NSIS install** - it downloads and verifies its own copies - so the prereq gate was stricter than the build actually needs.

| Change | File | Detail |
|--------|------|--------|
| Tool cache moved in-project | `backend/src/tauri.conf.json` | Added `"useLocalToolsDir": true` under `tauri.bundle`, so Tauri caches WiX + NSIS in `backend/src/target/.tauri/` (`WixTools314/`, `NSIS/`) instead of `%LOCALAPPDATA%` - keeps the portable/confinement contract |
| Gate relaxed | `build.ps1` | WiX and NSIS `$Prereqs` rows changed `Level = 'required'` to `'warn'`; each test now checks Tauri's in-project cache first, then a system install (`candle.exe` / `makensis.exe` are still detected when present) |
| Warn line wording | `build.ps1` | Warn branch prints `optional: <name> not found - <fix>` (was `missing (warn-only):`), so an auto-provisioned tool no longer reads as a hard failure |
| `-Help` prereq line | `build.ps1` | Now lists only the hard requirements and marks WiX/NSIS/rustup as optional/auto-downloaded |
| MSI target deferred | `backend/src/tauri.conf.json` | ~~`tauri.bundle.targets` set to `["nsis"]` (from `["msi","nsis"]`) because the per-user WiX template could not link (ICE38/ICE64)~~ **SUPERSEDED 2026-11-07** - the MSI leg was not carried into the shipped config; `targets` stays `["nsis"]` (NSIS-only) |
| MSI assertion repointed | `tests/regression.test.js` | ~~asserted NSIS-only, so a bare re-add of `"msi"` would fail loudly~~ **SUPERSEDED 2026-11-07** - the check locks NSIS-only (`tests/regression.test.js:287`) and forbids any `.ico` in `bundle.resources` (`:289`) |
| Launcher help text | `build.ps1` | `.SYNOPSIS`, `.DESCRIPTION` and the closing summary lines now read `nsis` instead of `msi/ + nsis/` |

Facts verified against the shipped CLI (`node_modules/@tauri-apps/cli-win32-x64-msvc/cli.win32-x64-msvc.node`, v1.6.3):

- WiX: downloads `wix3141rtm/wix314-binaries.zip` (sha256 `6ac824e1...`) and expects `candle.exe`, `light.exe`, `darice.cub`, `wix.dll`, `WixUIExtension.dll`, `WixUtilExtension.dll`.
- NSIS: downloads `nsis-3.zip` (sha1 `057e83c7...`) plus `nsis_tauri_utils.dll` v0.4.1, then runs `<tools>\NSIS\makensis.exe`.
- Cache base with `useLocalToolsDir: true` is `backend/src\target\.tauri\` (`WixTools314\`, `NSIS\`); the default per-user cache is `%LOCALAPPDATA%\tauri\`. There is **no PATH/system fallback for `makensis.exe`** on Windows.
- Network is still needed on the first bundle (WiX + NSIS ~40 MB) and for the WebView2 offline installer (~150 MB).

**RESOLVED 2026-11-07 - auto-version aborted before the build (2026-11-06):** both `build.ps1` runs that day stopped at the version bump with `ERROR: Auto-version: package-lock.json still pins <old> (pattern drift) - update build.ps1 bump stages` (`logs/build-20261106-002742.log:33-34` = `1.0.1 -> 1.0.2`, `logs/build-20261106-091239.log:33-34` = `1.0.2 -> 1.0.3`), so `tauri build` never started. Cause: the `$bumpStages` pattern for the lockfile root line was anchored `"$`, but that line ends `"version": "1.0.1",` - the trailing comma meant it never matched; the per-stage guard then threw on stage 2 of 6, so stages 3-6 (`package-lock.json` packages entry, `tauri.conf.json`, `Cargo.toml`, `Cargo.lock`) never ran. Two runs bumped `package.json` only, leaving `package.json` at `1.0.3` while the other four pinned `1.0.1`. `run.ps1` was unaffected because it does not bump.

**Launcher restructure (2026-11-07) - see "Build launcher rework" below.** The next real `build.bat` run picks the highest version it finds (`1.0.3`) as the base, syncs every manifest to it and bumps once, landing all five slots on **`1.0.4`**. Do not hand-edit the split manifests: that would burn a version number and the script repairs them itself.

### MSI leg: per-user WiX template failed ICE validation (2026-11-05) - still deferred

The MSI target was deferred on 2026-11-05 because the per-user WiX template could not link (ICE38/ICE64). The deferred state was kept as a failure record, but MSI was never actually re-enabled or built. tauri.bundle.targets stays ["nsis"] (NSIS-only).

| Blocker | File | Detail |
|---------|------|--------|
| Stale high-integrity artifact | `backend/src/target/x86_64-pc-windows-msvc/release/wix/x64/output.msi` | A 225 MB `output.msi` written by an **elevated** build on 2026-09-17 carries `Mandatory Label\High Mandatory Level:(I)(NW)`, so a normal (medium-integrity) build can neither overwrite, rename nor delete it. Tauri's MSI leg then dies with a bare `Access is denied. (os error 5)` immediately after `Info Target: x64`. Non-elevated workaround used: rename the containing folder to `wix\x64-stale-20260917`. Deleting the file itself needs an Administrator shell; nothing else under `target/` is affected |
| WiX template was a broken stub | `backend/src/wix/main.wxs` | Commit `ae3aff9` (2026-10-16) replaced the project's 321-line per-user template with a 23-line stub missing the `<Product>` element, so `candle.exe` failed with 5x `CNDL0005`. Restored the `bff82ac` (2026-09-16) version: per-user install into `LocalAppDataFolder`, automatic desktop shortcut, HKCU registry keys |
| MSI link fails ICE38/ICE64 | `backend/src/wix/main.wxs` | With the template restored `candle.exe` compiles, but `light.exe` exits 204: `ICE38` (component `Path`, plus the auto-generated resource component `Ib0654e57...`, use a *file* KeyPath while installing to the user profile) and `ICE64` (profile directory `I561cff89...`, created by `resources: ["../barangay.ico"]`, is absent from the RemoveFile table). Tauri's stock template installs per-machine into `$(var.PlatformProgramFilesFolder)`, so these ICEs never fire there. Patching the template alone cannot clear the resource ICEs because Tauri generates those components - the `resources` entry must also go |

MSI status as shipped: the `Path` component got an HKCU `RegistryValue` KeyPath and its `<File>` was demoted to `KeyPath="no"` (clears ICE38), and `../barangay.ico` was dropped from `bundle.resources` (clears the ICE64 profile-directory component), but the MSI leg was not carried into the shipped config. `tauri.bundle.targets` stays `["nsis"]` (NSIS-only). The earlier plan to add `RemoveFolder`/`RemoveFile` entries was not needed once the resource entry was removed.

## MSI leg status (2026-11-07)

The MSI target was never re-enabled or built. NSIS-only is the shipped config; no frontend, backend, database or config-value change.



## Build launcher rework



`build.ps1` and `build.bat` only. No frontend, backend, database or config change.

| Fix | File | Detail |
|-----|------|--------|
| Auto-version rewritten | `build.ps1` | `$bumpStages` (old-version-anchored patterns + a write-then-verify loop) replaced by a `$manifests` table of `File`/`Label`/`Read`/`Write` patterns. Each version slot is matched by **shape**, never by the previous number, so a drifted file is repaired instead of silently skipped; the lockfile's trailing comma no longer matters. `Cargo.toml` is anchored `(?m)^version = ` (column 0) so a dependency pin can never be rewritten. Three phases: resolve all six slots (highest version is the base), verify every slot is writable, then write and re-read. A failure therefore cannot leave a half-bumped tree - the old code wrote `package.json` and threw on `package-lock.json`. Failure messages now name the file and the version actually found (the old message reported the *expected* version, so `logs/build-20261106-091239.log:34` said "still pins 1.0.2" while the file really pinned `1.0.1`) |
| Version work extracted | `build.ps1` | The six-slot table and its three phases live in `function Invoke-PatchVersionBump [-Root]`, defined above the `-Audit` block, so the main `try` reads as a sequence (clean -> bump -> npm -> tauri build) instead of carrying ~70 lines inline. The `-NoBump` test and the `$PSCmdlet.ShouldProcess` gate deliberately stayed at the call site: `$PSCmdlet` in script scope is what makes `-WhatIf` keep working, and an advanced helper would have changed that `$WhatIfPreference` plumbing. One definition, one call site |
| False WiX/NSIS warnings | `build.ps1` | The two warn-only `$Prereqs` probes looked in `backend/src\target\.tauri-tools\tauriWixTools314\candle.exe` and `...\.tauri-tools\NSIS\makensis.exe`. Tauri's real cache (from `useLocalToolsDir: true`) is `backend/src\target\.tauri\WixTools314\candle.exe` and `...\.tauri\NSIS\makensis.exe`, both present on disk, so every build printed two bogus `optional: ... not found` lines. Paths corrected, system-install fallbacks kept, same wrong path fixed in the `-Help` text |
| `-Help` crashed | `build.ps1` | The help here-string was double-quoted and mentioned `$Prereqs`, which is not defined until later, so under `Set-StrictMode -Version Latest` `build.ps1 -Help` (and `build.bat -Help`) died with `ERROR: terminating error in build.ps1 - The variable '$Prereqs' cannot be retrieved because it has not been set`. Switched to a single-quoted here-string, so the text is literal |
| Single exit path | `build.bat` | Every branch now sets `RC` and goes to `:DONE`: the console code page the script changes (65001) is put back on the way out, and the real exit code always propagates. Added a `build.ps1` existence check. Dropped `EnableDelayedExpansion` (a `!` in the path or in forwarded arguments was being eaten). Both hard stops pause, because they are only reachable when the build cannot start and a double-clicked window would close before the reason showed |
| Batch gotcha | `build.bat` | Do not put a literal `)` inside an `if (...)` block: `Flags (-Help/-Audit/...)` closed the block early and the wrapper died with `need was unexpected at this time.` The flag list is now comma-separated |

Verified on 2026-11-07 with Windows PowerShell (`powershell.exe` interop) by slicing `Invoke-PatchVersionBump` and the `$Prereqs` table verbatim out of `build.ps1` into a throwaway harness under `tmp/` (deleted after the run - `tmp/` is gitignored), so the tests exercised the shipped code, not a retyped copy:

- split tree (the real current state: `package.json` 1.0.3, other four 1.0.1) -> `Auto-version: 1.0.3 -> 1.0.4`, and a line-level diff shows **exactly 6 changed lines**, one per declared slot (`package.json:4`, `package-lock.json:3`, `package-lock.json:9`, `backend/src/tauri.conf.json:11`, `backend/src/Cargo.toml:3`, `backend/src/Cargo.lock:154`), each moving to `1.0.4`. No dependency pin line changed
- synced tree (a copy of the case-1 result: all six slots 1.0.4) -> `1.0.4 -> 1.0.5`, all six slots `1.0.5`, and no `disagreed` warning
- unfindable slot (`tauri.conf.json` mangled to `ver-sion`) -> throws `no version slot in backend/src/tauri.conf.json - pattern drift`, exit 1, **all five files byte-identical**
- slot that resolves but cannot be rewritten (`tauri.conf.json` write pattern perturbed to be unmatchable) -> throws `cannot rewrite these slots`, exit 1, **all five files byte-identical**, so a half-bumped tree is unreachable even if only the read pattern were broken
- real repo manifests and `build.ps1` byte-identical after every test above
- `$Prereqs` probe now reports `ok` for WiX Toolset v3 and NSIS makensis (was `optional: ... not found`)
- `build.ps1 -Help` exit 0 with no error text; `build.bat -Help` exit 0 with output identical to the direct call; code page 437 restored after the wrapper exits; a bad flag exits 1 through the wrapper, matching a direct `build.ps1` call
- `build.ps1` parses with 0 errors (`[System.Management.Automation.Language.Parser]::ParseFile`), with exactly one definition and one call site of `Invoke-PatchVersionBump`, definition first
- `npm test` 57 pass / 0 fail / 6 suites. `npm run check:css` was not re-run after the `build.ps1` edit: its fingerprint covers `index.html` and `frontend/src/js/*.js`, none of which were touched

Correction to an earlier revision of this section: it claimed `"version": "1.6.3"` appeared 11 times in `Cargo.lock` and survived the bump. That number came from a compression summary and is wrong - `Cargo.lock` contains no `1.6.3` at all. The precision claim is now the line-level diff above, which is measured rather than recalled.

The MSI leg is remains deferred: as of 2026-11-07 `tauri.bundle.targets` is `["nsis"]` (NSIS-only), so a build produces the NSIS `.exe` installer . Rust compile/test/clippy/doc remain BLOCKED on this host (no `libsoup-2.4`, no MSVC `lib.exe`); CI is the compiler gate.

Also note `backend/src/target/.tauri-tools/probe/a.exe`, a leftover toolchain probe from an earlier session. Nothing reads it any more. Left in place because it sits under the gitignored build output tree.

## Settings page card redesign (2026-10-17)

Applied visual polish to the Settings page cards (`frontend/src/index.html` + `frontend/src/css/style.css`), zero logic changes:

| Change | File | Detail |
|--------|------|--------|
| Card wrapper | `frontend/src/index.html:398,424,474,488,507` | All 5 cards now use `.card card--section` with bordered surface; About card uses `.card--about` dark slate surface |
| Section headers | `frontend/src/index.html` | All 5 section heads now use `.section-head` with inline SVG icon + `.section-head__title`
| Action chip grid | `frontend/src/index.html:430-495` | Data & Maintenance (`.settings-grid` at :430), Printing (`.settings-grid` at :480), System Logs (`.settings-grid settings-grid--log` at :495); each chip uses `.settings-chip` with consistent icon+label layout |
| Settings actions | `frontend/src/index.html:411-416` | Barangay Information actions moved to `.settings-actions` with `.settings-btn--save/--reset/--theme` subclasses |
| Progress bar | `frontend/src/index.html:463` | Replaced undefined `bg-gray-200`/`bg-blue-600` with `.settings-progress__track` + `.settings-progress__fill` rules in style.css |
| Checkbox accent | `frontend/src/index.html:416,435` | Replaced undefined `text-blue-600 focus:ring-blue-500` with `.settings-checkbox` (accent-color var) — theme toggle at :416, backup toggle at :435 |
| Photo grid | `frontend/src/index.html:447` | Fixed bare `grid-cols-3` → `sm:grid-cols-3` so 3 columns render at ≥640px (1 column below) — bare `grid-cols-3` was inert since it is not in the hand-built tailwind.css subset |
| CSS | `frontend/src/css/style.css` | Added `.card--section`, `.card--about`, `.settings-grid`, `.settings-grid--log`, `.settings-chip`, `.settings-actions`, `.settings-progress__track`, `.settings-progress__fill`, `.settings-checkbox`, `.about-grid`, `.about-kv__key`, `.about-kv__value`, `.about-note`, `.section-head__icon`, `.settings-chip svg`, `.log-info`, `.log-info__label`, `.log-info__text` |

Validation: `npm test` 57/0/6, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Fonts & bundled type (2026-10-17)

All typefaces are bundled locally — no CDN, no external fonts:

| Font | File | Used in |
|------|------|----------|
| Inter (400/500/600/700) | `src/fonts/Inter-{Regular,Medium,SemiBold,Bold}.ttf` | SPA `--font-sans`, card `--font-body`/`--font-data` |
| Source Serif Pro (600) | `src/fonts/SourceSerifPro-SemiBold.ttf` | `--font-display` (dashboard title, card titles) |
| JetBrains Mono (400) | `src/fonts/JetBrainsMono-Regular.ttf` | `--font-mono` |
| OFL license text | `src/fonts/OFL.txt` | SIL OFL 1.1 notice for the three families |

- `frontend/src/css/style.css`: `@font-face` declarations reference `../fonts/` (style.css lives in `frontend/src/css/`).
- `frontend/src/card-template.html`: `@font-face` declarations reference `fonts/` (template lives at `src/` root).
- Replaced `"Work Sans"` with `"Inter"` in card-template.html header comment and README.md body-font line.
- CSP: `tauri.conf.json` has no `font-src`, so bundled fonts load under `default-src 'self'` (no CSP change needed).

Validation: `npm test` 57/0/6, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Icons (2026-10-17)

Dashboard glyphs are PNG files under `src/icons/` referenced via `<img src="icons/*.png" />` in `frontend/src/index.html` (person, users, calendar, check, refresh, badge, alert, document, search, settings) plus `src/icons/arrow.svg` for the three kiosk-tile arrows (`→` text char retired; fixed-color glyph, hover inverts it white via CSS `filter`). These are static image assets — they do not respond to `currentColor` theming but keep theming consistent across the navy/gold token system since each PNG is a fixed-color render. Five inline `<svg>` section-head icons on the Settings cards were replaced with PNG `<img>` tags on 2026-10-18; PNG icons are the intended source for all dashboard glyphs.

Validation: `npm test` 57/0/6, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Accessibility & focus polish (2026-10-18)

Zero logic changes — accessibility hardening only:

| Change | File | Detail |
|--------|------|--------|
| Birth date aria pairing | `frontend/src/index.html:202` | `aria-describedby="bdate-help"` on input + matching `id="bdate-help"` on help paragraph |
| Valid until aria pairing | `frontend/src/index.html:475` | `aria-describedby="valid-until-help"` on input + matching `id="valid-until-help"` on help paragraph |
| Resident ID aria pairing | `frontend/src/index.html:179` | Pre-existing `aria-describedby="resident-id-help"` — verified pairing with help paragraph |
| Focus management | `frontend/src/js/app/app.core.js` | Added `focusView()` method that scopes to the visible view container (offsetParent !== null, since all views coexist under x-show) and focuses the first focusable element: dashboard→.kiosk-tile, register→#resident-id, search→#search-input, settings→.hub-back, detail→.hub-back. Wired via `this.$watch('currentView', () => this.focusView())` in `init()`. |
| Nav orientation chip | `frontend/src/index.html` + `frontend/src/js/app/app.core.js` + `frontend/src/css/style.css` | Added `.nav-page` chip beside the brand block showing the current view (`.nav-page__dot` gold dot + `.nav-page__label`). Driven by `navPageTitle()` in app.core.js (edit mode narrows Register → "Edit Resident"); `aria-live="polite"` announces view changes. Tab strip stays hidden in kiosk mode. |

Validation: `npm test` 57/0/6, `npm run stamp:css` + `npm run check:css` CSS fresh, `npm audit` 0 vulnerabilities.

## Rust codebase quality pass (2026-11-06)

Documentation, error-handling and test work across `backend/src/src/` plus `tests/regression.test.js`. Frontend source, database, photos and configuration untouched. Full evidence, including blocked checks: `RUST_CODEBASE_QUALITY_REPORT.md`.

| Change | File | Detail |
|--------|------|--------|
| Guarded WAL checkpoint | `db.rs`, `backup.rs`, `encryption.rs`, `scheduled.rs` | New `pub async fn checkpoint_wal_truncate(pool: &SqlitePool) -> Result<i64, String>` (`db.rs:786`) replaces 5 sites that ran `PRAGMA wal_checkpoint(TRUNCATE)` under `let _ = ...`. That pragma always succeeds and reports busy in its result row, so the old code could copy or encrypt a `residents.db` missing every transaction still held in the WAL, and report success. The helper reads the `busy` column and returns an error; `backup_database:46` (x1), `restore_database:173` (x2), `encryption_encrypt_database` and `scheduled_backup:83` now propagate with `?`. `scheduled_backup` records `checkpointed N frame(s)`. Deliberate behaviour change: a busy checkpoint now fails the operation with a retry message instead of writing a possibly-incomplete file |
| Export header guard | `export.rs:234` | `assert_eq!(headers.len(), widths.len())` (M-2) is now a returned `anyhow` error, so a header/width desync cannot abort the app during an operator's export |
| Char-safe log truncation | `logger.rs` | `clean_msg.truncate(8000)` panicked when a multi-byte character straddled byte 8000. Extracted `sanitize_log_message` (walks back to `is_char_boundary` before truncating) and `sanitize_log_level`; constants `MAX_LOG_MESSAGE_BYTES = 8000`, `MAX_LOG_LEVEL_CHARS = 16` |
| Date parse panic removed | `residents.rs:263` | `parse_any_date` returns `Result<chrono::NaiveDate, String>`; `normalize_birth_date:298` and `normalize_valid_until:285` use the typed value instead of re-parsing with `.expect("helper emits ISO")`. The helper was briefly named `parse_any_date_to_iso`; renamed during the post-implementation review because the return type is a `NaiveDate`, not an ISO string |
| Dead code deleted | `residents.rs` | Removed `check_emergency_fk_pool` (pool-based emergency-contact FK check with zero callers; the live guard is the `AppState`-based check in `delete_resident`). Verified no remaining reference in `src/` |
| `allow(dead_code)` corrected | `crypto.rs:136` | Removed from `decrypt_or_plaintext`; its live caller is `residents::resolve_photo_bytes` (`residents.rs:1782`). Four documented `#[allow(dead_code)]` items remain: `db.rs:306,314` (asserted by `tests/regression.test.js:147-148`) and `crypto.rs:233,241` (reserved encrypted-backup path) |
| Rustdoc | 10 modules | 1,041 `///` lines; `residents.rs` 87 to 349. Purpose, arguments, returns, `# Errors`, `# Side Effects`, and `# Panics` on the only 3 production `expect()` calls (`crypto.rs:70,76,115`), each naming the invariant that keeps it unreachable. No runnable `# Examples` blocks: the functions worth demonstrating need a live database or photo directory |
| Tests | `db.rs` +2, `export.rs` +4, `logger.rs` +3, `residents.rs` +2 | 43 to 54 `#[test]`. New coverage: unique-path never overwrites a prior export (P2-8), local-date backup stem, char-boundary-safe log truncation (the panic regression), `escape_like` wildcard neutralisation (F2), legacy contact pass-through, export column/blank-cell mapping, address join order |
| Regression guard | `tests/regression.test.js` | New check fails if `PRAGMA wal_checkpoint(TRUNCATE)` appears without a `busy != 0` inspection; the `bundle ships MSI` check asserted NSIS-only at the time; it still locks NSIS-only (`msi` excluded). Two unused variables removed and three template literals converted to backticks |

Validation (2026-11-06): `cargo fmt --all -- --check` clean (exit 0, zero output, all 11 files); `cargo metadata --locked` exit 0; `npm test` 57 pass / 0 fail / 6 suites; `npm run check:css` CSS fresh; `npm audit` 0 vulnerabilities. Rust compile, test, clippy and doc remain **BLOCKED on this host** for the reasons in the workflow section above, so the Rust changes in this pass are statically reviewed and parse-verified but not compiler-verified. Committed locally on 2026-11-06 as `cde73d0` (this Rust pass), alongside the installer work in `836daa8` and this document's own update in `f9dcbd7`; nothing has been pushed.
