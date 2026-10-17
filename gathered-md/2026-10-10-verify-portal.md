# Verify Portal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A phone scan of the card QR auto-pops a LEGITIMATE/INVALID verdict from a static hosted page, backed by the existing Ed25519 backend.

**Architecture:** Phase A builds `verify-site/` standalone against fixtures (works, testable, shippable UI without backend changes). Phase B switches the card QR to the signed URL + adds base-URL settings + trust publish. Phase C ties both ends with a cross-language golden vector and rollout docs.

**Tech Stack:** Vanilla JS ES modules (browser + node --test), TweetNaCl verify-only (vendored, public domain), WebCrypto SHA-256, Rust (existing `verify.rs`/`config.rs`/`id_card.rs`), Netlify static hosting.

**Spec:** `docs/superpowers/specs/2026-10-10-verify-portal-design.md` — the plan argues from the spec; executors read both.

## Global Constraints

- Tauri v1 command pattern: `#[tauri::command] pub async fn name(...) -> Result<T, String>`, registered in `tauri::generate_handler!` in `src-tauri/src/main.rs` (existing entries e.g. `config::rotate_verify_key` at ~line 104).
- JS tests run via `npm test` = `node --test tests/*.test.js`; new tests go in `tests/verify-portal.test.js` and must be added to the `test` script list in `package.json`.
- `tools/check-css.mjs` only fingerprints `src/index.html` + `src/js/app.js` — `verify-site/` is exempt, keep its styles self-contained in `verify-site/styles.css`.
- Offline-first: zero CDN, zero fonts, zero trackers in `verify-site/`. No resident PII in the site (verdict-only).
- Private seed never leaves `config/id_verify_private` (already gitignored); `verify-site/trust.json` (public keys) is generated, gitignored, deployed.
- No git binary in this workspace — task steps end with test runs, not commits.
- Exact pinned values: `VERIFY_SCHEMA = 1` (`verify.rs:28`); sign-message bytes `v{kid}|{resident_id}|{valid}|{status}|{hex(hash)}` (`verify.rs:128-145`); fragment `{v}.{kid}.{resident_id}.{valid}.{status}.{hash_b64}.{sig_b64}` with URL_SAFE_NO_PAD (`verify.rs:170-183`); kid = first 8 hex of pubkey (`config.rs:213`); base URL must start `https://`, no spaces (`config.rs:492-497`).

## Review Focus

- Status text containing dots (e.g. `"ACTIVE. REISSUED"`) must still parse and verify — middle-field rejoin rule, tested in Task A1.
- Non-ASCII status (e.g. `"ACTIVO — Ñ"`) must verify — UTF-8 message bytes must match Rust byte-for-byte, tested via golden vector in Task C1.
- Unknown `kid` (pre-rotation card hitting new `trust.json`) must fail closed with reason, never throw, tested in Task A3.
- Empty fragment load must render landing state with no popup and no console errors, tested in Task A4 (string pins) + manual matrix.
- Signed URL length must stay printable/scanable — Rust test pins `QrCode::new(url).is_ok()` plus a length bound, Task B1.

## File Structure

- Create `verify-site/js/verify-lib.js` — pure functions (no DOM): `parseFragment(frag)`, `b64urlToBytes(s)`, `bytesToHex(b)`, `buildSignMessage(f)`, `isLegacyPayload(s)`. Importable by both `app.js` (browser) and node tests. (Spec §3 lists `app.js`; lib split keeps testable logic DOM-free.)
- Create `verify-site/app.js` — UI state machine (landing → verifying → LEGITIMATE | INVALID | LEGACY), `trust.json` fetch + shape check, SubtleCrypto SHA-256, seal popup wiring, typed-ID fallback form.
- Create `verify-site/crypto/nacl.min.js` — vendored TweetNaCl (verify path only used: `nacl.sign.detached.verify`).
- Create `verify-site/index.html`, `verify-site/styles.css`, `verify-site/_headers` (Netlify HSTS + strict CSP), `verify-site/README.md` (operator runbook).
- Generate (never commit) `verify-site/trust.json` — serialized `config::VerifyTrust` (`{active_kid, keys: {kid: {public 64-hex, created}}}`).
- Modify `src-tauri/src/id_card.rs` (single + bulk QR paths), `src/index.html` + `src/js/app.js` (settings field), `src-tauri/src/config.rs` or `verify.rs` (publish command), `src-tauri/src/main.rs` (register), `.gitignore`, `package.json` (test list), `tests/verify-portal.test.js`.

---

### Task A1: Fragment parser + vectors

**Files:**
- Create: `verify-site/js/verify-lib.js`
- Test: `tests/verify-portal.test.js`

**Interfaces:**
- Consumes: fragment format from spec §4 / `verify.rs:170-183`.
- Produces: `parseFragment(frag: string) -> {kind: 'signed'|'legacy'|'empty'|'malformed', v?, kid?, residentId?, valid?, status?, hashB64?, sigB64?, reason?}`; `b64urlToBytes(s: string) -> Uint8Array` (throws on bad chars); `bytesToHex(b: Uint8Array) -> string` (lowercase); `buildSignMessage(f) -> Uint8Array` (UTF-8 of `v{kid}|{rid}|{valid}|{status}|{hex}`, where hex = `bytesToHex(b64urlToBytes(f.hashB64))`); `isLegacyPayload(s: string) -> boolean` (trimmed starts with `{`).

- [ ] **Step 1: Write the failing test** — `tests/verify-portal.test.js` importing `../../verify-site/js/verify-lib.js`: valid 7-part vector parses to `kind: 'signed'` with exact fields; dotted-status vector (`...ACTIVE. REISSUED...`, 8+ dot-parts) parses with `status === 'ACTIVE. REISSUED'` (parse rule: `v=parts[0]`, `kid=parts[1]`, `sig=parts[last]`, `hash=parts[last-1]`, `residentId=middle[0]`, `valid=middle[1]`, `status=middle[2:].join('.')`); `'{}...'` → legacy; `''` → empty; `'a.b.c'` → malformed with reason.
- [ ] **Step 2: Run test to verify it fails** — Run: `node --test tests/verify-portal.test.js`. Expected: FAIL (module missing).
- [ ] **Step 3: Implement the five functions in `verify-site/js/verify-lib.js`** — pure, no DOM, ES-module exports, ASCII-only source.
- [ ] **Step 4: Run test to verify it passes** — Run: `node --test tests/verify-portal.test.js`. Expected: PASS. Then full suite: `npm test`. Expected: all green.

### Task A2: Vendored Ed25519 + RFC 8032 pin

**Files:**
- Create: `verify-site/crypto/nacl.min.js` (TweetNaCl 1.0.3, public domain, unmodified)
- Test: `tests/verify-portal.test.js` (append)

**Interfaces:**
- Consumes: `nacl.sign.detached.verify(msg: Uint8Array, sig: Uint8Array, pubkey: Uint8Array) -> boolean`.
- Produces: verified vendored primitive for Task A4.

- [ ] **Step 1: Write the failing test** — RFC 8032 §A.1 TEST 1 vectors (pub `d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a`, empty msg, sig `e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155 5f46b25...` full 64 bytes): `nacl.sign.detached.verify` true; flipped-bit sig false. Import from `../../verify-site/crypto/nacl.min.js`.
- [ ] **Step 2: Run test to verify it fails** — Expected: FAIL (file missing).
- [ ] **Step 3: Vendor the file** — download TweetNaCl 1.0.3 `nacl.min.js` unmodified; if offline, stop and report (owner supplies the file — do not hand-roll Ed25519).
- [ ] **Step 4: Run test to verify it passes** — `node --test tests/verify-portal.test.js` then `npm test`. Expected: green.

### Task A3: trust.json fetch + shape gate (fixture-driven)

**Files:**
- Create: `verify-site/trust.fixture.json` (committed sample: `active_kid: "abcd1234"`, two kids), plus `verify-site/trust.json` handling in `app.js` (Task A4)
- Test: `tests/verify-portal.test.js` (append)

**Interfaces:**
- Consumes: `VerifyTrust` shape (`config.rs:184-189`).
- Produces: `checkTrustShape(t: unknown) -> {ok: true, activeKid} | {ok: false, reason}` in `verify-lib.js`; `verifySignature(f, trust, nacl) -> {ok, reason?}` in `verify-lib.js` (unknown kid → `{ok:false, reason:'unknown-key'}`; bad pubkey hex → `{ok:false}` — never throws).

- [ ] **Step 1: Write the failing test** — fixture validates ok; `{}` → not-ok; unknown-kid signed fields → `verifySignature` false with reason `unknown-key`; `checkTrustShape` on `null`/array → not-ok.
- [ ] **Step 2: Run to verify it fails** — Expected: FAIL (functions missing).
- [ ] **Step 3: Implement both functions in `verify-lib.js`** — `verifySignature` takes `nacl` as a parameter (keeps lib DOM- and vendor-free for node tests).
- [ ] **Step 4: Run to verify it passes** — single file then `npm test`. Expected: green.

### Task A4: Portal UI (markup, style, wiring, hardening)

**Files:**
- Create: `verify-site/index.html`, `verify-site/styles.css`, `verify-site/app.js`, `verify-site/_headers`, `verify-site/README.md`
- Test: `tests/verify-portal.test.js` (string pins)

**Interfaces:**
- Consumes: `verify-lib.js` exports (Tasks A1/A3), `trust.json` (same-origin fetch).
- Produces: deployable folder; states landing/verifying/LEGITIMATE/INVALID/LEGACY; typed-ID fallback → LEGACY panel (never LEGITIMATE without signature).

- [ ] **Step 1: Write the failing test** — string pins on the new files: `index.html` has `#verdict` mount + `#fallback-form` + `<meta http-equiv="Content-Security-Policy" content="default-src 'self'">` + module script to `./app.js` only; `styles.css` has seal keyframes + `@media (prefers-reduced-motion: reduce)` static fallback; `app.js` imports `./js/verify-lib.js`, fetches `./trust.json` (relative, same-origin), auto-runs on load when fragment present, renders verdict text (not color-only); `_headers` has HSTS + `default-src 'self'`; `README.md` documents publish→deploy→receipt-compare + manual matrix (fresh scan, photocopy, tampered fragment, airplane mode).
- [ ] **Step 2: Run to verify it fails** — Expected: FAIL (files missing).
- [ ] **Step 3: Implement the five files** — elegant/neat, navy/gold LEGITIMATE seal (layered gradients, shine sweep, spring-in pop), slate/red INVALID, text verdict + reason always; reduced-motion → static seal + instant verdict; zero external refs (grep for `http` in site files must show only the README's Netlify docs, no code URLs).
- [ ] **Step 4: Run to verify it passes** — `npm test` green; manual browser check (`npx serve verify-site`, paste a Task A1 vector as `#fragment`): LEGITIMATE pops for a self-signed fixture (mint via node with fixture key — document the one-liner in README).

### Task B1: Card QR → signed URL (single + bulk)

**Files:**
- Modify: `src-tauri/src/id_card.rs` (~lines 59-75, 107-125, ~127-167, ~220-245)
- Test: `cargo test` (existing `id_card` tests)

**Interfaces:**
- Consumes: `crate::verify::verify_url_for(&resident) -> Result<String, String>`.
- Produces: card QR bytes = signed URL; unset base URL → propagated actionable error (no blank QR).

- [ ] **Step 1: Write the failing test** — replace `qr_payload_starts_with_resident_id_and_decodes` with `qr_payload_is_signed_verify_url`: payload starts with configured test base + `#1.` + splits to 7 dot-parts; keep `QrCode::new(payload).is_ok()`; add length bound `payload.len() < 300`. Check how tests set `verify_base_url` (follow the existing `civic.json`/env pattern in `config.rs` tests near lines 414/595).
- [ ] **Step 2: Run to verify it fails** — Run: `cargo test -p <crate> id_card` (crate root is `src-tauri/`). Expected: FAIL (still JSON payload).
- [ ] **Step 3: Implement** — `get_qr_data_url` calls `verify_url_for(&resident)?` then `qr_png_base64(&url)`; same in the bulk path (~line 167); delete now-unused `qr_data()`; update its doc comments (drop "offline verification"/"previously printed cards still verify" claims).
- [ ] **Step 4: Run to verify it passes** — `cargo test`. Expected: green.

### Task B2: verify_base_url Settings UI + print gating

**Files:**
- Modify: `src/index.html` (settings form), `src/js/app.js` (~lines 109, 201-248), `src/js/api.js` (only if a new invoke is needed — prefer existing `set_civic_config`)
- Test: `tests/verify-portal.test.js` (append)

**Interfaces:**
- Consumes: existing `API.getCivicConfig()` / `API.setCivicConfig({partial})`; server validation (`https://` + no spaces) already in `config.rs`.
- Produces: operator-editable portal URL field, persisted via `set_civic_config({verify_base_url})`; print path surfaces backend error as toast (no code change needed if invoke errors already toast — verify and pin).

- [ ] **Step 1: Write the failing test** — pins: settings form contains a `verify_base_url` input; `app.js` settings load/save round-trips `verify_base_url`; print/QR error path toasts the backend message (pin the existing toast call used by the card-print flow).
- [ ] **Step 2: Run to verify it fails** — Expected: FAIL.
- [ ] **Step 3: Implement** — add field beside other civic fields in the settings form; include `verify_base_url` in load/save partials; empty value allowed (means unverified-print blocked with backend error).
- [ ] **Step 4: Run to verify it passes** — `npm test` green.

### Task B3: publish_verify_trust command + receipt

**Files:**
- Modify: `src-tauri/src/verify.rs` or `config.rs` (new command), `src-tauri/src/main.rs` (~line 85 block), `.gitignore`
- Test: `cargo test`

**Interfaces:**
- Consumes: `config::load_verify_trust() -> VerifyTrust`.
- Produces: `#[tauri::command] pub async fn publish_verify_trust(dest: Option<String>) -> Result<PublishReceipt, String>` where `PublishReceipt { path: String, active_kid: Option<String>, key_count: usize, files: Vec<{name: String, sha256: String}> }`; default dest = `<project-root>/verify-site/trust.json`; receipt hashes every deployed file (`index.html`, `styles.css`, `app.js`, `js/verify-lib.js`, `crypto/nacl.min.js`, `trust.json`) so the operator can compare post-deploy.

- [ ] **Step 1: Write the failing test** — call with `dest` pointed at a tmp dir: receipt lists 6 files with 64-hex hashes matching independently computed SHA-256; `trust.json` content deserializes to `VerifyTrust` with `keys` non-empty.
- [ ] **Step 2: Run to verify it fails** — Expected: FAIL (no such command).
- [ ] **Step 3: Implement + register in `generate_handler!` + append `.gitignore` line `verify-site/trust.json`** (private key already ignored; never write it).
- [ ] **Step 4: Run to verify it passes** — `cargo test` green.

### Task C1: Cross-language golden vector (ties A↔B)

**Files:**
- Modify: `src-tauri/src/verify.rs` (tests module), `tests/verify-portal.test.js` (append)

**Interfaces:**
- Consumes: fixed seed `[0x42; 32]`, fixed fields (`resident_id "BRGY-2026-00001"`, `valid "01-01-2027"`, `status "ACTIVE"`, fixed 32-byte hash of a fixed canonical string).
- Produces: identical fragment verified by both stacks.

- [ ] **Step 1: Write the failing tests** — Rust: mint sig with the fixed seed, assert fragment equals the hardcoded golden string (compute actual in Step 3, paste back — the test pins it thereafter); JS: same golden fragment verifies true via vendored nacl + `checkTrustShape` fixture holding the fixed pubkey.
- [ ] **Step 2: Run to verify they fail** — both suites. Expected: FAIL.
- [ ] **Step 3: Implement + paste the golden string into both tests** — re-run both. Expected: green. (Non-ASCII status variant included: `status "ACTIVO — Ñ"` second vector, proving UTF-8 byte parity.)
- [ ] **Step 4: Full verification** — `npm test` + `cargo test` green.

### Task C2: Runbook, legacy docs, rollout checklist

**Files:**
- Modify: `verify-site/README.md` (extend), `Documents/QR_PII_README.md` (legacy-payload note)

**Interfaces:**
- Consumes: everything above.
- Produces: operator can roll out without asking questions.

- [ ] **Step 1: Write the failing test** — pins: README contains publish→deploy→receipt-compare steps, rotation procedure, unknown-kid/tampered-fragment troubleshooting, manual matrix checklist; QR_PII_README notes the legacy 7-key payload is retired from new cards (kept as LEGACY fallback).
- [ ] **Step 2: Run to verify it fails** — Expected: FAIL.
- [ ] **Step 3: Implement the docs.** — plus flip spec Status (`2026-10-10-verify-portal-design.md:3`) to `approved — implementing`.
- [ ] **Step 4: Run to verify it passes** — `npm test` green, final full `cargo test`.
