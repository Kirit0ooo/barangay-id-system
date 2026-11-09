//! ID-card helpers for the HTML-only print flow.
//!
//! History: this module previously rendered PDFs (front/back draw fns,
//! single + bulk generators, per-file progress events, failure manifests,
//! per-resident filenames, audit-trail writes). PDF generation was fully
//! removed: printing is `card-template.html` filled by
//! `src/js/app.js:buildCardPayload` and printed on CR100 98.6x66.8mm via the
//! browser (Save-as-PDF via the print dialog when a file is needed).
//!
//! What remains — the two things the template cannot do itself:
//! - `fmt_ddmmyyyy` — storage-ISO → display date (also used by export.rs).
//! - `get_qr_data_url` — legacy QR PNG data-URL (unsigned 7-key JSON:
//!   id/name/dob/purok/contact/valid/status), fetched by the frontend per
//!   card. Authenticity is by manual BRGY-ID lookup in the app.

use crate::db::AppState;
use crate::residents::Resident;
use base64::Engine as _;
use qrcode::QrCode;
use std::io::Cursor;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

/// Display helper: storage-ISO YYYY-MM-DD (or legacy DD-MM-YYYY / DD/MM/YYYY) → DD-MM-YYYY.
/// Shared with export.rs (Excel shows display format, never raw storage).
pub(crate) fn fmt_ddmmyyyy(s: &str) -> String {
    // Input: YYYY-MM-DD or DD-MM-YYYY or DD/MM/YYYY -> output DD-MM-YYYY
    if s.contains('-') {
        let p: Vec<&str> = s.split('-').collect();
        if p.len() == 3 {
            if p[0].len() == 4 {
                return format!("{}-{}-{}", p[2], p[1], p[0]);
            } // YYYY-MM-DD -> DD-MM-YYYY
            return s.to_string();
        }
    }
    if s.contains('/') {
        return s.replace('/', "-");
    }
    s.to_string()
}

/// L1 single source of truth for validity display: storage-ISO → DD-MM-YYYY,
/// empty → today+365. residents.rs backfills valid_until on create/update and
/// db.rs migrates legacy rows, so persisted rows always carry a value and the
/// frontend must mirror this rule (never invent its own +365 variant).
pub(crate) fn valid_until_display(resident: &Resident) -> String {
    // P1-6: storage is ISO; every display edge converts via fmt_ddmmyyyy.
    match resident.valid_until.as_deref() {
        Some(v) if !v.trim().is_empty() => fmt_ddmmyyyy(v.trim()),
        _ => {
            // Fixed 1 year validity - auto, no UI per requirement
            let vu = chrono::Local::now().date_naive() + chrono::Duration::days(365);
            vu.format("%d-%m-%Y").to_string()
        }
    }
}

/// Preview QR at 200px (screen). Encodes the legacy 7-key resident JSON;
/// a 1x1 placeholder is returned on failure (never panics).
fn qr_png_base64(data: &str) -> String {
    // Generate QR as PNG base64 data URL.
    // Fallback to empty placeholder on failure rather than panic
    let code = match QrCode::new(data.as_bytes()) {
        Ok(c) => c,
        Err(_) => {
            // Return a 1x1 transparent PNG placeholder
            return "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=".to_string();
        }
    };
    let img = code
        .render::<::image::Luma<u8>>()
        .max_dimensions(200, 200)
        .build();
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);
    if ::image::DynamicImage::ImageLuma8(img)
        .write_to(&mut cursor, ::image::ImageFormat::Png)
        .is_err()
    {
        return "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=".to_string();
    }
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&buf)
    )
}

/// Legacy card-QR payload: unsigned 7-key JSON
/// {id, name ("LAST FIRST,MIDDLE"), dob (DD-MM-YYYY), purok, contact,
/// valid (DD-MM-YYYY), status}. Scannable offline; authenticity is by
/// manual BRGY-ID lookup in the app (no signature).
fn qr_data(r: &Resident) -> String {
    let middle = r.middle_name.as_deref().unwrap_or("");
    let name = format!("{} {},{}", r.last_name, r.first_name, middle);
    let doc = serde_json::json!({
        "id": r.resident_id,
        "name": name,
        "dob": fmt_ddmmyyyy(&r.birth_date),
        "purok": r.address_purok.as_deref().unwrap_or(""),
        "contact": r.contact_number.as_deref().unwrap_or(""),
        "valid": valid_until_display(r),
        "status": r.remarks.as_deref().unwrap_or("ACTIVE"),
    });
    doc.to_string()
}

/// Dedicated QR data-URL command — the card template path calls this instead
/// of regex-scraping `<img>` out of preview HTML. Encodes the legacy
/// unsigned JSON payload (`qr_data`).
#[tauri::command]
pub async fn get_qr_data_url(
    state: State<'_, Arc<Mutex<AppState>>>,
    resident_id: i64,
) -> Result<String, String> {
    let pool = { state.lock().await.pool.clone() };
    let resident = sqlx::query_as::<_, Resident>(&format!(
        "SELECT {} FROM residents WHERE id = ?",
        crate::residents::RESIDENT_COLUMNS
    ))
    .bind(resident_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| format!("Resident not found: {}", e))?;
    // C4: never mint a QR from a partial record; inactive rows are not printable.
    if resident.is_active.unwrap_or(1) != 1 {
        return Err("Resident is inactive".into());
    }
    crate::residents::validate_resident_complete(&resident)?;
    let payload = qr_data(&resident);
    Ok(qr_png_base64(&payload))
}

/// P1-4: batch per-card binaries for bulk print — one IPC call returns photo
/// + QR for up to 200 residents instead of 2N invokes. Frontend processes photos
/// (resize, etc.) and uses QR codes directly.
#[tauri::command]
pub async fn get_cards_batch(
    state: State<'_, Arc<Mutex<AppState>>>,
    ids: Vec<i64>,
) -> Result<Vec<Option<(String, String)>>, String> {
    const MAX_BATCH: usize = 200;
    if ids.len() > MAX_BATCH {
        return Err(format!(
            "Batch too large ({} > {}) — chunk client-side",
            ids.len(),
            MAX_BATCH
        ));
    }
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };
    // C4/M3: single WHERE IN (...) round-trip (no N+1 loop). Inactive or
    // incomplete rows yield None entries so they are never printable.
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT {} FROM residents WHERE id IN ({}) AND (is_active = 1 OR is_active IS NULL)",
        crate::residents::RESIDENT_COLUMNS,
        placeholders
    );
    let mut q = sqlx::query_as::<_, Resident>(&sql);
    for id in &ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(&pool).await.map_err(|e| e.to_string())?;
    let mut by_id = std::collections::HashMap::with_capacity(rows.len());
    for r in rows {
        by_id.insert(r.id.unwrap_or(-1), r);
    }
    let mut out = Vec::with_capacity(ids.len());
    for resident_id in ids {
        let resident: Option<&Resident> = by_id.get(&resident_id);
        // Active-only + completeness gate: inactive/incomplete → None entry.
        let eligible = resident
            .filter(|r| r.is_active.unwrap_or(1) == 1)
            .filter(|r| crate::residents::validate_resident_complete(r).is_ok());
        let photo = eligible
            .and_then(|r| r.photo_filename.as_deref())
            .and_then(|f| crate::residents::get_photo_base64(f, &app_data_dir));
        let qr = eligible.map(|r| qr_png_base64(&qr_data(r)));
        out.push(photo.zip(qr)); // zip combines two Options into Option<(T, U)>
    }
    Ok(out)
}

#[cfg(test)]
mod qr_tests {
    use super::*;
    use crate::residents::Resident;

    fn francis_like() -> Resident {
        Resident {
            id: Some(1),
            resident_id: "BRGY-2026-00001".to_string(),
            last_name: "PATINDOL".to_string(),
            first_name: "FRANCIS".to_string(),
            middle_name: Some("ADONA".to_string()),
            suffix: None,
            birth_date: "1990-01-15".to_string(),
            birth_place: None,
            gender: "Male".to_string(),
            civil_status: Some("Single".to_string()),
            citizenship: Some("Filipino".to_string()),
            occupation: None,
            contact_number: Some("09171234567".to_string()),
            address_street: None,
            address_purok: Some("AVOCADO".to_string()),
            address_barangay: Some("Barangay (Nouvelas Occidental) Balud".to_string()),
            address_city: Some("BASEY".to_string()),
            address_province: Some("SAMAR".to_string()),
            religion: None,
            blood_type: None,
            height: None,
            weight: None,
            philhealth: None,
            sss: None,
            tin: None,
            voters_id: None,
            emergency_contact_name: None,
            emergency_contact_relation: None,
            emergency_contact_number: None,
            emergency_contact_resident_id: None,
            photo_filename: None,
            is_active: Some(1),
            date_registered: Some("2026-01-01 00:00:00".to_string()),
            date_updated: None,
            remarks: Some("ACTIVE".to_string()),
            valid_until: None,
            last_pdf_path: None,
            last_pdf_generated_at: None,
            emergency_contact_address: None,
        }
    }

    #[test]
    fn qr_payload_is_legacy_unsigned_json() {
        // Full removal: the card QR is the unsigned 7-key JSON again —
        // id/name/dob/purok/contact/valid/status. No signature, no URL.
        let r = francis_like();
        let payload = qr_data(&r);
        let v: serde_json::Value =
            serde_json::from_str(&payload).expect("qr_data emits valid JSON");
        assert_eq!(v["id"], serde_json::json!("BRGY-2026-00001"));
        assert_eq!(v["name"], serde_json::json!("PATINDOL FRANCIS,ADONA"));
        assert_eq!(v["dob"], serde_json::json!("15-01-1990"));
        assert_eq!(v["purok"], serde_json::json!("AVOCADO"));
        assert_eq!(v["contact"], serde_json::json!("09171234567"));
        assert_eq!(v["status"], serde_json::json!("ACTIVE"));
        assert!(v["valid"].as_str().is_some());
        assert_eq!(v.as_object().unwrap().len(), 7);
        // Still a scannable QR matrix, well under the length budget.
        assert!(QrCode::new(payload.as_bytes()).is_ok());
        assert!(payload.len() < 300);
        // Data-URL renderer never panics, even on hostile input.
        let data_url = qr_png_base64(&payload);
        assert!(data_url.starts_with("data:image/png;base64,"));
        assert!(qr_png_base64("").starts_with("data:image/png;base64,"));
    }

    #[test]
    fn fmt_ddmmyyyy_converts_iso_and_passes_legacy_through() {
        assert_eq!(fmt_ddmmyyyy("2027-02-15"), "15-02-2027");
        assert_eq!(fmt_ddmmyyyy("20-11-2026"), "20-11-2026");
        assert_eq!(fmt_ddmmyyyy("20/11/2026"), "20-11-2026");
    }
}
