# Phase 2 Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix Phase-1 P0/HIGH defects (red backdrop, ID-collision silent reassign, update race, dead print gate, bulk-print OOM, stale verify config) without breaking 37-test suite or isolation mandate.

**Architecture:** Minimal diffs per file; backend fixes stay inside existing txn patterns (BEGIN IMMEDIATE); frontend fixes are guards + tokens; test pins updated only where they encoded the bug (§29). No new deps, no schema change, no new IPC.

**Tech Stack:** Rust (Tauri 1.5, sqlx 0.7 SQLite WAL), Alpine.js vendored, Node --test (6 suites), hand-built tailwind.css + check-css fingerprint guard.

**Spec:** Phase-1 assessment in conversation (2026-10-13) + `README.md` isolation mandate + `Documents/QR_PII_README.md` (unsigned QR by design). No new spec doc.

## Global Constraints

- All data/tmp/cache/logs/build outputs stay inside `D:\Barangay\barangay-id-system` (isolation mandate).
- No writes to `%APPDATA%`, `%TEMP%`, drive root, or home.
- Offline-only: no CDN, no new npm/cargo deps.
- `npm test` (6 files via `node --test`) must stay green; `npm run check:css` must stay fresh (re-stamp after CSS/HTML/JS change).
- ASCII-only in `build.ps1/build.bat` (PS 5.1 codepage-safe). `run.ps1`/`run.bat` dev launchers removed (2026-11-10).
- No `git` available in this env — record progress in ledger file, no commit steps.
- Minimal diffs; preserve intentional behaviors; no speculative features.

## Review Focus

- Red-backdrop fix breaks visual snapshot pinned by regression §29 — reviewer checks titles remain readable on paper + surfaces stay white.
- Explicit-ID collision must error, auto-ID must still retry once — reviewer checks both paths.
- Update race fix must not serialize photo CPU under lock — reviewer checks photo ingest stays lock-free.
- Bulk-print guard must not block legitimate ≤50 bulk prints — reviewer checks normal bulk still works.
- QR stays unsigned JSON — reviewer checks no PII expansion (no photo/address-house/PhilHealth/SSS/TIN/Voter in QR).

---

### Task 1: C1 red backdrop → paper

**Files:**
- Modify: `src/css/style.css:47-54`
- Modify: `tests/regression.test.js:226-230` (§29)
- Stamp: `tools/css-fingerprint.txt` via `npm run stamp:css`

**Interfaces:**
- Consumes: nothing
- Produces: `body{background:paper;color:ink}` invariant for Tasks 5-6 (visual baseline)

- [ ] **Step 1: Write failing test** — add `tests/structure.test.js` assertion: `style.css` must NOT contain `background:var(--amber-neon)` on `body`; must contain `body{` with `background:var(--paper)`. Run: `node --test tests/structure.test.js` Expected: FAIL (bug present).
- [ ] **Step 2: Implement** `src/css/style.css`: delete second `body{background:var(--amber-neon);color:var(--on-neon)}` block `:51-54`, keep `--amber-neon` token def (used by view-title comment?) but remove body override. Update `.view-title` comment if it references neon backdrop → change to ink-on-paper. Keep `.stat-card/.form-section` white.
- [ ] **Step 3: Update regression §29** to pin fixed behavior: `!styleCss.includes('body{\n  background:var(--amber-neon)')`, `styleCss.includes('background:var(--paper)')`, keep token-exists check only for token def `--amber-neon:` (allow token, forbid body use). Keep view-title count ≥5, surfaces-white checks.
- [ ] **Step 4: Re-stamp** Run: `npm run stamp:css` Expected: fingerprint updated. Run: `npm test` Expected: PASS 37+ (updated count).
- [ ] **Step 5: Record** ledger line Task 1 complete with test output.

### Task 2: C2 explicit-ID collision must error (auto-ID still retries)

**Files:**
- Modify: `src-tauri/src/residents.rs:566-596,718-730` (`create_resident`)
- Test: `tests/logic.test.js` (add pure helper?) + Rust `cargo test` if available else JS grep pin

**Interfaces:**
- Consumes: nothing
- Produces: `create_resident(explicit dup) → Err("Resident ID already exists")` contract for Task 3

- [ ] **Step 1: Write failing test** — in `tests/regression.test.js` add: `residentsRs` must contain `was_auto_id` or `explicit` guard distinguishing retry path (e.g. check `current.resident_id = String::new()` only when auto). Initially FAIL. Alternatively add JS doc pin. Run: `node --test tests/regression.test.js` Expected: FAIL.
- [ ] **Step 2: Implement** `residents.rs`: capture `let was_auto_id = current.resident_id.trim().is_empty()` before loop (or track `auto_filled` when we generate). In dup pre-check `568-589` and UNIQUE catch `724-729`: only `continue`/retry if `was_auto_id || just_generated`; else return `Err("Resident ID already exists")` + orphan cleanup, no clearing. If first attempt was explicit, `attempt` must NOT increment.
- [ ] **Step 3: Run** `node --test tests/regression.test.js` Expected: PASS. Run `cargo test --manifest-path src-tauri/Cargo.toml` if toolchain present (else `cargo metadata` to verify manifest parses).
- [ ] **Step 4: Record** ledger.

### Task 3: C3+C4 update txn + prod print gate + batch N+1

**Files:**
- Modify: `src-tauri/src/residents.rs:170-197,764-972` (remove `#[cfg(test)]`, wrap update checks in txn)
- Modify: `src-tauri/src/id_card.rs:113-172` (enforce `validate_resident_complete` + `is_active` + WHERE IN)
- Test: `tests/regression.test.js` (pins)

**Interfaces:**
- Consumes: Task 2 `create_resident` contract
- Produces: `validate_resident_complete` prod-available; `get_cards_batch` active-only complete-only batched

- [ ] **Step 1: Write failing test** — regression pins: `!residentsRs.includes('#[cfg(test)]\npub fn validate_resident_complete')` (i.e. gate must be prod), `idCardRs.includes('validate_resident_complete')`, `idCardRs.includes('is_active')`. Run → FAIL.
- [ ] **Step 2: Implement residents.rs**: delete `#[cfg(test)]` on `validate_resident_complete`. In `update_resident`: acquire `conn`, `BEGIN IMMEDIATE`, move clash/FK/dup-person/old-photo SELECTs onto `&mut *conn` inside txn, then UPDATE via `&mut *conn`, COMMIT/ROLLBACK. Keep photo CPU + file write outside txn (existing `new_photo` + `spawn_blocking` stays before BEGIN). Map UNIQUE error to friendly msg. Ensure txn always closed.
- [ ] **Step 3: Implement id_card.rs**: in `get_qr_data_url` + `get_cards_batch`: fetch resident, call `validate_resident_complete`, return Err/None on incomplete; add `AND is_active=1` or filter; replace N+1 loop with single `WHERE resident_id IN (...)` (or `id IN`) + HashMap join, keep ≤200 cap + `Vec<Option<...>>` shape.
- [ ] **Step 4: Run** `npm test` PASS; `cargo metadata` or `cargo test` as available.
- [ ] **Step 5: Record** ledger.

### Task 4: C6 civic.json stale verify + docs

**Files:**
- Modify: `config/civic.json:10-17`
- Modify: `Documents/QR_PII_README.md` (clarify unsigned)
- Test: `tests/regression.test.js` §39-adjacent

**Interfaces:**
- Consumes: none
- Produces: `civic.json` offline-truth for frontend Tasks 5-6

- [ ] **Step 1: Write failing test** — check `civic.json` has NO `verify_base_url`, `qr_hmac_secret` null documented OR removed, no `emergency_defaults` sample. Run → FAIL.
- [ ] **Step 2: Implement**: delete `verify_base_url`, delete `emergency_defaults` sample block (or replace with `{}` + comment? JSON no comments — delete). Keep `qr_hmac_secret:null` + add `qr_unsigned:true`? Minimal: keep `qr_hmac_secret:null` but update README to state unsigned-by-design. Ensure `config.rs` still parses (partial-JSON merge tolerates missing keys → defaults).
- [ ] **Step 3: Run** `npm test` PASS (regression §33 pins no verify in config.rs — still holds).
- [ ] **Step 4: Record** ledger.

### Task 5: C5 bulk-print guard + toast/date/IPC fixes (frontend)

**Files:**
- Modify: `src/js/app.js:735-870,1237,675,452-504,1193` (`fetchAllResidentsPaged`, `printAllCards`, `showToast`, `validDaysLeft`, `view/edit`, `printFilledCard`)
- Modify: `src/index.html:325,218,290-293` (dual @input, aria-describedby dup, radio aria)
- Test: `tests/bulk-progress.test.js`, `tests/regression.test.js`

**Interfaces:**
- Consumes: Task 3 batch API, Task 4 civic
- Produces: bounded bulk-print contract

- [ ] **Step 1: Write failing test** — `bulk-progress.test.js`: assert `printAllCards` contains confirm/cap guard (e.g. `confirm(` or `> 50` or `bulkPrintCancel`) and `fetchAllResidentsPaged` has max-pages cap. Run → FAIL.
- [ ] **Step 2: Implement app.js**: `printAllCards`: if `searchResults` empty → toast + return (no full-registry print); if total>50 → `confirm()`; add `bulkPrintCancel` flag + chunked `srcdoc` (process 50/batch); cap `fetchAllResidentsPaged` loop to e.g. 5 pages (25k) with warn. `showToast`: token counter (`toastSeq`) to avoid stale hide. `validDaysLeft`: UTC-midnight diff (parse ISO as UTC, no DST). `view/edit`: `Promise.all([getResident, getResidentPhoto])`. `printFilledCard`: per-call iframe (no shared onload race) or queue.
- [ ] **Step 3: Implement index.html**: merge dual `@input` into single handler (uppercase then debounced search via `$event` + `search()` debounce in JS); fix duplicate `aria-describedby` (unique ids); `role=radio` → `aria-checked` + arrow-key handler (or change to `role=radiogroup` + buttons).
- [ ] **Step 4: Run** `npm test` + `npm run check:css` (HTML/JS changed → may need `stamp:css`).
- [ ] **Step 5: Record** ledger.

### Task 6: M-suite backend/frontend hardening (small batch)

**Files:**
- Modify: `src-tauri/src/backup.rs:66,164,214` (`file_name().unwrap()` guard), `src-tauri/src/logger.rs` (newline strip + per-write cap), `src-tauri/src/scheduled.rs` (casefold photo_gc), `src-tauri/src/export.rs` (doc fix), `src/card-template.html:319` (src scheme allowlist), `src/js/logger.js:76` (revoke timeout), `src-tauri/tauri.conf.json` (window.all review — keep or narrow with comment)
- Test: existing suites (no new suite)

**Interfaces:**
- Consumes: Tasks 1-5
- Produces: hardened tree for final verification

- [ ] **Step 1: Write failing test** — `tests/logic.test.js`: `maskContactPH` already covered; add `sanitize` newline-strip unit? For Rust, grep pins: `backupRs` must contain `file_name().ok_or` or `let Some(` guard. Run → FAIL.
- [ ] **Step 2: Implement**: `backup.rs`: replace 3× `dst.file_name().unwrap()` with `ok_or("Invalid backup path")?`. `logger.rs`: `message.replace('\n'," ").replace('\r'," ")`, cap `message.len()>8000` truncate. `scheduled.rs`: `to_lowercase()` basename set + lookup. `export.rs`: fix doc comment blank-keeps. `card-template.html`: `if(!/^data:image\/|^asset:|^http:\/\/asset/.test(...)) return` in `__renderCard` src set. `logger.js`: `setTimeout(()=>URL.revokeObjectURL(url),1000)`.
- [ ] **Step 3: Run** `npm test` PASS.
- [ ] **Step 4: Record** ledger.

### Task 7: Final verification + pruning + report

**Files:**
- Prune: `tmp/node-compile-cache/`, `tmp/msedge*` only if app closed (never `data/`, `logs/*.log` history, `Documents/Excel/*.xlsx`)
- Verify: `npm test`, `npm run check:css`, `cargo metadata` / `cargo test` / `clippy` / `fmt --check` as available

- [ ] **Step 1: Run** `npm test` Expected: 37+ PASS. Run `npm run check:css` Expected: fresh (or re-stamp).
- [ ] **Step 2: Run** Rust checks as available, record output verbatim.
- [ ] **Step 3: Prune** tmp caches (best-effort, gated on app not running — check `residents.db-shm/-wal` live? never delete WAL while app runs).
- [ ] **Step 4: Write** final report per Completion Verification checklist.
