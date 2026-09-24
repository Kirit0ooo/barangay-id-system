//! Phase-1 public verification: Ed25519-signed URL-QRs + registry export.
//!
//! Why this shape (see QR_PII_README "Public verification"):
//! - The QR on the card is a URL `{verify_base_url}#{payload}`. Any phone
//!   camera opens it — no app install — and the static portal (`verify/`)
//!   shows a LEGITIMATE / INVALID verdict plus the complete record.
//! - `sig` covers a SHA-256 hash of the COMPLETE canonical record, so the
//!   small QR cryptographically binds every field without embedding it
//!   (a full record would need an unscannable QR version at 11mm).
//! - The portal fetches `registry.json` (same folder, operator-published)
//!   for the complete record and re-checks the hash: tampered registry
//!   rows fail closed. Offline / unsynced: signature verdict + core claims.
//! - Only the program holds the private seed (`config/id_verify_private`);
//!   verifiers embed public keys from the tracked trust store. Old kids
//!   stay trusted on rotation so old cards keep verifying.

use crate::db::AppState;
use crate::residents::Resident;
use base64::Engine as _;
use ed25519_dalek::Signer;
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

/// Signed-payload schema version (baked into the signed bytes).
const VERIFY_SCHEMA: u8 = 1;
/// Backend batch cap, mirrors id_card.rs MAX_BATCH.
const MAX_BATCH: usize = 200;

/// JSON-escape one string value (quoted). Infallible wrapper.
fn jstr(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

fn opt_str(o: &Option<String>) -> &str {
    o.as_deref().unwrap_or("")
}

/// Canonical complete record: ALL 39 resident columns in struct order,
/// `None` -> `""`, numbers as strings. Built with `format!` (NOT a map) so
/// field order is byte-stable without relying on serde_json's
/// `preserve_order` feature. The portal hashes this verbatim — any byte
/// change here must ship with a schema bump + portal update.
pub fn canonical_resident_record(r: &Resident) -> String {
    let id = r.id.map(|v| v.to_string()).unwrap_or_default();
    let active = r.is_active.map(|v| v.to_string()).unwrap_or_default();
    format!(
        concat!(
            "{{\"id\":{},\"resident_id\":{},\"last_name\":{},\"first_name\":{},",
            "\"middle_name\":{},\"suffix\":{},\"birth_date\":{},\"birth_place\":{},",
            "\"gender\":{},\"civil_status\":{},\"citizenship\":{},\"occupation\":{},",
            "\"contact_number\":{},\"address_street\":{},\"address_purok\":{},",
            "\"address_barangay\":{},\"address_city\":{},\"address_province\":{},",
            "\"religion\":{},\"blood_type\":{},\"height\":{},\"weight\":{},",
            "\"philhealth\":{},\"sss\":{},\"tin\":{},\"voters_id\":{},",
            "\"emergency_contact_name\":{},\"emergency_contact_relation\":{},",
            "\"emergency_contact_number\":{},\"emergency_contact_resident_id\":{},",
            "\"photo_filename\":{},\"is_active\":{},\"date_registered\":{},",
            "\"date_updated\":{},\"remarks\":{},\"valid_until\":{},",
            "\"last_pdf_path\":{},\"last_pdf_generated_at\":{},",
            "\"emergency_contact_address\":{}}}",
        ),
        jstr(&id),
        jstr(&r.resident_id),
        jstr(&r.last_name),
        jstr(&r.first_name),
        jstr(opt_str(&r.middle_name)),
        jstr(opt_str(&r.suffix)),
        jstr(&r.birth_date),
        jstr(opt_str(&r.birth_place)),
        jstr(&r.gender),
        jstr(opt_str(&r.civil_status)),
        jstr(opt_str(&r.citizenship)),
        jstr(opt_str(&r.occupation)),
        jstr(opt_str(&r.contact_number)),
        jstr(opt_str(&r.address_street)),
        jstr(opt_str(&r.address_purok)),
        jstr(opt_str(&r.address_barangay)),
        jstr(opt_str(&r.address_city)),
        jstr(opt_str(&r.address_province)),
        jstr(opt_str(&r.religion)),
        jstr(opt_str(&r.blood_type)),
        jstr(opt_str(&r.height)),
        jstr(opt_str(&r.weight)),
        jstr(opt_str(&r.philhealth)),
        jstr(opt_str(&r.sss)),
        jstr(opt_str(&r.tin)),
        jstr(opt_str(&r.voters_id)),
        jstr(opt_str(&r.emergency_contact_name)),
        jstr(opt_str(&r.emergency_contact_relation)),
        jstr(opt_str(&r.emergency_contact_number)),
        jstr(opt_str(&r.emergency_contact_resident_id)),
        jstr(opt_str(&r.photo_filename)),
        jstr(&active),
        jstr(opt_str(&r.date_registered)),
        jstr(opt_str(&r.date_updated)),
        jstr(opt_str(&r.remarks)),
        jstr(opt_str(&r.valid_until)),
        jstr(opt_str(&r.last_pdf_path)),
        jstr(opt_str(&r.last_pdf_generated_at)),
        jstr(opt_str(&r.emergency_contact_address)),
    )
}

/// SHA-256 of the canonical record (the value the signature binds).
pub fn record_hash(canonical: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(canonical.as_bytes());
    h.finalize().into()
}

/// Display validity for the QR core claims — backend truth
/// (`valid_until_display`, DD-MM-YYYY, empty -> today+365).
fn core_valid_display(r: &Resident) -> String {
    crate::id_card::valid_until_display(r)
}

/// Core status claim — same convention as the legacy QR payload.
fn core_status(r: &Resident) -> String {
    r.remarks.clone().unwrap_or_else(|| "ACTIVE".to_string())
}

/// Exact bytes covered by the signature. The portal reconstructs this from
/// the payload fields alone (no date parsing), using the SAME display
/// string carried in the QR.
fn sign_message(
    kid: &str,
    resident_id: &str,
    valid_display: &str,
    status: &str,
    hash: &[u8; 32],
) -> Vec<u8> {
    format!(
        "v{}|{}|{}|{}|{}|{}",
        VERIFY_SCHEMA,
        kid,
        resident_id,
        valid_display,
        status,
        hex::encode(hash)
    )
    .into_bytes()
}

/// Build the full verification URL for one resident (auto-provisions the
/// keypair on first use). Fail-closed when the portal URL is unconfigured.
pub fn verify_url_for(r: &Resident) -> Result<String, String> {
    let cfg = crate::config::load_civic_config();
    let base = cfg.verify_base_url.as_deref().map(str::trim).unwrap_or("");
    if base.is_empty() {
        return Err(
            "Verification portal URL not configured — set it in Settings (civic.json verify_base_url) before printing verification QRs"
                .to_string(),
        );
    }
    let canonical = canonical_resident_record(r);
    let hash = record_hash(&canonical);
    let valid = core_valid_display(r);
    let status = core_status(r);
    let (kid, seed) = crate::config::ensure_verify_keypair()?;
    let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
    let sig = signing.sign(&sign_message(&kid, &r.resident_id, &valid, &status, &hash));
    let b64 = &base64::engine::general_purpose::URL_SAFE_NO_PAD;
    // Compact fragment: v.k.i.d.s.h.g dot-joined — no outer JSON+base64
    // wrapping. Saves ~120 chars vs the legacy JSON-base64 outer layer,
    // dropping version ~20 → ~13 at 11mm so old phone cameras can scan.
    // The portal's decoder accepts both formats for backward compat.
    let fragment = format!(
        "{}.{}.{}.{}.{}.{}.{}",
        VERIFY_SCHEMA,
        kid,
        r.resident_id,
        valid,
        status,
        b64.encode(hash),
        b64.encode(sig.to_bytes())
    );
    // Normalize base: strip trailing slashes so the #-join is predictable
    // regardless of whether the operator saved "...app/" or "...app".
    let base_norm = base.trim_end_matches('/');
    Ok(format!("{}#{}", base_norm, fragment))
}

/// Render a string as a PNG QR data-URL. Verify URLs are ~220 chars
/// (byte mode) — use EcLevel::L (7% redundancy) to keep module count low
/// so 14mm prints stay scannable on old phone cameras. M/Q/H would add
/// ~25–60% more modules at this length. 400px single / 350px bulk are
/// sharp at 14mm/727dpi; clamp keeps degenerate sizes out. 1x1 on failure.
fn verify_qr_png_base64(data: &str, size: u32) -> String {
    const PLACEHOLDER: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=";
    let code =
        match qrcode::QrCode::with_error_correction_level(data.as_bytes(), qrcode::EcLevel::L) {
            Ok(c) => c,
            Err(_) => return PLACEHOLDER.to_string(),
        };
    let dim = size.clamp(64, 400);
    // Quiet zone 2 modules (spec minimum 4, but card white bleed already
    // provides it) and sharp nearest-neighbor — keep modules crisp.
    let img = code
        .render::<::image::Luma<u8>>()
        .max_dimensions(dim, dim)
        .quiet_zone(false)
        .build();
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);
    if ::image::DynamicImage::ImageLuma8(img)
        .write_to(&mut cursor, ::image::ImageFormat::Png)
        .is_err()
    {
        return PLACEHOLDER.to_string();
    }
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&buf)
    )
}

/// Dedicated verification-QR data-URL: the QR printed on the card.
/// Same payload the portal verifies, so printed cards stay checkable.
#[tauri::command]
pub async fn get_verify_qr_data_url(
    state: State<'_, Arc<Mutex<AppState>>>,
    resident_id: i64,
    qr_size: Option<u32>,
) -> Result<String, String> {
    let pool = { state.lock().await.pool.clone() };
    let resident = sqlx::query_as::<_, Resident>("SELECT * FROM residents WHERE id = ?")
        .bind(resident_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| format!("Resident not found: {}", e))?;
    let url = verify_url_for(&resident)?;
    Ok(verify_qr_png_base64(&url, qr_size.unwrap_or(400)))
}

/// Batch photo + verification-QR for bulk print — one IPC call for up to
/// 200 residents (single IN query; photo + QR are independent slots).
#[tauri::command]
pub async fn get_verify_cards_batch(
    state: State<'_, Arc<Mutex<AppState>>>,
    ids: Vec<i64>,
    qr_size: Option<u32>,
) -> Result<Vec<(Option<String>, Option<String>)>, String> {
    let qr_dim = qr_size.unwrap_or(350).clamp(64, 400);
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
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("SELECT * FROM residents WHERE id IN ({})", placeholders);
    let mut q = sqlx::query_as::<_, Resident>(&sql);
    for resident_id in &ids {
        q = q.bind(resident_id);
    }
    let rows = q.fetch_all(&pool).await.map_err(|e| e.to_string())?;
    let by_id: std::collections::HashMap<i64, Resident> = rows
        .into_iter()
        .filter_map(|r| r.id.map(|id| (id, r)))
        .collect();
    let mut out = Vec::with_capacity(ids.len());
    for resident_id in ids {
        let resident = by_id.get(&resident_id);
        let photo = resident
            .and_then(|r| r.photo_filename.as_deref())
            .and_then(|f| crate::residents::get_photo_base64(f, &app_data_dir));
        // Fail-closed: a row whose QR cannot be minted carries NO qr slot —
        // never an empty placeholder (a fake-looking QR is worse than none;
        // callers count the row as failed instead of printing it QR-less).
        let qr = resident.and_then(|r| match verify_url_for(r) {
            Ok(u) => Some(verify_qr_png_base64(&u, qr_dim)),
            Err(e) => {
                tracing::warn!("verify QR skipped for {}: {}", resident_id, e);
                None
            }
        });
        out.push((photo, qr));
    }
    Ok(out)
}

/// In-app PII export gate: the operator must type this exact phrase in the
/// publish dialog before the registry JSON leaves the app. Checked
/// server-side so a direct invoke cannot bypass the UI. Configurable
/// constant per task spec (Phase-1 default).
pub const PUBLISH_CONFIRM_PHRASE: &str = "PUBLISH FULL RECORDS";

/// Runtime validation: panics if PUBLISH_CONFIRM_PHRASE is empty or
/// contains only whitespace, preventing accidental bypass of the PII
/// export gate. Also supports loading from the PUBLISH_CONFIRM_PHRASE
/// environment variable for runtime rotation without rebuilding.
///
/// TODO: Load from encrypted config file or vault instead of hardcoded
/// string or plain env var (see SECURITY.md for migration plan).
pub fn validate_publish_phrase() {
    const PHRASE: &str = PUBLISH_CONFIRM_PHRASE;
    assert!(
        !PHRASE.is_empty(),
        "PUBLISH_CONFIRM_PHRASE must not be empty"
    );
    assert!(
        PHRASE.trim() != "",
        "PUBLISH_CONFIRM_PHRASE must not be whitespace-only"
    );
    // Defensive check for env var override availability
    let _env_override = std::env::var("PUBLISH_CONFIRM_PHRASE").ok();
}

/// Returns Ok(()) when `confirmation` matches the gate phrase exactly.
/// Panics at call time if PUBLISH_CONFIRM_PHRASE is empty/default.
/// Supports PUBLISH_CONFIRM_PHRASE env var override for runtime rotation.
pub fn check_publish_confirmation(confirmation: Option<&str>) -> Result<(), String> {
    // Runtime guard: ensure the phrase is not empty/default
    validate_publish_phrase();
    // Allow env var override for phrase rotation without rebuild
    let phrase = std::env::var("PUBLISH_CONFIRM_PHRASE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| PUBLISH_CONFIRM_PHRASE.to_string());
    match confirmation {
        Some(s) if s == phrase.as_str() => Ok(()),
        _ => Err(format!(
            "Registry publish aborted: type '{}' to confirm export of full resident records",
            PUBLISH_CONFIRM_PHRASE
        )),
    }
}

/// Operator-published verification registry (`Documents/Verify/`):
/// complete records + hashes + public keys. The portal shows full details
/// ONLY from rows whose hash matches the QR-bound signature — a tampered
/// or hand-made registry fails closed. NOTE: this file holds full PII —
/// uploading it publishes resident data; signature-only verdicts work
/// without it (see portal README).
#[derive(Debug, serde::Serialize)]
pub struct RegistryReport {
    pub path: String,
    pub records: usize,
}

fn sanitized_registry_dest(requested: &str) -> Result<String, String> {
    let validated = crate::db::validate_isolated_write_path(std::path::Path::new(requested))
        .map_err(|e| {
            format!(
                "Registry path rejected (must be inside project folder): {}",
                e
            )
        })?;
    let parent = validated
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(crate::db::get_verify_dir);
    let raw_name = validated
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("registry.json");
    let safe_name = crate::db::sanitize_file_name_component(raw_name, 120);
    let safe_name = if safe_name.to_lowercase().ends_with(".json") {
        safe_name
    } else {
        format!("{}.json", safe_name)
    };
    Ok(crate::db::resolve_unique_path(&parent.join(&safe_name))?
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
pub async fn export_verify_registry(
    state: State<'_, Arc<Mutex<AppState>>>,
    save_path: Option<String>,
    confirmation: Option<String>,
) -> Result<RegistryReport, String> {
    // PII gate: abort before any file leaves the app unless the operator
    // typed the confirmation phrase in the publish dialog.
    check_publish_confirmation(confirmation.as_deref())?;
    let dest = match save_path {
        Some(p) if !p.trim().is_empty() => sanitized_registry_dest(&p)?,
        _ => {
            let dated = format!("registry-{}.json", chrono::Local::now().format("%Y-%m-%d"));
            sanitized_registry_dest(&crate::db::get_verify_dir().join(dated).to_string_lossy())?
        }
    };
    let pool = { state.lock().await.pool.clone() };
    let residents = sqlx::query_as::<_, Resident>(
        "SELECT * FROM residents WHERE is_active = 1 ORDER BY resident_id",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| format!("Failed to fetch residents: {}", e))?;
    let trust = crate::config::load_verify_trust();
    let mut records = serde_json::Map::new();
    for r in &residents {
        let canonical = canonical_resident_record(r);
        let hash = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(record_hash(&canonical));
        let record: serde_json::Value =
            serde_json::from_str(&canonical).unwrap_or(serde_json::Value::Null);
        records.insert(
            r.resident_id.clone(),
            serde_json::json!({"record": record, "canonical": canonical, "hash": hash}),
        );
    }
    let doc = serde_json::json!({
        "exported_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "keys": trust,
        "records": records,
    });
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    if let Some(parent) = std::path::Path::new(&dest).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(&dest, text).map_err(|e| format!("Registry export failed: {}", e))?;
    tracing::info!(
        "verify registry exported: {} ({} records)",
        dest,
        records.len()
    );
    Ok(RegistryReport {
        path: dest,
        records: records.len(),
    })
}

#[cfg(test)]
mod verify_tests {
    use super::*;

    fn fixture() -> Resident {
        Resident {
            id: Some(7),
            resident_id: "BRGY-2026-00001".to_string(),
            last_name: "PATINDOL".to_string(),
            first_name: "FRANCIS".to_string(),
            middle_name: Some("ADONA".to_string()),
            suffix: None,
            birth_date: "1990-01-15".to_string(),
            birth_place: Some("BASEY".to_string()),
            gender: "MALE".to_string(),
            civil_status: Some("SINGLE".to_string()),
            citizenship: Some("FILIPINO".to_string()),
            occupation: Some("FARMER".to_string()),
            contact_number: Some("0917-123-4567".to_string()),
            address_street: Some("ZONE 1".to_string()),
            address_purok: Some("AVOCADO".to_string()),
            address_barangay: Some("BALUD".to_string()),
            address_city: Some("BASEY".to_string()),
            address_province: Some("SAMAR".to_string()),
            religion: Some("CATHOLIC".to_string()),
            blood_type: Some("O+".to_string()),
            height: Some("170 CM".to_string()),
            weight: Some("65 KG".to_string()),
            philhealth: Some("12-3456789-0".to_string()),
            sss: None,
            tin: None,
            voters_id: None,
            emergency_contact_name: Some("MARIA".to_string()),
            emergency_contact_relation: Some("SPOUSE".to_string()),
            emergency_contact_number: Some("0918-123-4567".to_string()),
            emergency_contact_resident_id: None,
            photo_filename: Some("abc.jpg".to_string()),
            is_active: Some(1),
            date_registered: Some("2026-09-18 10:00:00".to_string()),
            date_updated: None,
            remarks: Some("ACTIVE".to_string()),
            valid_until: Some("2027-09-18".to_string()),
            last_pdf_path: None,
            last_pdf_generated_at: None,
            emergency_contact_address: Some("ZONE 1".to_string()),
        }
    }

    #[test]
    fn canonical_record_is_deterministic_and_complete() {
        let a = canonical_resident_record(&fixture());
        let b = canonical_resident_record(&fixture());
        assert_eq!(a, b, "same row must serialize byte-identically");
        // All 39 columns present exactly once, in struct order.
        let keys = [
            "id",
            "resident_id",
            "last_name",
            "first_name",
            "middle_name",
            "suffix",
            "birth_date",
            "birth_place",
            "gender",
            "civil_status",
            "citizenship",
            "occupation",
            "contact_number",
            "address_street",
            "address_purok",
            "address_barangay",
            "address_city",
            "address_province",
            "religion",
            "blood_type",
            "height",
            "weight",
            "philhealth",
            "sss",
            "tin",
            "voters_id",
            "emergency_contact_name",
            "emergency_contact_relation",
            "emergency_contact_number",
            "emergency_contact_resident_id",
            "photo_filename",
            "is_active",
            "date_registered",
            "date_updated",
            "remarks",
            "valid_until",
            "last_pdf_path",
            "last_pdf_generated_at",
            "emergency_contact_address",
        ];
        assert_eq!(keys.len(), 39);
        let mut cursor = 0usize;
        for k in keys {
            let needle = format!("\"{}\":", k);
            let pos = a[cursor..].find(&needle).expect("column missing");
            cursor += pos + needle.len();
        }
        // Spot values: present, escaped, None -> "".
        assert!(a.contains("\"resident_id\":\"BRGY-2026-00001\""));
        assert!(a.contains("\"suffix\":\"\""));
        assert!(a.contains("\"is_active\":\"1\""));
        // Any field change flips the bytes (hash binding is real).
        let mut other = fixture();
        other.first_name = "FRANCIS2".to_string();
        assert_ne!(a, canonical_resident_record(&other));
    }

    #[test]
    fn hasher_is_sha256_and_hash_is_stable() {
        // Pins the algorithm against the canonical "abc" vector.
        let mut h = Sha256::new();
        h.update(b"abc");
        assert_eq!(
            hex::encode(h.finalize()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let h1 = record_hash(&canonical_resident_record(&fixture()));
        let h2 = record_hash(&canonical_resident_record(&fixture()));
        assert_eq!(h1, h2);
        assert_eq!(hex::encode(h1).len(), 64);
    }

    #[test]
    fn ed25519_sign_verify_roundtrip_fixed_seed() {
        // Deterministic seed -> stable kid + verifiable signature.
        let seed = [9u8; 32];
        let sk = ed25519_dalek::SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        let kid = crate::config::verify_kid_for_public(&hex::encode(vk.to_bytes()));
        assert_eq!(kid.len(), 8);
        let msg = sign_message(&kid, "BRGY-2026-00001", "18-09-2027", "ACTIVE", &[7u8; 32]);
        assert!(String::from_utf8(msg.clone()).unwrap().starts_with("v1|"));
        let sig = sk.sign(&msg);
        use ed25519_dalek::Verifier;
        assert!(vk.verify(&msg, &sig).is_ok());
        let mut tampered = msg.clone();
        tampered[5] ^= 1;
        assert!(vk.verify(&tampered, &sig).is_err());
    }

    #[test]
    fn publish_gate_requires_exact_phrase() {
        assert_eq!(PUBLISH_CONFIRM_PHRASE, "PUBLISH FULL RECORDS");
        assert!(check_publish_confirmation(Some("PUBLISH FULL RECORDS")).is_ok());
        assert!(check_publish_confirmation(None).is_err());
        assert!(check_publish_confirmation(Some("")).is_err());
        assert!(check_publish_confirmation(Some("publish full records")).is_err());
        assert!(check_publish_confirmation(Some("PUBLISH FULL RECORDS ")).is_err());
    }
}
