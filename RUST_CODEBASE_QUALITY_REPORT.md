# Rust Codebase Quality Report

Pass executed 2026-11-06. Scope: `src-tauri/src/*.rs` (11 modules, the whole Rust crate) plus
`tests/regression.test.js`. Frontend, database, photos, backups and configuration were not
modified.

Base revision: `41e7d1f` (2026-11-05 15:52). All work is left **uncommitted** so it stays
separable from the pre-existing uncommitted work found in the tree (see section H).

This pass followed `CODEBASE_REVIEW.md` in the project root.

---

## A. Overall Assessment

**Original condition.** The crate was already well organised, split into single-purpose
modules, with no `unsafe` code, no `panic!`/`todo!`/`unimplemented!` and no obviously wrong
error architecture. Its weaknesses were narrower than the file count suggests:

1. Documentation was uneven. Roughly 1 in 4 functions carried a rustdoc comment, and almost
   none documented failure behaviour or side effects. `residents.rs` — the largest module at
   ~2,200 lines — had 87 doc lines for the whole file.
2. Five call sites ran `PRAGMA wal_checkpoint(TRUNCATE)` and deliberately discarded the
   result (`let _ = ...`). That statement does not fail on a busy database; it *succeeds* and
   reports busy in the returned row. So the old code could copy or encrypt a `residents.db`
   that was missing every transaction still sitting in the WAL, and report success.
3. Three production panics were reachable in principle: a byte-offset `String::truncate` on
   frontend log text, a re-parse `expect()` after date normalisation, and an `assert_eq!` on
   export header/width width in the Excel writer.
4. Test coverage stopped at 43 functions, concentrated in validation helpers. The WAL
   checkpoint helper, the logger sanitisers and the export column mapping had none.
5. Naming was already fully conformant, and `Result`/`Option` were already used for their
   intended meanings.

**Condition after this pass.** All five points above are addressed, with 11 new tests
(43 to 54) and roughly 950 new documentation lines. The three production panic paths are
gone. The WAL checkpoint is now a single guarded helper whose failure aborts the operation
instead of silently producing an incomplete backup.

**What is not proven.** The Rust crate could not be compiled, linted, tested or documented on
this host, because the native dependencies it links cannot be built here. Section G records
the exact commands and the exact blockers. The Rust work is therefore **statically reviewed
and parse-verified, but not compiler-verified**. Section I lists what CI must confirm.

---

## B. Documentation Improvements

Rustdoc lines across the crate: **1,041**, concentrated in the modules with the most
behaviour to explain.

| Module | Doc lines | Notes |
|---|---|---|
| `residents.rs` | 349 | was 87. Validation rules, soft-delete semantics, photo pipeline, search escaping |
| `db.rs` | 190 | path isolation rules, schema fields, `checkpoint_wal_truncate` contract |
| `export.rs` | 133 | column mapping, import result fields, rejection-reason shape |
| `crypto.rs` | 77 | file format, nonce handling, `# Panics` for the three `expect()` calls |
| `scheduled.rs` | 66 | backup flow, photo GC result fields |
| `config.rs` | 53 | `CivicConfig` fields and fallback behaviour |
| `backup.rs` | 53 | atomic copy-then-rename sequence, retention |
| `main.rs` | 50 | every Tauri command wrapper, `# Errors`, `# Side Effects` |
| `logger.rs` | 42 | sanitiser bounds and why they exist |
| `id_card.rs` | 18 | unchanged content, verified still accurate |
| `encryption.rs` | 10 | command-level contract |

Sections used, and only where the implementation justifies them:

- `# Arguments`, `# Returns`, `# Errors`, `# Panics`, `# Side Effects`.
- `# Panics` appears in exactly three places, all on the three `expect()` calls in
  `crypto.rs`, and each states the invariant that keeps it unreachable: the AES-256 key is an
  invariant `[u8; 32]` held by `CryptoKey` (`crypto.rs:70`, `crypto.rs:115`), and the GCM
  implementation does not fail for in-memory buffers (`crypto.rs:76`).
- `# Errors` is used on every fallible function whose failure is not obvious from its name.

Public items documented: all `pub fn` in `db.rs`, `crypto.rs`, `logger.rs`, `config.rs`,
`backup.rs`, `encryption.rs`, `scheduled.rs`, `export.rs`, `residents.rs` and every Tauri
command wrapper in `main.rs`. Struct and enum fields that carry a unit or a derived meaning
(for example "Rows left untouched: unchanged, blank, unmatched, or rejected") were documented
in place.

**Verification of the documentation itself.** Phase Nine requires docs to match behaviour, so
selected claims were read back against the implementation:

| Claim in docs | Implementation | Result |
|---|---|---|
| `main.rs` `log_frontend` "Never" errors | body is `logger::append_frontend_log(...); Ok(())` | accurate |
| `main.rs` `run()` "logged and exits with status 1 instead of panicking" | `std::process::exit(1)` after logging (F6 comment) | accurate |
| `main.rs` "Sets TMP/TEMP/TMPDIR/WEBVIEW2_USER_DATA_FOLDER" | `env::set_var` for all four at lines 106-109 | accurate |
| `crypto.rs` `# Panics` on the key-length invariant | `new_from_slice` on an invariant `[u8; 32]` | accurate |
| `logger.rs` "capped at `MAX_LOG_MESSAGE_BYTES`" | constant is 8000, applied in `sanitize_log_message` | accurate |
| `db.rs` `per_resident_export_stem` "Used for per-resident exports" | no Rust caller; kept deliberately, see section I | comment corrected to say so |

**Not documented, and why.** None of the 11 files was left undocumented. Two deliberate
gaps:

- No `# Examples` blocks were added, so there are no runnable doc tests. The functions worth
  demonstrating need a live SQLite database, a photo directory, or both; an example that
  cannot run would be worse than none. `cargo test --doc` therefore has nothing to collect.
- `id_card.rs` received no new comments. Its existing comments were checked against the code
  and are still accurate, and the module is generated-HTML assembly that reads clearly.

---

## C. Naming Improvements

**One rename was made, during the post-implementation review.** The initial audit found no
pre-existing violations, so nothing was renamed merely to look consistent. The review against
the loaded Rust guide then found that this pass's own return-type change had left one private
helper misnamed: `parse_any_date_to_iso` now returns `chrono::NaiveDate`, not an ISO string, so
the name contradicted both its signature and its own doc ("into a real calendar date"). It is
now `parse_any_date` — definition at `residents.rs:263`, callers at `:285` and `:298`. No
public name changed.

Checked mechanically across all 11 files:

| Rule | Check | Result |
|---|---|---|
| Functions `snake_case` | regex for a capital inside any `fn` name | 0 violations |
| Structs and enums `UpperCamelCase` | regex for a lowercase first letter after `struct`/`enum` | 0 violations |
| Enum variants `UpperCamelCase` | regex for lowercase variants | 0 violations |
| `pub const` `SCREAMING_SNAKE_CASE` | regex for lowercase after `pub const` | 0 violations |
| Modules `snake_case` | filenames | all conformant |

Names also express domain meaning rather than implementation detail
(`validate_isolated_write_path`, `photo_gc`, `escape_like`, `resolve_unique_path`,
`backup_base_name`), and no ambiguous abbreviations were found.

**Public names deliberately preserved** (the spec's compatibility safeguard). These exist for
frontend, database or historical reasons and were left alone:

| Name | Why it cannot be renamed |
|---|---|
| `per_resident_pdf_filename`, `sanitize_file_name_component` | asserted by name in `tests/regression.test.js:147-148` and by the export path |
| `get_qr_data_url`, `get_cards_batch`, `optimize_database_cmd` | invoked from `src/js/api.js` by exact string |
| All `Resident` struct fields | serialised straight to the frontend and to `data/residents.db`; a rename changes the wire format and the import/export columns |
| `civic.json` keys (`photo_gc_dry_run`, etc.) | read from the operator's config file on disk |
| `BRGYENC1` magic, `BACKUP-YYYY-MM-DD` stems | written into existing files on disk |

---

## D. Error-Handling Improvements

### D1. Discarded WAL checkpoint errors (the most serious finding)

Five call sites used `let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&pool).await;`
in `backup.rs` (x3), `encryption.rs` (x1) and `scheduled.rs` (x1). `PRAGMA wal_checkpoint`
always returns a row and does not raise an error when another connection still holds the WAL;
the busy flag is *in the row*. Discarding the result meant the code could not tell "flushed"
from "did nothing", and then copied or encrypted a database missing the un-checkpointed
transactions.

Fix: one guarded helper, `pub async fn checkpoint_wal_truncate(pool: &SqlitePool) -> Result<i64, String>`
(`db.rs:786`). It reads the `busy` column and returns an error when it is non-zero, and the
frame count otherwise. All five sites now propagate with `?`:

- `backup.rs:89` (`backup_database:46`), `backup.rs:222` and `backup.rs:254` (`restore_database:173`)
- `encryption.rs:100` (`encryption_encrypt_database`) — before sidecar deletion
- `scheduled.rs:97` (`scheduled_backup:83`) — logs `checkpointed N frame(s)`, used at line 234

**Behaviour change, deliberate.** A backup or encrypt that previously "succeeded" while
missing WAL transactions now fails with
`WAL checkpoint is busy (another connection is still reading); nothing was changed — retry in a moment`.
That is the safe direction (no data is written, the operator retries) but it is a visible
change, so it is listed again in section I.

### D2. `Result` versus `Option`

No changes were needed for correctness: the crate already used `Option` for genuine absence
(optional config fields, optional photo, optional `emergency_contact_resident_id`) and
`Result` for failure. The one new helper returns `Result`, not `Option`, because "busy" is a
failure of the requested flush, not an absent value.

### D3. Panics removed from production paths

| Panic | File | Fix |
|---|---|---|
| `clean_msg.truncate(8000)` panics with "byte index is not a char boundary" when a multi-byte character straddles the offset, and frontend text can put one there | `logger.rs` | extracted `sanitize_log_message`, which walks back to `is_char_boundary` before truncating; `sanitize_log_level` bounds the label to `MAX_LOG_LEVEL_CHARS` (16) |
| `chrono::NaiveDate::parse_from_str(&iso, ...).expect("helper emits ISO")` | `residents.rs:263` | `parse_any_date` now returns `Result<chrono::NaiveDate, String>` directly; `normalize_birth_date:298` and `normalize_valid_until:285` use the typed value and no longer re-parse |
| `assert_eq!(headers.len(), widths.len())` in the Excel writer | `export.rs:234` | returned as `anyhow::anyhow!("headers/widths length mismatch: {} vs {}")`, so a programming slip cannot abort the app during an export |

### D4. Error suppression and dead error paths

- Deleted `check_emergency_fk_pool` (`residents.rs`) — a `SqlitePool`-based emergency-contact
  FK check with **zero callers**. It returned `Result<(), String>`, so it looked live. The
  live guard is the `AppState`-based check used by `delete_resident`. Verified: no remaining
  reference anywhere in `src/`.
- Removed `#[allow(dead_code)]` from `CryptoKey::decrypt_or_plaintext` (`crypto.rs:136`),
  which was wrong: it has a live caller at `residents.rs:1782` (photo decrypt-on-read).

### D5. Remaining error-handling weaknesses

Three `expect()` calls remain in production code (`crypto.rs:70`, `:76`, `:115`). All three
guard invariants that cannot be violated by any input: the key array is a private
`Zeroizing<[u8; 32]>`, and AES-GCM does not fail for in-memory buffers. Each now carries a
`# Panics` section naming the invariant, which satisfies "where a panic is genuinely justified
by an established invariant, document that invariant". They were not replaced with `?` because
doing so would turn an impossible condition into an error path the caller cannot act on.

---

## E. Code Quality and Safety

| Area | Finding |
|---|---|
| `unsafe` | **Zero** occurrences in the crate. Nothing to document or remove. |
| Ownership and borrowing | No unnecessary clones introduced. The one signature change (`parse_any_date`) removes a `String` round trip by returning `NaiveDate` directly. |
| Resource management | The new helper is a plain `async fn` over the shared pool; no lock, handle or temporary is held longer than before. The five call sites are unchanged in ordering, so RAII behaviour is preserved. |
| Concurrency | Untouched. The crate's `AppState` mutex and pool use are unchanged, and the checkpoint helper takes `&SqlitePool` by reference, matching the surrounding code. |
| Visibility | New helper is `pub` inside the crate so `backup.rs`, `encryption.rs` and `scheduled.rs` can call it; nothing was widened beyond that. |
| Dead code | One dead function deleted. Four `#[allow(dead_code)]` items remain: `per_resident_export_stem` and `per_resident_pdf_filename` (`db.rs:306,314`), `key_exists_with_dir` and `encrypt_file_in_place` (`crypto.rs:233,241`). All four carry a comment saying why they are kept, and the first two are asserted by `tests/regression.test.js:147-148`, so deleting them would break an existing test. See section I. |
| Unused imports | Checked the one removal that could have broken the build: `SqlitePool` was dropped from `use sqlx::{FromRow, SqlitePool}` in `residents.rs`. `grep -n "SqlitePool" src-tauri/src/residents.rs` now returns nothing, so the removal is complete rather than a compile error. |
| Dependencies | **None added, none removed, `Cargo.lock` untouched.** `anyhow` was already a direct dependency (`Cargo.toml:17`) and already imported in `export.rs:3`. |

---

## F. Test Coverage

Rust test functions: **43 to 54** (+11). `cargo metadata --locked` confirms the manifest and
lockfile are consistent.

| Module | Before | After | New tests, and what they pin down |
|---|---|---|---|
| `db.rs` | 14 | 16 | `backup_base_name_uses_the_local_date` — the backup stem is local-date based, not UTC. `resolve_unique_path_never_overwrites_an_existing_file` — exporting twice must not destroy the earlier workbook (regression for the P2-8 defect). |
| `export.rs` | 4 | 8 | `column_lookup_is_case_and_space_insensitive` — header matching ignores case and padding, the common operator error. `blank_cells_read_as_absent` — empty cells must not become `Some("")`. `workbook_cells_flatten_to_trimmed_strings` — cell-to-string conversion trims. `full_address_joins_present_parts_in_display_order` — address parts join in display order, including the all-absent case (`""`). |
| `logger.rs` | 0 | 3 | `log_level_keeps_alphanumerics_only_and_bounds_length` — injection punctuation is dropped and the label is capped. `log_message_strips_line_breaks` — one call cannot forge extra log lines. `log_message_cap_never_splits_a_character` — the actual panic regression: a 2-byte character straddling byte 8000 must not panic and must not lose the preceding bytes. |
| `residents.rs` | 13 | 15 | `escape_like_neutralizes_wildcards_for_literal_search` — a typed `%` matches a literal `%`, not "anything", and a backslash is escaped. `format_contact_keeps_legacy_values_and_formats_ph_mobiles` — legacy landlines and short numbers pass through trimmed instead of being rewritten. |

Test quality: every new test asserts behaviour (a returned value, or "does not panic and stays
within the cap"), none asserts "did not crash", none duplicates the implementation, and the
filesystem-touching test uses a `tmp/` directory inside the project and cleans up after
itself, so it stays inside the project boundary.

Regression guards added to `tests/regression.test.js`:

- `checkpoint helper inspects the busy result` — fails if `PRAGMA wal_checkpoint(TRUNCATE)`
  is present without a `busy != 0` check, so the original defect cannot come back silently.
- The `bundle ships MSI` check now asserts NSIS-only, matching `tauri.conf.json` (the MSI
  target is deferred; see the 2026-11-05 section of `AGENTS.md`).

Two unused variables flagged by the project's lint (`idCardFlat`, `goldenQr`) were removed
from that file, and three template literals were converted to backticks. No assertion was
weakened or deleted.

Not added, deliberately: no `#[tokio::test]` integration test for the WAL helper. It needs a
real SQLite database and a second connection holding the WAL to produce the busy state, and a
test that cannot produce that state would pass without testing anything. The frontend
assertion above covers the cheap half of the regression. This is recorded again in section I.

---

## G. Validation Results

Legend: PASS (ran, expected result) / FAIL (ran, unexpected result) / BLOCKED (could not run,
reason identified) / NOT RUN / PARTIAL.

| Check | Command | Result | Evidence |
|---|---|---|---|
| Formatting | `cargo fmt --all -- --check` | **PASS** | exit 0, zero output. All 11 files also pass `rustfmt --check` individually. This also proves every file parses as valid Rust. |
| Manifest and lockfile integrity | `cargo metadata --locked --format-version 1` | **PASS** | exit 0 |
| Frontend suite | `npm test` | **PASS** | 57 tests, 12 suites, 57 pass, 0 fail, 0 skipped, 1126 ms |
| CSS freshness gate | `npm run check:css` | **PASS** | `CSS fresh: markup unchanged since last stamp` |
| Dependency audit (npm) | `npm audit` | **PASS** | `found 0 vulnerabilities` |
| Compilation (host target) | `cargo check --workspace --all-targets --locked` | **BLOCKED** | exit 101: `The system library 'libsoup-2.4' required by crate 'soup2-sys' was not found`. `pkg-config --exists libsoup-2.4` fails; `webkit2gtk-4.0` is also missing. (`gtk+-3.0` 3.24.52 and `glib-2.0` 2.88.4 *are* present.) |
| Compilation (deployment target) | `cargo check --target x86_64-pc-windows-msvc --all-targets --locked` | **BLOCKED** | exit 101: `error occurred in cc-rs: failed to find tool "lib.exe": No such file or directory`, from `libsqlite3-sys v0.27.0` building bundled SQLite. The MSVC librarian is not on this host. |
| Unit and integration tests | `cargo test --workspace --locked` | **BLOCKED** | Same native-dependency blockers as compilation; `cargo test` must link, so it fails earlier still. |
| Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | **BLOCKED** | Same blockers; clippy cannot run without the dependencies building. |
| Documentation tests | `cargo test --doc --workspace --locked` | **NOT RUN** | Blocked by the same cause, and the crate contains no `# Examples` blocks, so there is nothing for it to collect (see section B). |
| Documentation generation | `cargo doc --workspace --no-deps --locked` | **BLOCKED** | Same blockers. |
| Static consistency audit | see below | **PASS** | Compensating check for the blocked compiler; details below. |

No check was skipped in order to hide a failure, and no failing check was deleted, weakened or
disabled.

### Compensating static verification

Because no compiler ran, every symbol touched by this pass was traced by hand. The method and
the result:

| What was checked | Command shape | Result |
|---|---|---|
| Changed signature has consistent callers | `grep -rn parse_any_date src/` | 1 definition + 2 callers, both `?`-propagating the new `NaiveDate` |
| Deleted function has no references | `grep -rn check_emergency_fk_pool src/` | 0 references |
| New helper resolves and is imported | `grep -rn checkpoint_wal_truncate src/` | defined `db.rs:786`, imported in `backup.rs:9` and `encryption.rs:16`, called 5 times |
| Error type of `?` matches | inspected enclosing `fn` signatures | all five call sites are in functions returning `Result<_, String>`, and the helper returns `Result<i64, String>` |
| Import removal left no orphan use | `grep -c SqlitePool src/residents.rs` | 0, so `use sqlx::{FromRow, SqlitePool}` to `use sqlx::FromRow` is safe |
| Macros resolve | `grep -n anyhow src/export.rs`, `grep -n ^anyhow Cargo.toml` | `use anyhow::Result;` at line 3, dependency declared at `Cargo.toml:17` |
| New `ck_msg` binding is used | `grep -n ck_msg src/scheduled.rs` | bound at 98, used at 234, so no unused-variable warning |
| Constants and helpers declared once | `grep -n MAX_LOG src/logger.rs` | one declaration each (98, 160), one definition pair, called at 244-245 |
| Every helper the new tests call exists | 16-name existence sweep | all 16 present |
| No production panic added | occurrence sweep + line vs `#[cfg(test)]` boundary | 0 `unsafe`, 0 `panic!`/`todo!`/`unreachable!`/`unimplemented!`; all 34 `unwrap()` are inside `#[cfg(test)]`; 3 `expect()` remain, all documented |
| Naming conformant | 4 regex sweeps | 0 violations |

This is strong evidence, not equivalent to a compiler. A type error in the new helper or a
missed caller would still be caught only by the compiler.

---

## H. Files Modified

Rust and test changes from this pass: **1072 insertions / 109 deletions**, measured with
`git diff --numstat -- src-tauri/src tests`. The per-file figures below are total lines
changed (insertions plus deletions).

| File | Change | Purpose |
|---|---|---|
| `src-tauri/src/db.rs` | +219 | new `checkpoint_wal_truncate`; schema and path-isolation rustdoc; 2 new tests |
| `src-tauri/src/residents.rs` | +347 | rustdoc (87 to 349 doc lines); typed date parser; dead `check_emergency_fk_pool` deleted; 2 new tests |
| `src-tauri/src/export.rs` | +195 | `assert_eq!` to returned error; result-field rustdoc; 4 new tests |
| `src-tauri/src/logger.rs` | +102 | char-boundary-safe truncation helpers and constants; 3 new tests |
| `src-tauri/src/backup.rs` | +80 | 3 WAL call sites now guarded and propagated; flow rustdoc |
| `src-tauri/src/scheduled.rs` | +76 | guarded checkpoint with frame count; photo GC result rustdoc |
| `src-tauri/src/crypto.rs` | +47 | `# Panics` sections; wrong `#[allow(dead_code)]` removed |
| `src-tauri/src/main.rs` | +47 | rustdoc only; no code change |
| `src-tauri/src/config.rs` | +39 | rustdoc only; no code change |
| `src-tauri/src/encryption.rs` | +15 | guarded checkpoint before sidecar deletion; rustdoc |
| `tests/regression.test.js` | +14 / -10 | new busy-result guard; MSI check realigned to NSIS-only; 2 unused variables removed; backtick literals |
| `RUST_CODEBASE_QUALITY_REPORT.md` | new | this report |

Not modified by this pass: `src-tauri/src/id_card.rs`, `Cargo.toml`, `Cargo.lock`, all of
`src/`, `tests/` other than `regression.test.js`, `config/`, `data/`, `tools/`, and the
launchers.

### Pre-existing changes found in the tree, verified but not authored here

These were already uncommitted when this pass started (file mtimes 2026-11-05, before the
10:46 session start). They were inspected so this report does not misattribute them, and
they were left exactly as found:

| File | State found |
|---|---|
| `AGENTS.md` | documents the 2026-11-05 installer-toolchain work |
| `build.ps1` | WiX and NSIS prerequisites relaxed from required to warn-only |
| `src-tauri/tauri.conf.json` | `useLocalToolsDir: true`; `targets` changed from `["msi","nsis"]` to `["nsis"]` |
| `src-tauri/wix/main.wxs` | 23-line stub replaced by the 321-line per-user template |
| `package.json` | version `1.0.1` to `1.0.3` |

**No commit was made.** Bundling a commit would have mixed this pass with the unfinished
installer work above, which is not mine to finalise.

---

## I. Remaining Issues

**Verification gaps (the important ones).** Rust compilation, clippy, unit tests and doc
generation did not run. The changes are parse-verified and traced by hand, but not accepted by
a compiler. Concretely, something in section G's static audit could still be wrong in a way
only the compiler sees: a trait-implementation mismatch, an incorrect tuple type for the
`PRAGMA` row, or a lifetime issue in a new test.

What CI must confirm, in this order:

1. `cargo test --manifest-path src-tauri/Cargo.toml` via `.github/workflows/ci.yml`, or
   `build.bat` / `cargo test` on the Windows side. Expect 54 tests to run.
2. `cargo clippy --all-targets -- -D warnings` — this pass added a `pub` helper and one
   import change; clippy has not seen either.
3. `cargo fmt --all -- --check` is already clean and should stay clean.

**Behaviour changes an operator could notice** (all deliberate, all in the safe direction):

- A backup, restore, encrypt or scheduled backup now **fails** with a retry message when the
  WAL checkpoint is busy, instead of completing with a database that may be missing
  un-checkpointed transactions. A run that previously reported success may now report
  "retry in a moment".
- A header/width mismatch in the Excel writer now returns an error instead of panicking. The
  app stays alive rather than closing.
- Frontend log messages longer than 8000 bytes are truncated at a character boundary instead
  of panicking. Messages that previously crashed the log call are now written, shortened.

**Known weaknesses not addressed:**

| Item | Why it was left |
|---|---|
| Four `#[allow(dead_code)]` items (`db.rs:306,314`, `crypto.rs:233,241`) | Each has a comment explaining its purpose (legacy alias, reserved encrypted-backup path). The first two are asserted by `tests/regression.test.js:147-148`; deleting them would break an existing test, which the spec forbids without a documented reason. `per_resident_export_stem`'s comment was corrected to say it currently has no Rust caller. |
| The WAL busy path has no automated test | Producing a genuinely busy WAL needs a second live connection and a real database file. A test that cannot reach that state would assert nothing. The frontend check covers the "was the result inspected" half. |
| Rust verification is CI-only | The host cannot build `soup2-sys`, `webkit2gtk-sys` (Linux) or bundled SQLite for MSVC (Windows). Installing a GTK/WebKit stack or the MSVC toolchain is a system change that needs separate authorisation. |
| Pre-existing uncommitted installer work | Out of scope for a Rust quality pass. Left untouched and uncommitted so it can be reviewed on its own. The deferred MSI target still needs the template fixes described in `AGENTS.md` before `"msi"` can return to `targets`. |

---

## J. Final Status

**CONDITIONALLY COMPLETE.**

All material improvements are in place and every one is backed by either a passing local check
or a hand-traced static check: documentation, error handling, panic removal, dead-code
removal, 11 new tests, and formatting. The frontend suite, the CSS gate and the dependency
audit pass with their original numbers.

It is not **VERIFIED**, because the acceptance criterion "verify that all changes compile and
behave correctly" could not be met on this host: the Rust crate cannot build here for either
target, so no compiler, clippy or test run has examined this pass's Rust changes. Until CI or
a Windows-side build runs, the correct reading of the Rust work is "reviewed and
parse-verified, compilation unconfirmed".

Matched against the spec's Phase Nine checklist:

| Requirement | State |
|---|---|
| Every function has meaningful documentation | met for the 11 modules |
| Public APIs have rustdoc; errors and panics documented | met; 3 `# Panics` on the only 3 production `expect()` |
| Naming conventions consistent | met; audit found 0 violations, 0 renames |
| Public API compatibility preserved | met; no public name, field or wire format changed |
| `Result` and `Option` used for their intended meanings | met |
| Unjustified panics addressed | met; 3 production panic paths removed |
| Unsafe documented | not applicable; 0 `unsafe` |
| Critical functions tested, normal and failure cases | met on this host's limits; 11 added, the WAL busy path cannot be unit-tested locally |
| No regression in working functionality | no behaviour regression introduced; three deliberate, safer changes flagged above |
| No unnecessary dependencies | met; `Cargo.lock` untouched |
| Existing tests intact | met; none deleted or weakened, `npm test` 57/57 |
| Docs match behaviour | met for the claims spot-checked (section B table) |
| No unrelated files or user data touched | met; mtimes prove only the 10 Rust files and `tests/regression.test.js` came from this pass |
| Compilation, tests, clippy, doc verified | **not met - BLOCKED**, exact reasons in section G |
