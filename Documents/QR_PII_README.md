# QR Code PII Notice (F16)

## What the QR code contains

Every card in the field carries a QR code produced by `qr_data()` in
`src-tauri/src/id_card.rs`. It encodes a **compact JSON**
object for **offline verification**:

```json
{
  "id": "<resident_id, e.g. BRGY-2026-00001>",
  "name": "<LAST FIRST, MIDDLE>",
  "dob": "<DD-MM-YYYY>",
  "purok": "<PUROK>",
  "contact": "<contact number if on file>",
  "valid": "<DD-MM-YYYY validity>",
  "status": "<remarks or ACTIVE>"
}
```

No photo, address-house, email, or government ID numbers (PhilHealth/SSS/TIN/Voter's)
are encoded. Generation is local-only (`qrcode` crate); no data leaves the machine.

The payload is **unsigned by design** (no HMAC/signature; `qr_hmac_secret`
in `config/civic.json` is null). Authenticity is manual BRGY-ID lookup in
the app — a scanned QR is a convenience copy of the printed data, not proof.

## Legacy status (signed-URL experiment removed)

A signed verification-URL QR + static portal experiment shipped briefly
and was fully removed: the QR once again encodes the 7-key JSON above on
every print. Cards printed during the experiment window carry a URL
payload the current code never mints — treat any non-JSON QR as
unverifiable by scan and fall back to manual BRGY-ID lookup in the app.
Everything below applies to all cards in the field.

## Privacy handling

- Treat printed/relayed QR payloads as **personally identifying**: name + birth date +
  contact + purok is enough to identify a resident.
- Do **not** photograph, repost, or transmit QR images over unsecured channels.
- Verification should be visual/offline (scan → compare against presented card).
- Bulk print runs render one QR per resident — store printed stacks
  with the same care as the resident database (`data/residents.db`).
- To minimize exposure, share a printed card only with the card holder.

## Operator controls

- QR size: 200px preview/print data-URL (`get_qr_data_url` in `src-tauri/src/id_card.rs`).
- QR failure never panics: a transparent 1×1 placeholder is rendered instead.
- To change what new card QRs encode, edit `qr_data()` in
  `src-tauri/src/id_card.rs`; the 7-key payload above is frozen —
  this document must be updated in the same change.
