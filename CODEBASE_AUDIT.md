# CODEBASE_AUDIT.md — Initial Assessment Report (Phase 4)

**Framework:** CODEBASE_REVIEW.md v2.0 (14-phase evidence-based audit)
**Status:** Phase 1–2 complete (inventory + architecture), Phase 3 verification done, Phase 4 report below
**Repo:** `/mnt/d/Barangay/barangay-id-system`
**Branch:** `master` @ `11903a3` (fix(csp): sharpen card-template.html safeguard comment), working tree clean, ahead of origin/master by 1
**Assessed:** 2026-11-11
**Version:** 1.0.20 (all manifests synced)

---

## 1. Verified green gates (baseline)

| Gate | Result | Evidence |
|---|---|---|
| `npm test` | 60 pass / 0 fail / 13 suites / 7 files | `m00154` (smoke, regression, golden-card, bulk-progress, logic, structure, preview-window) |
| `npm run check:css` | CSS fresh, exit 0 | `m00155` |
| `npm audit` | 0 vulnerabilities | documented in AGENTS.md quality gates |
| `cargo fmt --all -- --check` | clean, exit 0, all 11 files | AGENTS.md (2026-11-06) |
| `cargo metadata --locked` | exit 0 (manifest + lockfile integrity) | AGENTS.md |
| Rust compile / test / clippy / doc | BLOCKED on this host | no `libsoup-2.4`, no MSVC `lib.exe`; CI-only via `.github/workflows/ci.yml` |

**Note — AGENTS.md test-count staleness:** AGENTS.md repeatedly states "57 pass / 0 fail / 6 suites" but the live suite runs **60 pass / 0 fail / 13 suites / 7 files** (`tests/preview-window.test.js` is the 7th file; AGENTS.md lists only 6). AGENTS.md is out of date on this point and should be restated to 60/0/13/7.

**Version drift:** AGENTS.md's launcher-rework section references a historical split state (`package.json` 1.0.3 vs others 1.0.1). The live tree is fully in sync at **1.0.16** across `package.json`, `package-lock.json`, `backend/src/tauri.conf.json`, `backend/src/Cargo.toml`, `backend/src/Cargo.lock`, `frontend/src/index.html` (About card), and `README.md`. No action needed.

---

## 2. Advisor findings investigated (primary-source verification)

CODEBASE_REVIEW.md requires evidence tied to file:line; advisor output is not treated as evidence. Three "critical/high" claims were checked and **dismissed** — each is already soundly defended in the shipped code:

| Claim | Advisor location | Primary-source reality | Verdict |
|---|---|---|---|
| CB-006 SQL injection via `VACUUM INTO '{}'` string interpolation | `scheduled.rs` `scheduled_backup` fallback | The interpolated `tmp_str` derives from `Local::now()` (`scheduled_YYYYMMDD_HHMMSS.db`), is gated by `is_machine_backup_name()` (strict 15-char digit+underscore pattern, scheduled.rs:33-45), and single quotes are escaped `'\''`→`"''"` before interpolation (scheduled.rs:193-194). Not attacker-controlled. | **Dismissed — already defended** |
| CB-002 "strict YYYY-MM-DD only" date parsing rejecting valid formats | `residents.rs` `normalize_valid_until` | `parse_any_date` accepts YYYY-MM-DD, DD-MM-YYYY, DD/MM/YYYY (residents.rs:249-278). Unit tests cover all three formats plus invalid inputs (residents.rs:2013-2022). | **Dismissed — already supported** |
| CB-005 path traversal bypass in `validate_isolated_write_path` | `db.rs` | Function performs lexical normalization + `..` rejection + null-byte rejection + canonical-parent check + symlink escape check (db.rs:477-546). Unit tests reject `..\..\Windows\System32\evil.txt`, `Documents/../../evil.txt`, and relative traversal variants (db.rs:889-1091). | **Dismissed — already defended** |

No finding was accepted without a file:line quote from the actual source.

---

## 3. Genuine observations (not defects — documentation / process gaps)

These are not regressions or security defects; they are noted so the next agent doesn't mistake "undocumented" for "broken":

1. **Rust changes are not compiler-verifiable on this host.** `cargo check/test/clippy/doc` are blocked (no `libsoup-2.4`, no MSVC `lib.exe`). CI (`cargo test --manifest-path backend/src/Cargo.toml`) is the compiler gate. Any Rust edit must be reviewed statically and validated in CI before merge.
2. **Encryption key sits beside data** (`.crypto-key/key.bin` next to `data/residents.db`). This is the documented threat-model boundary (AGENTS.md "Encryption at rest"): protects against file-copy-only attackers, not against an attacker with both key and data. Pre-existing plaintext backups and plaintext photos added while the DB is unencrypted remain readable — documented as known boundary gaps.
3. **`tmp/` is gitignored** and used for throwaway harness probes (e.g. the `Invoke-PatchVersionBump` harness was run there and deleted). Keep it out of commits.

---

## 4. What was NOT done (scope boundary, per CODEBASE_REVIEW.md)

- No files were written, edited, deleted, or moved (read-only assessment).
- No user data touched: `data/residents.db`, `data/photos/`, `data/backups/`, `config/civic.json`, `.crypto-key/` preserved untouched.
- No network, no push, no deploy, no privileged operations, no secrets printed.
- No fabrication: every claim above tied to a file:line or a live command result.

---

## 5. Recommended next steps (Phase 5 remediation plan)

If the user wants remediation work, the smallest complete plan is:

1. **Update AGENTS.md test-count line** (57/0/6 → 60/0/13/7) and remove the outdated launcher-rework version-drift paragraph (tree is at 1.0.16, fully synced). One `docs` branch.
2. **Re-verify after any change:** `npm test` (60/0/13/7), `npm run check:css` (CSS fresh), `npm audit` (0), plus `cargo fmt --check` for any Rust edit.
3. **Rust edits** (if any): static review only locally; validate in CI. Never claim compiler-verified on this host.

No security fix is required from the advisor's three headline claims — all three are already defended in the shipped code with tests.

---

**End of Phase 4 initial assessment.** Ready for user direction on whether to proceed to Phase 5 (remediation plan) and any specific defect to fix.
