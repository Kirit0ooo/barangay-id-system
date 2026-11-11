# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.0.x   | :white_check_mark: |

Only the latest release is actively supported. Older versions do not receive security updates.

## Reporting a Vulnerability

If you discover a security vulnerability, please report it privately:

1. **Do not** open a public GitHub issue.
2. Contact the maintainer directly through the repository's contact channel.
3. Include: affected version, steps to reproduce, and potential impact.

All reports will be acknowledged within 72 hours. You will receive status updates until the issue is resolved.

## Security Architecture

### Application type

Offline-first Tauri v1 desktop application for barangay ID registration. Runs entirely on the local machine (Windows-primary; WSL/Linux for CI only). Core functions require no network access.

### Data flow

```
Frontend (Alpine SPA)
  → Tauri invoke wrappers (api.js)
  → Tauri commands (backend/src/src/*.rs)
  → sqlx SQLite (data/residents.db)
  → JSON response back to frontend
```

- All IPC is asynchronous and parameterized (no string-interpolated SQL).
- Photos stored base64→JPEG under `data/photos/`; max 6 MB binary (~8 MB base64).
- Bulk ID printing is HTML-only (card-template.html via hidden iframe). No PDF generation exists.

### Data isolation

All program data lives inside the project tree:

| Location | Contents |
|----------|----------|
| `data/residents.db` | SQLite database (plaintext or AES-256-GCM encrypted) |
| `data/photos/` | Resident photos (plaintext or individually encrypted) |
| `data/backups/` | Database backups (30-day retention) |
| `config/civic.json` | Barangay civic configuration |
| `Documents/Excel/` | Excel export/import workbooks |
| `logs/` | Application logs |
| `tmp/` | Temporary/cache files |
| `.crypto-key/key.bin` | 32-byte AES-256 encryption key |

Nothing is written to AppData, TEMP, or any location outside the project root.

## Encryption at Rest

File-level AES-256-GCM encryption for `data/residents.db` and `data/photos/*`:

| Component | Detail |
|-----------|--------|
| Crypto module | `backend/src/src/crypto.rs` |
| Key storage | `.crypto-key/key.bin` — 32-byte random key, generated on demand, never rotated |
| File format | `[magic "BRGYENC1"][version][nonce(12)][ciphertext+GCM tag]` |
| Random nonce | Per file, never reused |

### Threat model

**Protected against:** An attacker who copies data files without also copying `.crypto-key/key.bin`.

**NOT protected against:** An attacker who has both the key file and the data. The key sits beside the data by design (operator request).

### Known boundary gaps

- Pre-existing plaintext backups (created before encryption was enabled) remain readable.
- Photos encrypted individually; new photos added while the DB is unencrypted remain plaintext until explicitly encrypted.
- The encryption key is not rotated. If the key is compromised, re-encrypt with a new key and delete the old one.

## Frontend Security

| Area | Control |
|------|---------|
| SQL injection | All queries use parameterized SQL via sqlx; LIKE wildcards escaped (`escape_like`) |
| XSS in card template | `textContent` for text; `SAFE_IMG` allowlist drops `javascript:` and other unsafe schemes |
| CSP | `script-src` includes `'unsafe-eval'` (required by Alpine.js `new Function`); no `font-src` (bundled fonts under `default-src 'self'`) |
| Logging | Frontend log buffer in localStorage; newline injection stripped; single-message size capped |
| QR codes | Plain 7-key JSON, unsigned-by-design; completeness gate via `validate_resident_complete` |

## Backend Security

| Area | Control |
|------|---------|
| Search data leak | Digit-free (name-only) queries bind `NULL` to the contact OR branch — no table leakage |
| Status precedence | Operator remarks (LOST/RENEW/DAMAGE) outrank generic expiry |
| FK orphan guard | `delete_resident` checks `emergency_contact_resident_id` before soft-deleting |
| DOB validation | Rejects future dates and dates more than 120 years before today |
| Explicit columns | All queries list columns explicitly (`RESIDENT_COLUMNS`); no `SELECT *` |
| WAL checkpoint | `checkpoint_wal_truncate` inspects the `busy` column; a busy checkpoint fails the operation instead of writing a possibly-incomplete file |
| Export header guard | Header/width desync returns an error instead of panicking |
| Log truncation | Multi-byte character boundary checked before truncation (no panic on UTF-8 straddle) |

## Security Best Practices for Operators

1. **Generate an encryption key** (`encryption_generate_key`) before encrypting the database or photos.
2. **Protect `.crypto-key/key.bin`** — back it up separately from the data files. Anyone with both can decrypt everything.
3. **Encrypt backups too** — `backup_database` copies the DB in its current state (encrypted or plaintext). Plaintext backups created before encryption remain readable.
4. **Run photo GC** regularly — `photo_gc` removes orphan photos after each scheduled backup.
5. **Do not expose the app to the network** — it is a local desktop application with no server mode.
6. **Keep the app updated** — only the latest release receives security patches.

## Dependency-level findings

### `zip` crate (Rust) — investigated, not applicable

The `zip` crate appears as a **transitive dependency** (not a direct dependency) of this project:

| Direct dependency | Uses `zip` | `zip` version | Dual-header (<= 9.0.4)? | Symlink DoS (>= 1.3.0, < 9.0.0)? |
|-------------------|------------|---------------|-------------------------|-----------------------------------|
| `calamine 0.24.0` (Excel import) | Yes | `0.6.6` | Yes | No |
| `rust_xlsxwriter 0.78.0` (Excel export) | Yes | `2.6.1` | Yes | Yes |

**Finding:** Both transitive `zip` versions fall within at least one affected range:
- `zip 0.6.6` (via `calamine`) is affected by the dual-header CWE-436 interpretation conflict (<= 9.0.4).
- `zip 2.6.1` (via `rust_xlsxwriter`) is affected by both the dual-header conflict (<= 9.0.4) and the symlink `Vec::with_capacity()` DoS (>= 1.3.0, < 9.0.0).

**Not exploitable in this project:** Both vulnerabilities require `ZipArchive::extract()` (or `ZipStreamReader::extract()`) to be called on attacker-supplied input, triggering symlink extraction or dual-header parsing with a security gateway differential. This project does **none** of those:
- It does **not** accept user-submitted ZIP files for extraction.
- It does **not** call `ZipArchive::extract()`, `ZipStreamReader::extract()`, or `ZipArchive`/`ZipStreamReader` directly in any source file.
- `calamine` reads `.xlsx` cell data into memory — it does not extract files to the filesystem and does not process symlinks.
- `rust_xlsxwriter` only **writes** `.xlsx` files; it never reads or extracts archives.

**Action:** No code change required. When the Rust toolchain becomes available on this host (currently blocked — no `libsoup-2.4`, no MSVC `lib.exe`), run `cargo update -p zip` to pull in patched versions (`zip >= 9.0.0` for the symlink DoS, `zip >= 9.0.5` for the dual-header conflict) as a precaution.

### `glib` crate (Rust) — investigated, not applicable

The `glib` crate appears as a **transitive dependency** (not a direct dependency) of this project:

| Direct dependency | Uses `glib` | `glib` version | Affected (>= 0.15.0, < 0.20.0)? |
|-------------------|-------------|----------------|-----------------------------------|
| GTK/webkit2gtk stack (via Tauri, Linux GUI) | Yes | `0.15.12` | Yes |

`glib-sys` is also present at version `0.15.10` as a transitive dependency of `glib`.

**Finding:** `glib 0.15.12` is within the affected range (`>= 0.15.0, < 0.20.0`, patched in `0.20.0`). The `VariantStrIter::impl_get` function is unsound: it passes an immutable reference `&p` to a `*mut libc::c_char` initialized to NULL as an out-argument to the variadic C function `glib_sys::g_variant_get_child`, which mutates the pointer behind `&p` in-place. Under optimization this is undefined behaviour and can cause NULL pointer dereference crashes.

**Not exploitable in this project:** The unsoundness is triggered only when iterating `glib::Variant` strings via `VariantStrIter` (Iterator / DoubleEndedIterator implementations). This project:
- Does **not** declare `glib` as a direct dependency.
- Does **not** call `glib::Variant`, `VariantStrIter`, or any `g_variant_get*` function in any source file.
- Uses the GTK/webkit2gtk crates only as part of Tauri's Linux GUI stack; the project targets Windows primarily (webview2), where these crates are not compiled.

**Action:** No code change required. When the Rust toolchain becomes available on this host (currently blocked — no `libsoup-2.4`, no MSVC `lib.exe`), run `cargo update -p glib` to pull in `glib >= 0.20.0` as a precaution.

## Disclosure Policy

- Vulnerability reports are acknowledged within 72 hours.
- A fix timeline is provided within 7 days of confirmation.
- Public disclosure occurs after a fix is released or after 90 days, whichever comes first, unless the reporter requests otherwise.
- No bounty program is currently offered.

## Security Contacts

Repository contact channel for security reports — do not file public issues for security matters.