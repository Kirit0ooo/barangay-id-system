# Verify Portal — Design Spec (A: Replace)

Date: 2026-10-10 | Branch: `feat/p0-fixes` | Status: **approved - implementing**
Related code: `src-tauri/src/verify.rs` (signed URL-QR + registry, backend wired),
`src-tauri/src/config.rs` (keypair/trust store), `src-tauri/src/id_card.rs` (current unsigned QR)

## 1. Goal
A phone camera scan of the physical ID card's QR opens a hosted static page that
**automatically pops a verdict: LEGITIMATE or INVALID** — no app install, no typing.
Verdict-only: the page proves authenticity via Ed25519, never publishes resident PII.

## 2. Decisions (owner-approved)
- **A-Replace:** the card QR becomes the signed verification URL only. The old
  unsigned 7-key JSON payload is dropped from new cards (no dual QR, no clutter).
- **Fallback:** the portal accepts (a) old unsigned QRs → verdict `LEGACY — look up
  in office records`, and (b) a manually typed resident ID → same `LEGACY` path.
  Both fail closed (never LEGITIMATE).
- **Verdict-only:** no `registry.json` publishing in phase 1. No PII leaves the office.
- **Hosting:** static files deployable to Netlify (or any static host) from a new
  modular `verify-site/` folder in this repo.

## 3. Architecture (`verify-site/`, new folder)
```
verify-site/
  index.html        # markup: scanner landing, verdict stage, 3D seal mount, fallback form
  styles.css        # elegant/neat theme, seal 3D + shine keyframes, reduced-motion
  app.js            # fragment parser, SHA-256, verify orchestration, UI state machine
  crypto/ed25519.js # vendored Ed25519 verify-only implementation (no CDN, offline-safe)
  trust.json        # GENERATED at publish time (active_kid + keys), gitignored, deployed
  README.md         # operator runbook: publish trust, deploy, rotate, verify receipt
```

## 4. Data flow (scan → auto-popup)
1. Card QR encodes `{verify_base_url}#{v}.{kid}.{resident_id}.{valid}.{status}.{hash_b64}.{sig_b64}`
   (`v`=schema 1; `hash`=SHA-256 of canonical 39-col record; `sig`=Ed25519 over the
   dot-joined message — see `verify.rs::sign_message`).
2. Page loads → parses fragment → looks up `kid` in `trust.json` → recomputes the
   signed message → `ed25519_verify(pubkey, msg, sig)`.
3. Valid → auto-popup 3D shining seal + `LEGITIMATE` + resident ID + validity window.
   Anything else (bad sig, unknown kid, malformed fragment, legacy payload) →
   `INVALID`/`LEGACY` panel with reason. No network calls except fetching `trust.json`
   (same origin).

## 5. Key management (robustness core)
- Private seed lives ONLY in `config/id_verify_private` on the office machine.
  It is **never** copied into `verify-site/`, never committed, never deployed.
- Rotation (`rotate_verify_key`) appends new kid, keeps old kids → old cards keep
  verifying. `trust.json` (public keys only) is regenerated + redeployed per rotation.
- **Deployment receipt (tampered-page defense):** publish step prints SHA-256 of each
  site file; operator compares post-deploy (Netlify serves bytes as-is). Mismatch →
  redeploy, never link the QR base URL to a tampered page.
- Transport: Netlify HTTPS enforced (HSTS); strict CSP (`default-src 'self'`,
  no scripts beyond local files, no frames); zero fonts/trackers/analytics.

## 6. Verdict presentation
Elegant, neat page; status is a 3D CSS seal (layered conic/radial gradients,
rotating shine sweep, spring-in pop) in navy/gold for LEGITIMATE, slate/red for
INVALID. `prefers-reduced-motion` → static seal, instant verdict, no sweep.
Verdict text + reason always rendered as text (not color-only) for accessibility.

## 7. Backend changes required (implementation plan scope)
- `id_card.rs`: card QR payload → `verify::verify_url_for()` (fail-closed when
  `verify_base_url` unset: block print with actionable error, not a blank QR).
- `config.rs`/Settings UI: expose `verify_base_url` (currently no UI); add
  `publish_verify_trust` command writing `verify-site/trust.json` + receipt hashes.
- `export_verify_registry` stays but is NOT part of phase-1 flow (verdict-only).

## 8. Tests
- JS parser unit tests (fragment vectors incl. malformed/legacy/unknown-kid).
- `trust.json` shape pin (regression suite).
- Rust e2e: mint QR for seeded resident → feed fragment through the same checks the
  page performs (mirrors `verify.rs` roundtrip tests).
- Manual matrix: fresh card scan, photocopy scan (still LEGITIMATE — bearer limit,
  documented), tampered fragment, offline airplane-mode load.

## 9. Known limits (NOT solved here)
- Photocopies scan as legitimate (bearer property) — physical card security features
  remain the anti-copy layer.
- No revocation list in phase 1 (lost/stolen cards verify until re-issued under a
  new kid set) → phase 2: `revoked_ids` in `trust.json`.

## 10. Rollout
1. Set `verify_base_url` → publish `trust.json` → deploy `verify-site/` →
   compare receipt hashes. 2. Print one test card, scan with two phones.
3. Reprint circulation (old cards → `LEGACY` path until replaced).
