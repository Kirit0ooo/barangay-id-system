# QR Code PII Notice (F16)

## What the QR code contains

Every printed Barangay Resident ID card embeds a QR code
produced by `qr_data()` in `src-tauri/src/id_card.rs`. It encodes a **compact JSON**
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
- To change encoded fields, edit `qr_data()` and rebuild; this document must be
  updated in the same change.
