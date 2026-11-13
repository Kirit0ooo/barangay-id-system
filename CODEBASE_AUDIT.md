# CODEBASE_AUDIT.md — Initial Assessment Report (Phase 4)

**Framework:** CODEBASE_REVIEW.md v2.0 (14-phase evidence-based audit)  
**Status:** Phases 1–4 complete (baseline + inventory + architecture + audit; initial report below)  
**Repo:** `/mnt/d/Barangay/barangay-id-system`  
**Branch:** `master` @ `514a70a` (ahead 3 vs origin/master)  
**Assessed:** 2026-11-13  
**Version:** 1.0.36 (all manifests synced: package.json, package-lock.json, backend/src/Cargo.toml, backend/src/Cargo.lock, backend/src/tauri.conf.json, frontend/src/index.html, README.md, tools/css-fingerprint.txt)  
**Working tree:** 8 pre-existing modified files — all are version bumps 1.0.35 → 1.0.36 (preserved; pre-date this task)

---

## 1. Executive summary

The Barangay ID Registration System is a Tauri v1.5 offline-first desktop app (Alpine.js frontend, Rust/sqlite backend). The codebase is in good, verified condition: the full test suite passes, CSS fingerprint is fresh, and `npm audit` reports 0 vulnerabilities. No functional, security, or data-integrity defects were found in this assessment.

One genuine finding: **documentation staleness** — AGENTS.md:197 states the test gate as "69 pass / 0 fail / 15 suites / 7 files" while the live suite runs **70 pass / 0 fail / 15 suites / 7 files**. All other "critical/high" claims from prior advisor output were re-checked against primary source and remain dismissed as already defended (CB-002, CB-005, CB-006).

Environment limits prevent local Rust compilation (CI-only) and visual/render checks (no graphical environment); both are documented, pre-existing constraints, not regressions.

---

## 2. Scope and method

- Read-only inspection of the full project tree (entry points, backend commands, frontend modules, tests, configs, launchers, CI, docs). No files modified during assessment.
- Primary-source verification: every claimed defense is tied to a file:line quote from the shipped source.
- Advisor/previous-report output is treated as background, not evidence; each claim was re-verified against the actual code.
- Tests, builds, linters, and formatters were only run where their side effects were understood and safe (`npm test`, `npm run check:css`, `npm audit` — all read-only with respect to project data).
- No network, no privileged operations, no production systems, no secrets printed.

---

## 3. Baseline

| Item | Value |
|---|---|
| Project root | /mnt/d/Barangay/barangay-id-system |
| VCS | Git worktree, branch `master` |
| HEAD commit | `514a70a5148aaed869daf2d9b9f6178df1ab22ce` ("fix(preview): print card iframe content for clean CR100 output") |
| Version | 1.0.36 across all manifests |
| Pre-existing changes | 8 files modified, all version bumps 1.0.35 → 1.0.36 (preserved, not touched by this task) |
| User data | `data/residents.db`, `data/photos/`, `data/backups/`, `config/civic.json`, `.crypto-key/` — preserved untouched |

---

## 4. Inventory

| Category | Paths |
|---|---|
| Frontend (Alpine SPA, classic scripts) | `frontend/src/index.html`, `frontend/src/js/app/*.js`, `frontend/src/js/app/{dashboard,residents,settings}/*.js`, `frontend/src/js/services/api.js`, `frontend/src/js/services/logger.js`, `frontend/src/js/shared/helpers.js`, `frontend/src/js/bootstrap/verbose.js` |
| Frontend styles/assets | `frontend/src/css/style.css`, `frontend/src/css/tailwind.css`, `frontend/src/card-template.html`, `frontend/src/fonts/`, `frontend/src/icons/`, `frontend/src/vendor/` |
| Backend (Tauri commands, sqlx SQLite) | `backend/src/src/{main,residents,backup,config,crypto,db,encryption,export,id_card,logger,scheduled}.rs` |
| Build/config | `backend/src/tauri.conf.json`, `backend/src/Cargo.toml`, `backend/src/Cargo.lock`, `package.json`, `package-lock.json`, `.npmrc` |
| Launchers/scripts | `build.ps1`, `build.bat`, `tools/launcher-common.ps1`, `tools/check-css.mjs`, `tools/css-fingerprint.txt` |
| Config/data | `config/civic.json`, `data/residents.db`, `data/photos/`, `data/backups/`, `.crypto-key/key.bin`, `logs/`, `tmp/` |
| Tests | `tests/{smoke,regression,golden-card,bulk-progress,logic,structure,preview-window}.test.js` |
| CI | `.github/workflows/ci.yml` |
| Docs | `AGENTS.md`, `README.md`, `DESIGN.md`, `SECURITY.md`, `CODEBASE_REVIEW.md`, `CODEBASE_REMEDIATION_REPORT.md`, `Documents/`, `gathered-md/` |

Excluded from line-by-line review: `node_modules/` (third-party, installed), minified/binary assets (`barangay.ico`, fonts, PNG icons), `backend/src/target/` (build output, gitignored). Provenance and role understood for each.

---

## 5. Architecture

**Data flow:** UI (Alpine view modules) → `frontend/src/js/services/api.js` (`window.__TAURI__.invoke`) → Tauri commands registered in `backend/src/src/main.rs` → sqlx SQLite WAL database (`data/residents.db`, 5-conn pool, `BEGIN IMMEDIATE` for ID generation) → JSON response → frontend state → rendered result.

**Backend commands (26):** create/get/update/delete/search/all/count/dashboard/recent/by-purok/generate-next-id/resident-photo residents; get/set civic config, get short barangay, open path; id_card QR (`get_qr_data_url`, `get_cards_batch`); backup/restore database; export/import excel; scheduled backup + photo GC; optimize database; encryption (generate key, status, encrypt/decrypt DB and photos); logging + documents dirs.

**Key safeguards already in code:**
- D5: `digit_search_pattern` returns `None` for digit-free queries (binds NULL, contact OR branches drop out) — residents.rs:237-256
- F2: `escape_like` neutralizes `%` `_` `\` wildcards — residents.rs:225-232
- P0: FK orphan guard before soft-delete — residents.rs `delete_resident`
- P2: explicit `RESIDENT_COLUMNS` instead of `SELECT *` — residents.rs
- P2-8: DOB sanity cap (120 years) + future-date rejection — `normalize_birth_date`
- WAL checkpoint guard: `checkpoint_wal_truncate` inspects `busy` and aborts on busy — db.rs:786-805; used by backup.rs, restore, encryption.rs, scheduled.rs
- AES-256-GCM file-level encryption for DB + photos, key at `.crypto-key/key.bin` (documented threat model: key sits beside data)

**Print:** HTML-only CR100 card via `card-template.html` + hidden iframe; no PDF generation anywhere. Frontend never reaches outside the project tree (TMP/TEMP/TMPDIR/WEBVIEW2 pinned to `tmp/`).

---

## 6. Coverage register

| Area | Coverage | Reason |
|---|---|---|
| Frontend views & API contracts | Reviewed | index.html + app modules + api.js read; exercised by 7 test suites |
| Backend commands & SQL | Partially reviewed | Critical functions read (search, digit pattern, escape_like, checkpoint guard); full residents.rs (2225 lines) sampled, not line-by-line |
| Rust compile/test/clippy/doc | Not reviewed (executable) | Blocked on this host: no `libsoup-2.4`, no MSVC `lib.exe`; CI (`cargo test --manifest-path backend/src/Cargo.toml`) is the compiler gate |
| `cargo fmt --check` / `cargo metadata --locked` | Reviewed (records) | Clean per AGENTS.md record (2026-11-06); not re-run locally (environment limit) |
| Tests | Executed | `npm test`: 70 pass / 0 fail / 15 suites / 7 files |
| CSS freshness | Executed | `npm run check:css`: CSS fresh, exit 0 |
| Dependency audit | Executed | `npm audit`: 0 vulnerabilities |
| Visual/render (WCAG, responsive, keyboard) | Not executed | No graphical environment on this Linux host |
| Network/external services | Not applicable | Offline-first app; no network used by design |

---

## 7. Existing strengths

- Parameterized sqlx queries everywhere (no string-interpolated SQL); explicit column lists.
- Soft-delete with FK orphan guard; ID generation under `BEGIN IMMEDIATE`.
- D5 search-table-leak fix with unit tests; F2 wildcard escaping with unit tests.
- WAL checkpoint `busy` inspection before backup/encrypt/restore.
- Offline-first, project-local data; no AppData/TEMP leakage; temp dirs pinned to `tmp/`.
- AES-256-GCM at-rest encryption with documented threat-model boundary.
- HTML-only print (no PDF attack surface); CSP with script hashes + `unsafe-eval` (documented Alpine requirement).
- 7 test suites, 70 tests, all passing; CSS fingerprint gate; npm audit clean.

---

## 8. Findings

| ID | Category | Severity | Confidence | Status | Location |
|---|---|---|---|---|---|
| CB-AGENTS-03 | Documentation staleness | Low | Confirmed | Open | AGENTS.md:197 ("69 pass / 0 fail / 15 suites / 7 files"; live suite runs 70) |

Previously reviewed and **dismissed** (primary-source re-verified this session):
- CB-002 date parsing — `parse_any_date` accepts YYYY-MM-DD / DD-MM-YYYY / DD/MM/YYYY (residents.rs:249-278), unit-tested.
- CB-005 path traversal — `validate_isolated_write_path` lexical + `..` + null-byte + canonical-parent + symlink checks (db.rs:477-546), unit-tested.
- CB-006 SQL injection — backup tmp name from `Local::now()` gated by `is_machine_backup_name` (strict pattern) + single-quote escaping (scheduled.rs:33-45, 193-194); not attacker-controlled.

Known boundary gaps (documented design, not regressions):
- Encryption key `.crypto-key/key.bin` sits beside `data/residents.db` — threat model accepts this (protects against file-copy-only attackers).
- Pre-existing plaintext backups and plaintext photos added while DB unencrypted remain readable until explicitly encrypted.
- `config/civic.json` dead keys `qr_hmac_secret: null`, `qr_minimal_mode: false` — dead config, not a vulnerability.

---

## 9. Validation baseline

| Check | Command | Result | Evidence |
|---|---|---|---|
| Test suite | `npm test` | PASS | 70 pass / 0 fail / 15 suites / 7 files |
| CSS freshness | `npm run check:css` | PASS | CSS fresh, exit 0 |
| Dependency audit | `npm audit` | PASS | 0 vulnerabilities |
| Rust fmt | `cargo fmt --all -- --check` | NOT RUN | Blocked on this host (no Rust toolchain target deps); clean per prior record |
| Rust compile/test/clippy/doc | — | BLOCKED | No `libsoup-2.4`, no MSVC `lib.exe`; CI-only |
| Visual/render checks | — | NOT RUN | No graphical environment |

---

## 10. Priorities (remediation order)

1. **CB-AGENTS-03** — restate AGENTS.md:197 test gate to "70 pass / 0 fail / 15 suites / 7 files" (docs only, lowest risk).
2. Re-validate after any change: `npm test`, `npm run check:css`, `npm audit`.
3. Rust edits (if any): static review only locally; validate in CI before merge.

No Critical/High findings; no data-loss or access-control hazards identified.

---

## 11. Uncertainties and blockers

- Rust compiler checks cannot run on this host; any Rust change rests on static review + CI. This is a pre-existing environmental limit, not a code defect.
- Visual/render verification (contrast, responsive layout, keyboard navigation) not performed — no graphical environment on this host; recommended on a Windows runtime when available.

---

## 12. Optional improvements (not required repairs)

- Remove or wire `qr_hmac_secret` / `qr_minimal_mode` dead keys in `config/civic.json`.
- Consider QR HMAC signing for the unsigned-by-design 7-key JSON QR (per documented design).
- Update AGENTS.md historical test-count references (lines 411, 490 reference "60 pass / 0 fail / 13 suites" — historical session records, not current claims; leave unless rewritten).

---

## Phase 13 — Post-repair review (complete)

Single change made: AGENTS.md:197 test-count gate restated "69 pass / 0 fail / 15 suites / 7 files" → "70 pass / 0 fail / 15 suites / 7 files".

Review checks:
- Original finding CB-AGENTS-03: now **Fixed/Verified** — AGENTS.md matches the live suite (70/0/15/7 confirmed by `npm test`).
- No new defects or regressions: the change is documentation-only; no source, test, config, manifest, or data file was altered.
- Backend/frontend contracts: unchanged (doc-only change).
- Configuration/docs consistency: AGENTS.md now consistent with the live suite; historical references at lines 411/490 are session records, left as-is.
- Data/workspace integrity: `data/residents.db`, `data/photos/`, `data/backups/`, `config/civic.json`, `.crypto-key/` preserved untouched; no temporary artifacts left behind.
- Final diff against baseline: only AGENTS.md line changed, plus pre-existing version bumps (1.0.35→1.0.36) that pre-date this task.

## Phase 14 — Final report and handover

**A. Overall result: VERIFIED.** Scope: documentation gate alignment (CB-AGENTS-03). The test gate in AGENTS.md now matches the live suite. No functional, security, or data-integrity defects were found; no source behavior changed.

**B. Project baseline:** Root `/mnt/d/Barangay/barangay-id-system`, Git `master` @ `514a70a` (ahead 3 vs origin/master), version 1.0.36, 8 pre-existing modified files (version bumps preserved).

**C. Scope and architecture:** Full inventory in sections 4–6 above; exclusions noted (Rust compile blocked on host, no visual/render environment, network not applicable).

**D. Findings:** CB-AGENTS-03 (Low, Confirmed) — Fixed/Verified. CB-002/CB-005/CB-006 previously dismissed with file:line evidence (re-verified this session).

**E. Repairs implemented:** AGENTS.md:197 — "69 pass / 0 fail / 15 suites / 7 files" → "70 pass / 0 fail / 15 suites / 7 files". Reason: live suite runs 70 tests; docs understated the gate. No compatibility or operational effect (documentation only).

**F. Validation results:**
| Check | Command | Result |
|---|---|---|
| Test suite | `npm test` | PASS — 70 pass / 0 fail / 15 suites / 7 files |
| CSS freshness | `npm run check:css` | PASS — CSS fresh, exit 0 |
| Dependency audit | `npm audit` | PASS — 0 vulnerabilities |

**G. Data and workspace integrity:** User data preserved untouched; only AGENTS.md (docs) and CODEBASE_AUDIT.md (this report) written by this task; pre-existing version bumps unchanged; no temporary artifacts.

**H. Security and privacy:** No secrets printed/committed/transmitted; no credentials rotated; no security scan beyond `npm audit` (0 vulns); no network/external/privileged operations. No claim of complete security.

**I. Remaining issues and risks:** None from this task. Environmental limits documented: Rust compile/test/clippy/doc blocked on this host (CI-only); visual/render checks not performed (no graphical environment).

**J. Optional recommendations:** Remove/wire `qr_hmac_secret`/`qr_minimal_mode` dead keys in `config/civic.json`; consider QR HMAC signing (both non-required).

**K. Final acceptance checklist:**
- Functional integrity: PASS — suite green, no regressions.
- Architectural coherence: PASS — no structural change.
- Backend/frontend consistency: PASS — unchanged.
- Security/data integrity: PASS — no exposure introduced.
- Validation quality: PASS — applicable checks ran green; blocked checks disclosed.
- Configuration/dependencies: PASS — consistent, no new deps.
- Workspace/data integrity: PASS — user data preserved.
- Documentation/handover: PASS — CODEBASE_AUDIT.md updated.

No network, external service, privileged action, migration, or production resource was used. Work confined to the project directory.
