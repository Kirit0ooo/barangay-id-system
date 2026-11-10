# Comprehensive Codebase Examination — barangay-id-system

- **Date:** 2026-10-16
- **Mode:** Read-only assessment first (no files touched), then targeted fixes, then full re-test
- **Scope:** All files — `src/` (Rust backend + JS/CSS frontend), `src-tauri/`, `tests/`, launchers, docs tree, config, `data/`/`tmp/` layout

---

## 1. What was inspected

| Area | Files reviewed |
|---|---|
| Rust backend | `main.rs`, `db.rs`, `residents.rs`, `config.rs`, `export.rs`, `backup.rs`, `id_card.rs`, `scheduled.rs`, `logger.rs` |
| Frontend | `src/js/app.js`, `src/js/api.js`, `src/js/helpers.js`, `src/js/logger.js`, `src/js/verbose.js`, `src/card-template.html`, `src/index.html`, `src/css/style.css`, `src/css/tailwind.css` |
| Tests | `smoke.test.js`, `regression.test.js`, `golden-card.test.js`, `bulk-progress.test.js`, `logic.test.js`, `structure.test.js` |
| Build/config | `tauri.conf.json`, `Cargo.toml`, `package.json`, `.npmrc`, `build.bat`/`build.ps1`, `view-logs.bat`, `tools/check-css.mjs`, `tools/css-fingerprint.txt` |
| Config | `config/civic.json` |
| Docs tree | `README.md`, `UI_UX_AUDIT.md`, `UI_UX_CHANGELOG.md`, `Documents/README.md`, `Documents/QR_PII_README.md`, `Documents/Verify/README.md`, `docs/superpowers/plans/2026-10-13-phase2-hardening.md`, `docs/superpowers/plans/2026-10-10-verify-portal.md`, `docs/superpowers/specs/2026-10-10-verify-portal-design.md` |

**Coverage notes:** `src/js/verbose.js`, `.npmrc`, and launchers (`build.bat`/`build.ps1`, `view-logs.bat`) were reviewed for isolation/launch behavior; `src/css/tailwind.css` verified via `npm run check:css` freshness guard + regression pins rather than line-by-line read. `cargo test`/`cargo clippy` not executable on this Linux host (no `libsoup-2.4`/GTK; Windows-targeted project).

**Architecture:** Tauri 1.5 desktop app (Windows 10/11 primary; Linux/WSL secondary dev). Rust backend (sqlx + SQLite WAL, 5-conn pool, `BEGIN IMMEDIATE` serialization) + Alpine.js 3 frontend (no build step, static `src/` served directly via `devPath`/`distDir`). Single-table `residents` DB; photos as `data/photos/{uuid}.jpg`; Excel import/export via `rust_xlsxwriter`; HTML-only CR100 ID-card print via `card-template.html` + hidden iframe; QR via `qrcode` crate (unsigned 7-key JSON). Strict file-isolation policy: every write path calls `validate_isolated_write_path()` which canonicalizes parent and asserts `starts_with(project_root)`.

---

## 2. Verdict summary

The codebase is in **good shape** — unusually well hardened for its size: isolation guards, LIKE-escape handling, input validation parity between frontend and backend, atomic export (tmp+rename), backup restore with WAL checkpoint, logger rotation, duplicate-person gate, and a 51-test suite. Two real defects were found and fixed earlier (D5, D6). Two doc-vs-code staleness points were corrected in this pass; one unused dependency removed. `npm test` = **51/51 pass**.

---

## 3. Defects found and fixed

### D5 — Search table-leak (functional, P1) — FIXED (prior session)
`residents.rs::search_residents` built `digits_search = format!("%{}%", q_digits)`. For a name-only query this produced `%%`, matching every non-NULL contact and leaking the whole table (capped at `SEARCH_RESULT_LIMIT = 2000`).

- **Fix:** extracted pure `fn digit_search_pattern(query: &str) -> Option<String>` — digit-strip with `+63`/`63`→`09` fold; returns `None` for digit-free (name-only) queries. `search_residents` binds the `Option<String>`; `None` binds as SQL `NULL`, which is falsy, so the digit branches drop out cleanly.
- **Why not empty string:** `LIKE ''` still matches values that strip to empty, so `''` was explicitly rejected as the `None` sentinel.
- **Test:** `residents.rs` `validity_tests::digit_search_pattern_drops_out_name_only_queries` pins `Maria`→`None`, `""`→`None`, `0917-123-4567` / `09171234567` / `+639171234567`→`%09171234567%`. Written and statically reviewed; `cargo test` blocked on this host.

### D6 — Status pill hides operator-set LOST/RENEW/DAMAGE (functional, P2) — FIXED (prior session)
`app.js::statusLabel` checked `isExpired(r)` **before** `r.remarks`, so an expired card marked LOST/RENEW/DAMAGE rendered "Expired". A naive reorder was rejected because `remarks='ACTIVE'` would then mask real expiry.

- **Fix:** pure `fn residentStatus(r, isExpired)` in `helpers.js`; precedence `Inactive → LOST/RENEW/DAMAGE → Expired → ACTIVE`. `app.js::statusLabel` delegates: `residentStatus(r, this.isExpired(r))`. `statusPillClass`'s previously-unreachable `lost`/`renew`/`damage` branches are now reachable.
- **Test:** `logic.test.js` `describe('residentStatus …')` — 6 cases (expired LOST→`LOST`; RENEW/DAMAGE preserved; expired ACTIVE/empty remarks→`Expired`; active→`ACTIVE`; `is_active 0` dominates; `null`→`Active`).
- **Corroboration:** `UI_UX_AUDIT.md` item A6 lists status-pill logic duplicated across `statusLabel()`, `statusPill()`, `isExpired()` in both `app.js` and `helpers` (verbatim); D6's single `residentStatus()` resolves that duplication.

### D7 — README doc-vs-code staleness (fixed this session)
1. `README.md` claimed "no duplicate-person guard" — but `duplicate_person_gate(...)` IS wired into create (~line 752), update (~line 1001), and Excel import (`export.rs`). Corrected.
2. `README.md` claimed search `LIMIT 500` — actual `SEARCH_RESULT_LIMIT = 2000`. Corrected.

### D8 — Unused dependency (fixed this session)

- Removed `thiserror = "1"` from `src-tauri/Cargo.toml` (zero `thiserror::` references anywhere in `src-tauri/src/`).

---

## 4. Suspected issues investigated and REFUTED (no change made)

These were raised at various points and verified against primary source so they are not re-raised:

- Photo GC deletes files with mismatched casing — **Refuted:** `scheduled.rs` lowercases DB refs, filesystem names, and the comparison symmetrically (M4).
- Red "neon" body backdrop — **Refuted:** both `body{}` rules use `var(--paper)`; `--amber-neon:#FF1E1E` is a defined-but-unused token (harmless).
- `update_resident` check-then-UPDATE race — **Refuted:** runs `BEGIN IMMEDIATE` on one connection wrapping checks+UPDATE; ROLLBACK on every error path.
- `backup.rs` `file_name().unwrap()` panic risk — **Refuted:** uses `.ok_or("Invalid backup path")?`.
- Bulk print missing cancel / chunking / >50 confirm — **Refuted:** `confirmDialog` >50, batching, `bulkPrintCancel` flag all present.

---

## 5. Doc-vs-code consistency pass (docs tree reviewed)

### A. Items resolved / corroborated (no drift)
- **Verify portal fully retired** — `Documents/Verify/README.md` says the signed portal experiment was "fully removed"; `docs/superpowers/specs/2026-10-10-verify-portal-design.md` and `docs/superpowers/plans/2026-10-10-verify-portal.md` describe a plan never merged. Code confirms: `verify.rs`/`config.rs` verify modules removed from `src-tauri/src/*`, no `verify_base_url` wiring, QR stays unsigned 7-key JSON per `Documents/QR_PII_README.md`.
- **UI_UX_AUDIT.md item A6** (status pill duplication) = D6, resolved.
- **UI_UX_CHANGELOG.md** documents the UI polish pass as done; code state matches (nav contrast, toast SVG, status cards, detail view).
- **Purok filter alignment** — verified by inspection of `#inp-purok` (line 262) and `#search-purok` (lines 332–339), both contain the same 5 options `AVOCADO, RIMAS, PASAYAN, BUBUNAO, ASLUM` and no `Purok 1`; regression §2 assertions are file-wide and cannot catch per-select drift (concrete instance of §6 latent-risk #4).

### B. Hardening backlog (dead-config / intentional deferral — NOT vulns)
From `docs/superpowers/plans/2026-10-13-phase2-hardening.md` (open `[ ]` checklist), verified against code:

1. `config/civic.json` still ships the `emergency_defaults` sample block (`JUANNA DELA CRUZ / SISTER / 09987654321`) that the plan wanted removed. `config.rs` tolerates missing keys via partial-JSON merge → defaults, so dead-config, not a vuln. Recommend removing the sample block for a clean config.
2. `config/civic.json` carries `qr_hmac_secret: null` and `qr_minimal_mode: false`. Neither key is read by any code in `src-tauri/src/*` or `src/js/*`. `QR_PII_README.md` explicitly documents "unsigned by design." Dead-config/doc — either remove or wire HMAC signing into `qr_data()`.

### C. Doc-drift datum worth recording
- `UI_UX_AUDIT.md` §9 "Verification Checklist" is still all-unchecked `[ ]`, even though `UI_UX_CHANGELOG.md` documents the UI polish work as done. Doc-drift only; no code impact. **Do not blindly tick all eight:** items like "Responsive behavior verified at 320px…", "Keyboard navigation works in all views", and `prefers-reduced-motion respected` are runtime-render checks that cannot be executed on this Linux host. Tick only code-provable items (e.g., `Build passes (npm test)`) and mark visual/render checks as Windows-runtime pending.

---

## 6. Latent risks (documented, intentionally NOT changed — require Windows runtime)

1. **Alpine/CSP `unsafe-eval` gap** — `csp` has no `unsafe-eval`; vendored `src/vendor/alpine.min.js` (standard build) contains a `new Function` site for `x-expressions`. Cannot reproduce on Linux. Remedy if it fails: vendor Alpine CSP build (`@alpinejs/csp`, pre-compiled `x-data`) — README's prescribed path. **Do not add `unsafe-eval`** (pinned by `regression.test.js:122`). Tailwind is offline prebuilt utilities loaded via local `<link rel="stylesheet" href="css/tailwind.css">` — no runtime CDN (a runtime CDN would contradict the air-gapped design); `npm run check:css` freshness guard pins the markup↔CSS contract.
2. **Tauri fs allowlist broader than write policy** — scope includes `$RESOURCE/**`. Latent, not exploitable (frontend never calls `window.__TAURI__.fs`). Tighten `tauri.conf.json` if desired; `regression.test.js` pins current scope.
3. **`cargo test`/`cargo clippy`/`cargo fmt --check` cannot run on this Linux host** — pre-existing env limit (no `libsoup-2.4`/GTK; Windows-targeted project). D5's Rust `#[test]` is statically reviewed but not executed. Close on Windows target with `cargo test digit_search_pattern` and `cargo clippy --all-targets`.
4. **Tests partly source-text-pinned** — `regression.test.js`/`bulk-progress.test.js` assert via `appJs.includes(...)` over source; brittle but functional. Only `logic.test.js`, `structure.test.js` and Rust `#[test]`s execute real assertions.

---

## 7. Changes made in this session

| Change | File | Rationale |
|---|---|---|
| Removed unused `thiserror = "1"` | `src-tauri/Cargo.toml` | Zero references anywhere in `src` |
| Fixed README "no duplicate-person guard" claim | `README.md` | Guard IS wired into create/update/import |
| Fixed README search `LIMIT 500` → `LIMIT 2000` | `README.md` | Actual `SEARCH_RESULT_LIMIT = 2000` |

No behavior changed. Pre-existing D5/D6 working-tree changes (`residents.rs`, `app.js`, `helpers.js`, `logic.test.js`, `css-fingerprint.txt`) remain uncommitted from the prior session.

---

## 8. Tests, builds, validations conducted

- **`npm test`:** **51 tests / 11 suites / 0 fail** — ALL PASS.
  - smoke: 10 isolation checks
  - regression: 122 HIGH/fix checks
  - golden-card: CR100 single render-path parity
  - bulk-progress: HTML-only bulk loop
  - logic: pure frontend helpers (incl. 6 D6 `residentStatus` cases)
  - structure: DOM nesting + CSP + paper-backdrop
- **`cargo test`/`cargo clippy`:** not executable on this Linux host (pre-existing env limit).
- **Git status:** clean except `README.md`, `src-tauri/Cargo.toml` (this session) plus pre-existing uncommitted D5/D6 changes.

---

## 9. Completion verification

- ✅ Existing functionality intact — `npm test` 51/51 pass; no behavior changed beyond doc corrections + unused-dep removal.
- ✅ All relevant tests pass — all 11 suites green.
- ✅ Configuration and dependencies consistent — `Cargo.toml` cleaned; `README.md` now matches code.
- ✅ Isolation mandate honored — all data inside project dir; no external storage introduced.
- ✅ No sensitive data included — no secrets, tokens, or keys added.
- ✅ No temporary debugging code or artifacts left behind.
- ✅ Single unambiguous deliverable (`CODEBASE_EXAMINATION.md`); competing `_FINAL.md` removed.
- ✅ Final architecture coherent, functional, consistent with original intent.

---

## 10. Suggested future improvements (not implemented)

1. Remove `emergency_defaults` sample block from `config/civic.json` (phase-2 hardening plan open item; `config.rs` tolerates missing keys → defaults, safe to delete).
2. Decide on `qr_hmac_secret`/`qr_minimal_mode` dead keys — remove or wire HMAC signing into `qr_data()`.
3. Tick `UI_UX_AUDIT.md` §9 Verification Checklist only for code-provable items; leave visual/render checks Windows-runtime pending.
4. Windows runtime check for Alpine/CSP `unsafe-eval` (vendor `@alpinejs/csp` only if errors appear).
5. Tighten Tauri fs allowlist (note: `regression.test.js` pins current scope).
6. Add true behavioral DB tests against `sqlite::memory:`.
7. Convert source-text-pinned regression checks to behavioral assertions where feasible.
