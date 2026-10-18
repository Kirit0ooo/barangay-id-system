use crate::crypto::load_key;
use crate::db::AppState;
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Local;
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone, PartialEq)]
pub struct Resident {
    pub id: Option<i64>,
    pub resident_id: String,
    pub last_name: String,
    pub first_name: String,
    pub middle_name: Option<String>,
    pub suffix: Option<String>,
    pub birth_date: String,
    pub birth_place: Option<String>,
    pub gender: String,
    pub civil_status: Option<String>,
    pub citizenship: Option<String>,
    pub occupation: Option<String>,
    pub contact_number: Option<String>,
    pub address_street: Option<String>,
    pub address_purok: Option<String>,
    pub address_barangay: Option<String>,
    pub address_city: Option<String>,
    pub address_province: Option<String>,
    pub religion: Option<String>,
    pub blood_type: Option<String>,
    pub height: Option<String>,
    pub weight: Option<String>,
    pub philhealth: Option<String>,
    pub sss: Option<String>,
    pub tin: Option<String>,
    pub voters_id: Option<String>,
    pub emergency_contact_name: Option<String>,
    pub emergency_contact_relation: Option<String>,
    pub emergency_contact_number: Option<String>,
    /// Optional link to a registered resident (their `resident_id`, e.g.
    /// BRGY-2026-00001). Free-form name/number above is kept for
    /// non-resident contacts; when this is set it must match an existing
    /// resident (checked at create/update). See README "Emergency contact".
    pub emergency_contact_resident_id: Option<String>,
    pub photo_filename: Option<String>,
    pub is_active: Option<i64>,
    pub date_registered: Option<String>,
    pub date_updated: Option<String>,
    pub remarks: Option<String>,
    pub valid_until: Option<String>,
    /// Legacy PDF audit trail (last generated ID PDF path + timestamp).
    /// DORMANT since PDF removal: never written, kept in struct + schema so
    /// existing DBs keep mapping without migration errors.
    pub last_pdf_path: Option<String>,
    pub last_pdf_generated_at: Option<String>,
    /// Free-form address for a non-resident emergency contact. Appended last
    /// so column order stays aligned with fresh CREATE TABLE order.
    pub emergency_contact_address: Option<String>,
}

/// Explicit column list for all Resident queries — P2 fragility fix.
/// Replaces `SELECT *` so schema drift (e.g. future DROP of last_pdf_*)
/// is caught at compile-time via column count, and legacy `email` column
/// (dropped in init_db) is never selected. Order matches struct field order
/// for readability; sqlx maps by name, not position.
pub const RESIDENT_COLUMNS: &str = "id, resident_id, last_name, first_name, middle_name, suffix, birth_date, birth_place, gender, civil_status, citizenship, occupation, contact_number, address_street, address_purok, address_barangay, address_city, address_province, religion, blood_type, height, weight, philhealth, sss, tin, voters_id, emergency_contact_name, emergency_contact_relation, emergency_contact_number, emergency_contact_resident_id, photo_filename, is_active, date_registered, date_updated, remarks, valid_until, last_pdf_path, last_pdf_generated_at, emergency_contact_address";

#[derive(Debug, Serialize, Deserialize)]
pub struct ResidentInput {
    pub resident: Resident,
    #[serde(rename = "photoData")]
    pub photo_data: Option<String>,
}

#[tauri::command]
pub async fn generate_next_id(state: State<'_, Arc<Mutex<AppState>>>) -> Result<String, String> {
    let pool = { state.lock().await.pool.clone() };
    let year = Local::now().format("%Y").to_string();
    // F5: atomic read under BEGIN IMMEDIATE so concurrent callers serialize
    // instead of both reading the same MAX suffix (MAX alone still races).
    // Transaction is always closed via COMMIT on success or ROLLBACK on error
    // to avoid leaving the pooled connection in an open txn (would block the pool).
    let mut conn = pool.acquire().await.map_err(|e| e.to_string())?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| e.to_string())?;
    let pattern = format!("BRGY-{}-%", year);
    let max_suffix: Option<i64> = match sqlx::query_scalar(
        "SELECT MAX(CAST(substr(resident_id, -5) AS INTEGER)) FROM residents WHERE resident_id LIKE ?",
    )
    .bind(&pattern)
    .fetch_one(&mut *conn)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(e.to_string());
        }
    };
    if let Err(e) = sqlx::query("COMMIT").execute(&mut *conn).await {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(e.to_string());
    }
    let next = max_suffix.unwrap_or(0) + 1;
    Ok(format!("BRGY-{}-{:05}", year, next))
}

/// Purok allowlist — mirrors frontend PUROK_OPTIONS (app.js) and the
/// register-form dropdown (index.html). Backend rejects anything else.
pub const ALLOWED_PUROKS: [&str; 5] = ["AVOCADO", "RIMAS", "PASAYAN", "BUBUNAO", "ASLUM"];

/// Strip a raw phone value to bare digits, honouring +63/63 PH country prefix.
/// Returns (digits, looked_like_ph_mobile). Never panics.
fn strip_to_digits(raw: &str) -> (String, bool) {
    let t = raw.trim();
    let mut digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    // +639... / 639... → 09... (PH country code 63)
    if (t.starts_with('+') || digits.len() == 12) && digits.starts_with("63") {
        digits = format!("0{}", &digits[2..]);
    }
    let looks_ph = digits.starts_with("09");
    (digits, looks_ph)
}

/// Validate + normalize a Philippine mobile number to display form `09XX-XXX-XXXX`.
/// Accepts 09XXXXXXXXX, +639XXXXXXXXX, 639XXXXXXXXX, and the same with
/// spaces/dashes/parens. Err on anything else.
pub fn normalize_ph_contact(raw: &str) -> Result<String, String> {
    let (digits, _) = strip_to_digits(raw);
    if digits.len() != 11
        || !digits.starts_with("09")
        || !digits.chars().all(|c| c.is_ascii_digit())
    {
        return Err(
            "Contact number must be a Philippine mobile (09XX-XXX-XXXX, 11 digits starting 09)"
                .into(),
        );
    }
    Ok(format!(
        "{}-{}-{}",
        &digits[0..4],
        &digits[4..7],
        &digits[7..11]
    ))
}

/// Lenient display formatter for PDF/QR/frontend: PH mobiles → `09XX-XXX-XXXX`,
/// anything else passes through trimmed (legacy landlines/short numbers keep
/// the old charset rule in validate_resident_input).
pub fn format_contact_for_display(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    normalize_ph_contact(t).unwrap_or_else(|_| t.to_string())
}

/// Source-of-truth completeness gate: called before card printing
/// so a card is never printed from a partial record.
/// Required: names, past birth_date, gender, purok allowlist, resident_id.
/// Returns Err("Incomplete record — missing X") on the first gap.
pub fn validate_resident_complete(r: &Resident) -> Result<(), String> {
    if r.last_name.trim().is_empty() {
        return Err("Incomplete record — missing last name".into());
    }
    if r.first_name.trim().is_empty() {
        return Err("Incomplete record — missing first name".into());
    }
    if r.resident_id.trim().is_empty() {
        return Err("Incomplete record — missing resident ID".into());
    }
    // Birth date must parse AND be in the past (rejects 10-09-2026-style futures at PDF time too).
    normalize_birth_date(&r.birth_date).map_err(|_| {
        format!(
            "Incomplete record — invalid or future birth date ({})",
            r.birth_date.trim()
        )
    })?;
    let g = r.gender.trim().to_lowercase();
    if g != "male" && g != "female" {
        return Err("Incomplete record — missing/invalid gender".into());
    }
    match r.address_purok.as_deref() {
        Some(p) if ALLOWED_PUROKS.contains(&p.trim().to_uppercase().as_str()) => {}
        _ => return Err("Incomplete record — missing/invalid purok".into()),
    }
    Ok(())
}

/// F2: escape LIKE wildcards so user input is matched literally.
fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Digit-normalized LIKE pattern for the two contact branches (P1-2/D5).
/// Strips the query to digits (folding a +63/63 PH prefix to 09 form) so a
/// formatted `0917-123-4567` matches the stored `09XX-XXX-XXXX` invariant.
/// Returns `None` for a digit-free (name-only) query — binding "%%" would make
/// the OR branches true for every non-NULL contact and leak the whole table;
/// `None` binds as SQL NULL, which is falsy, so the branches drop out cleanly.
fn digit_search_pattern(query: &str) -> Option<String> {
    let mut q_digits: String = query.chars().filter(|c| c.is_ascii_digit()).collect();
    if (q_digits.len() == 12 || q_digits.len() == 13) && q_digits.starts_with("63") {
        q_digits = format!("0{}", &q_digits[2..]);
    }
    if q_digits.is_empty() {
        None
    } else {
        Some(format!("%{}%", q_digits))
    }
}

/// Parse DD-MM-YYYY, DD/MM/YYYY, or YYYY-MM-DD into canonical YYYY-MM-DD (no future check — shared by DOB + validity).
fn parse_any_date_to_iso(raw: &str, label: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(format!("{} is required", label));
    }
    if s.len() > 20 {
        return Err(format!("{} too long", label));
    }
    let parsed = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .or_else(|_| chrono::NaiveDate::parse_from_str(s, "%d-%m-%Y"))
        .or_else(|_| chrono::NaiveDate::parse_from_str(s, "%d/%m/%Y"))
        .map_err(|_| format!("{} must be YYYY-MM-DD (or DD-MM-YYYY / DD/MM/YYYY)", label))?;
    Ok(parsed.format("%Y-%m-%d").to_string())
}

/// P1-6: valid_until storage is canonical ISO YYYY-MM-DD (display converts to
/// DD-MM-YYYY at the edge). Accepts legacy DD-MM-YYYY / DD/MM/YYYY on the way in.
pub fn normalize_valid_until(raw: &str) -> Result<String, String> {
    parse_any_date_to_iso(raw, "Valid until")
}

/// R3 — DOB normalization: accept DD-MM-YYYY, DD/MM/YYYY, YYYY-MM-DD, trim, validate real date and not future, return canonical YYYY-MM-DD.
pub fn normalize_birth_date(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("Birth date is required".into());
    }
    if s.len() > 20 {
        return Err("Birth date too long".into());
    }
    let iso = parse_any_date_to_iso(s, "Birth date")?;
    let parsed = chrono::NaiveDate::parse_from_str(&iso, "%Y-%m-%d").expect("helper emits ISO");
    if parsed > chrono::Local::now().date_naive() {
        return Err("Birth date cannot be in the future".into());
    }
    // P2-8: sanity cap — a living person cannot be more than 120 years old
    // at time of registration.
    let oldest_allowed = chrono::Local::now().date_naive() - chrono::Duration::days(365 * 120);
    if parsed < oldest_allowed {
        return Err("Birth date is too old — person would be over 120 years".into());
    }
    Ok(iso)
}

/// Auto-uppercase helper: trim + to_uppercase for all text fields except birth_date (ISO canonical).
fn upper_opt(o: &Option<String>) -> Option<String> {
    o.as_ref().map(|s| s.trim().to_uppercase())
}
/// Verify an emergency_contact_resident_id FK value exists (nullable = skip).
/// Runs inside the caller's txn/conn so create + update share one gate.
async fn check_emergency_fk_conn(
    conn: &mut sqlx::SqliteConnection,
    fk: &Option<String>,
) -> Result<(), String> {
    if let Some(f) = fk {
        let t = f.trim();
        if !t.is_empty() {
            let ok: Option<i64> =
                sqlx::query_scalar("SELECT id FROM residents WHERE resident_id = ? LIMIT 1")
                    .bind(t)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(|e| e.to_string())?;
            if ok.is_none() {
                return Err(format!("Emergency contact resident {} not found", t));
            }
        }
    }
    Ok(())
}

#[allow(dead_code)]
async fn check_emergency_fk_pool(pool: &SqlitePool, fk: &Option<String>) -> Result<(), String> {
    if let Some(f) = fk {
        let t = f.trim();
        if !t.is_empty() {
            let ok: Option<i64> =
                sqlx::query_scalar("SELECT id FROM residents WHERE resident_id = ? LIMIT 1")
                    .bind(t)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
            if ok.is_none() {
                return Err(format!("Emergency contact resident {} not found", t));
            }
        }
    }
    Ok(())
}

/// Duplicate-person gate (single owner — was copy-pasted in create, update, and
/// is now also enforced on import). Matches the same normalized name + same ISO
/// birth date. suffix is included so JR/SR/II/III father-son pairs are not
/// collapsed; TRIM() + UPPER() keep legacy mixed-case / trailing-space rows
/// comparable. `exclude_id` lets update/import exclude the row being edited
/// (self is never a duplicate); create passes None (a new row has no id yet).
/// Returns the conflicting resident_id, or None if no duplicate.
pub(crate) async fn duplicate_person_gate(
    conn: &mut sqlx::SqliteConnection,
    r: &Resident,
    exclude_id: Option<i64>,
) -> Result<Option<String>, String> {
    let dup: Option<String> = if let Some(eid) = exclude_id {
        sqlx::query_scalar(
            "SELECT resident_id FROM residents WHERE TRIM(UPPER(last_name)) = TRIM(UPPER(?)) AND TRIM(UPPER(first_name)) = TRIM(UPPER(?)) AND TRIM(COALESCE(UPPER(middle_name), '')) = TRIM(COALESCE(UPPER(?), '')) AND TRIM(COALESCE(UPPER(suffix), '')) = TRIM(COALESCE(UPPER(?), '')) AND birth_date = ? AND id != ? LIMIT 1",
        )
        .bind(&r.last_name)
        .bind(&r.first_name)
        .bind(r.middle_name.clone().unwrap_or_default())
        .bind(r.suffix.clone().unwrap_or_default())
        .bind(&r.birth_date)
        .bind(eid)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query_scalar(
            "SELECT resident_id FROM residents WHERE TRIM(UPPER(last_name)) = TRIM(UPPER(?)) AND TRIM(UPPER(first_name)) = TRIM(UPPER(?)) AND TRIM(COALESCE(UPPER(middle_name), '')) = TRIM(COALESCE(UPPER(?), '')) AND TRIM(COALESCE(UPPER(suffix), '')) = TRIM(COALESCE(UPPER(?), '')) AND birth_date = ? LIMIT 1",
        )
        .bind(&r.last_name)
        .bind(&r.first_name)
        .bind(r.middle_name.clone().unwrap_or_default())
        .bind(r.suffix.clone().unwrap_or_default())
        .bind(&r.birth_date)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| e.to_string())?
    };
    Ok(dup)
}

/// resident_id uniqueness pre-check (single owner — was copy-pasted in create
/// and update). Returns true if another resident already holds `resident_id`.
/// `exclude_id` (update) lets a resident keep their own id; create passes None.
pub(crate) async fn resident_id_clash(
    conn: &mut sqlx::SqliteConnection,
    resident_id: &str,
    exclude_id: Option<i64>,
) -> Result<bool, String> {
    let hit: Option<i64> = if let Some(eid) = exclude_id {
        sqlx::query_scalar("SELECT id FROM residents WHERE resident_id = ? AND id != ? LIMIT 1")
            .bind(resident_id)
            .bind(eid)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| e.to_string())?
    } else {
        sqlx::query_scalar("SELECT id FROM residents WHERE resident_id = ? LIMIT 1")
            .bind(resident_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| e.to_string())?
    };
    Ok(hit.is_some())
}

/// UPDATE row (single owner — was copy-pasted 3×: update_resident, import
/// chunk-flush, import remainder-flush). Owns ONLY the column list — the thing
/// that actually drifts. Callers keep their own policy: photo-swap resolution,
/// date_updated, the gates, and txn COMMIT/ROLLBACK. Sets resident_id =
/// r.resident_id (a real change on edit; a NO-OP on import, which matches by
/// resident_id and never rewrites it). Returns rows_affected.
pub(crate) async fn update_resident_row(
    conn: &mut sqlx::SqliteConnection,
    r: &Resident,
) -> Result<u64, String> {
    sqlx::query(
        r#"UPDATE residents SET
            resident_id = ?, last_name = ?, first_name = ?, middle_name = ?, suffix = ?,
            birth_date = ?, birth_place = ?, gender = ?, civil_status = ?, citizenship = ?,
            occupation = ?, contact_number = ?,
            address_street = ?, address_purok = ?,
            address_barangay = ?, address_city = ?, address_province = ?,
            religion = ?, blood_type = ?, height = ?, weight = ?,
            philhealth = ?, sss = ?, tin = ?, voters_id = ?,
            emergency_contact_name = ?, emergency_contact_relation = ?, emergency_contact_number = ?,
            photo_filename = ?, date_updated = ?, remarks = ?, valid_until = ?,
            emergency_contact_resident_id = ?,
            emergency_contact_address = ?
            WHERE id = ?"#,
    )
    .bind(&r.resident_id)
    .bind(&r.last_name)
    .bind(&r.first_name)
    .bind(&r.middle_name)
    .bind(&r.suffix)
    .bind(&r.birth_date)
    .bind(&r.birth_place)
    .bind(&r.gender)
    .bind(&r.civil_status)
    .bind(&r.citizenship)
    .bind(&r.occupation)
    .bind(&r.contact_number)
    .bind(&r.address_street)
    .bind(&r.address_purok)
    .bind(&r.address_barangay)
    .bind(&r.address_city)
    .bind(&r.address_province)
    .bind(&r.religion)
    .bind(&r.blood_type)
    .bind(&r.height)
    .bind(&r.weight)
    .bind(&r.philhealth)
    .bind(&r.sss)
    .bind(&r.tin)
    .bind(&r.voters_id)
    .bind(&r.emergency_contact_name)
    .bind(&r.emergency_contact_relation)
    .bind(&r.emergency_contact_number)
    .bind(&r.photo_filename)
    .bind(&r.date_updated)
    .bind(&r.remarks)
    .bind(&r.valid_until)
    .bind(&r.emergency_contact_resident_id)
    .bind(&r.emergency_contact_address)
    .bind(r.id)
    .execute(&mut *conn)
    .await
    .map(|res| res.rows_affected())
    .map_err(|e| e.to_string())
}

pub(crate) fn normalize_resident_strings(r: &mut Resident) {
    r.resident_id = r.resident_id.trim().to_uppercase();
    r.last_name = r.last_name.trim().to_uppercase();
    r.first_name = r.first_name.trim().to_uppercase();
    r.middle_name = upper_opt(&r.middle_name);
    r.suffix = upper_opt(&r.suffix);
    r.birth_place = upper_opt(&r.birth_place);
    r.gender = r.gender.trim().to_uppercase();
    r.civil_status = upper_opt(&r.civil_status);
    r.citizenship = upper_opt(&r.citizenship);
    r.occupation = upper_opt(&r.occupation);
    // Contact numbers: PH mobiles stored in display form 09XX-XXX-XXXX (source
    // of truth for PDF/QR/preview); other values keep trimmed passthrough.
    r.contact_number = r
        .contact_number
        .as_ref()
        .map(|s| format_contact_for_display(&s.to_uppercase()));
    r.address_street = upper_opt(&r.address_street);
    r.address_purok = upper_opt(&r.address_purok);
    r.address_barangay = upper_opt(&r.address_barangay);
    r.address_city = upper_opt(&r.address_city);
    r.address_province = upper_opt(&r.address_province);
    r.religion = upper_opt(&r.religion);
    r.blood_type = upper_opt(&r.blood_type);
    r.height = upper_opt(&r.height);
    r.weight = upper_opt(&r.weight);
    r.philhealth = upper_opt(&r.philhealth);
    r.sss = upper_opt(&r.sss);
    r.tin = upper_opt(&r.tin);
    r.voters_id = upper_opt(&r.voters_id);
    r.emergency_contact_name = upper_opt(&r.emergency_contact_name);
    r.emergency_contact_relation = upper_opt(&r.emergency_contact_relation);
    r.emergency_contact_number = r.emergency_contact_number.as_ref().map(|s| {
        let t = s.trim();
        if t.to_uppercase().starts_with("BRGY-") {
            t.to_uppercase()
        } else {
            format_contact_for_display(&s.to_uppercase())
        }
    });
    r.emergency_contact_resident_id = r
        .emergency_contact_resident_id
        .as_ref()
        .map(|s| s.trim().to_uppercase());
    r.emergency_contact_address = upper_opt(&r.emergency_contact_address);
    r.remarks = upper_opt(&r.remarks);
    r.valid_until = upper_opt(&r.valid_until);
    // birth_date is ISO canonical YYYY-MM-DD — leave as-trimmed here; normalized via normalize_birth_date afterwards
    r.birth_date = r.birth_date.trim().to_string();
}

pub(crate) fn validate_resident_input(r: &Resident) -> Result<(), String> {
    if r.last_name.trim().is_empty() {
        return Err("Last name is required".into());
    }
    if r.first_name.trim().is_empty() {
        return Err("First name is required".into());
    }
    if r.birth_date.trim().is_empty() {
        return Err("Birth date is required".into());
    }
    if r.gender.trim().is_empty() {
        return Err("Gender is required".into());
    }
    let g = r.gender.trim().to_lowercase();
    if g != "male" && g != "female" {
        return Err("Gender must be Male or Female".into());
    }
    if r.last_name.len() > 100 || r.first_name.len() > 100 {
        return Err("Name too long (max 100 chars)".into());
    }
    // R3/F9: birth_date normalized via helper — accepts YYYY-MM-DD, DD-MM-YYYY, DD/MM/YYYY, validates real date + not future.
    normalize_birth_date(&r.birth_date)?;
    // Contact: PH mobiles normalize to 09XX-XXX-XXXX; legacy non-PH values
    // keep the old charset/length rule so existing rows never break.
    if let Some(c) = &r.contact_number {
        if c.len() > 20 {
            return Err("Contact number too long".into());
        }
        if !c.trim().is_empty() {
            let t = c.trim();
            let (digits, looks_ph) = strip_to_digits(t);
            if looks_ph || t.starts_with('+') {
                // Looks like a PH mobile (or E.164) → strict 11-digit 09 rule.
                if digits.len() != 11 || !digits.starts_with("09") {
                    return Err("Contact number must be a Philippine mobile (09XX-XXX-XXXX, 11 digits starting 09)".into());
                }
            } else {
                // Legacy path: digits/+/-/space/parens only, 7–20 chars.
                if t.len() < 7 {
                    return Err("Contact number too short (min 7 digits)".into());
                }
                if !t
                    .chars()
                    .all(|ch| ch.is_ascii_digit() || "+- ()".contains(ch))
                {
                    return Err(
                        "Contact number may only contain digits, +, -, spaces, parentheses".into(),
                    );
                }
                if !t.chars().any(|ch| ch.is_ascii_digit()) {
                    return Err("Contact number must contain at least one digit".into());
                }
            }
        }
    }
    // Emergency contact: validated name (2–60 chars) + phone (same PH rule).
    // A BRGY- ID value is format-checked only here; existence is verified
    // against the DB at create/update (emergency_contact_resident_id FK).
    if let Some(n) = &r.emergency_contact_name {
        let t = n.trim();
        if !t.is_empty() {
            if t.chars().count() < 2 || t.chars().count() > 60 {
                return Err("Emergency contact name must be 2–60 characters".into());
            }
            if !t
                .chars()
                .all(|ch| ch.is_alphabetic() || " .-'Ññ".contains(ch))
            {
                return Err(
                    "Emergency contact name may only contain letters, spaces, .-' characters"
                        .into(),
                );
            }
        }
    }
    if let Some(c) = &r.emergency_contact_number {
        if c.len() > 32 {
            return Err("Emergency contact number too long".into());
        }
        let t = c.trim();
        if !t.is_empty() && !t.to_uppercase().starts_with("BRGY-") {
            let (digits, looks_ph) = strip_to_digits(t);
            if looks_ph || t.starts_with('+') {
                if digits.len() != 11 || !digits.starts_with("09") {
                    return Err(
                        "Emergency contact number must be a Philippine mobile (09XX-XXX-XXXX)"
                            .into(),
                    );
                }
            } else if !t
                .chars()
                .all(|ch| ch.is_ascii_digit() || "+- ()".contains(ch))
            {
                return Err(
                    "Emergency contact number may only contain digits, +, -, spaces, parentheses"
                        .into(),
                );
            }
        }
    }
    // Purok allowlist — frontend dropdown is mirrored here (never free text).
    if let Some(p) = &r.address_purok {
        if !p.trim().is_empty() && !ALLOWED_PUROKS.contains(&p.trim().to_uppercase().as_str()) {
            return Err("Purok must be one of: AVOCADO, RIMAS, PASAYAN, BUBUNAO, ASLUM".into());
        }
    }
    if r.resident_id.len() > 32 {
        return Err("Resident ID too long".into());
    }
    if r.resident_id.trim().is_empty() {
        return Err("Resident ID is required".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn create_resident(
    state: State<'_, Arc<Mutex<AppState>>>,
    input: ResidentInput,
) -> Result<i64, String> {
    let mut resident = input.resident;
    // Auto-uppercase all text fields except birth_date (ISO canonical) before any validation/storage
    normalize_resident_strings(&mut resident);

    // R3 — normalize DOB to canonical YYYY-MM-DD before any storage/validation (accepts DD-MM-YYYY, DD/MM/YYYY, YYYY-MM-DD).
    // Fail fast before photo txn so orphan handling is not needed for date errors.
    match normalize_birth_date(&resident.birth_date) {
        Ok(norm) => resident.birth_date = norm,
        Err(e) => return Err(e),
    }
    // P1-6: valid_until storage is canonical ISO YYYY-MM-DD (accepts legacy DD-MM-YYYY).
    if let Some(vu) = resident.valid_until.clone() {
        if !vu.trim().is_empty() {
            match normalize_valid_until(&vu) {
                Ok(norm) => resident.valid_until = Some(norm),
                Err(e) => return Err(e),
            }
        }
    }

    // Perf-save-001: fail fast on bad input BEFORE the global Mutex /
    // SQLite write txn. (Empty resident_id is auto-filled inside the txn,
    // so that one rule is enforced after the fill instead.)
    if !resident.resident_id.trim().is_empty() {
        validate_resident_input(&resident)?;
    }

    // Guard photo payload size (base64 ~ 8 MB ~ 6 MB binary) to avoid DoS
    if let Some(ref pd) = input.photo_data {
        if pd.len() > 8 * 1024 * 1024 {
            return Err("Photo too large (max 8MB base64)".into());
        }
    }

    // Perf-save-001: photo ingest (the dominant save cost on phone photos:
    // base64 decode + EXIF orient + resize + JPEG encode) runs with NO
    // locks held. Only the dir is cloned under a short lock; the JPEG
    // lands on disk BEFORE BEGIN so the write txn stays short (dup-check
    // + ID fill + insert + commit). Orphan cleanup on later failure below.
    let app_data_dir = { state.lock().await.app_data_dir.clone() };
    let mut saved_photo: Option<String> = None;
    if let Some(photo_data) = input.photo_data {
        if !photo_data.is_empty() {
            let (filename, jpeg) = process_photo_bounded(photo_data)
                .await
                .map_err(|e| format!("Failed to save photo: {}", e))?;
            // P1 sync-blocking fix: photo write off the async runtime.
            let photos_dir = app_data_dir.join("photos");
            let fname = filename.clone();
            let data = jpeg.clone();
            tokio::task::spawn_blocking(move || -> Result<(), String> {
                std::fs::create_dir_all(&photos_dir)
                    .map_err(|e| format!("Failed to save photo: {}", e))?;
                std::fs::write(photos_dir.join(&fname), &data)
                    .map_err(|e| format!("Failed to save photo: {}", e))?;
                Ok(())
            })
            .await
            .map_err(|e| format!("Photo save task failed: {}", e))??;
            saved_photo = Some(filename.clone());
            resident.photo_filename = Some(filename);
        }
    }

    // M-1: clone handles, release lock before DB awaits (never hold the
    // Mutex across the write txn; BEGIN IMMEDIATE already serializes writers).
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };

    // R2 — retry loop for MAX->INSERT on UNIQUE resident_id violation (max 2 attempts).
    // Each attempt holds its own BEGIN IMMEDIATE; on UNIQUE we rollback, re-query MAX with a fresh BEGIN and retry once.
    // Orphan photo is cleaned on each definitive failure, retained across retry if it succeeds.
    let mut attempt: u8 = 0;
    let mut current = resident;
    // C2: track whether the caller supplied an explicit ID. Only auto-filled
    // (empty-input) IDs may retry with a fresh MAX on collision; an explicit
    // ID collision must error so operator typos are never silently reassigned.
    let explicit_id = !current.resident_id.trim().is_empty();
    loop {
        let mut conn = pool.acquire().await.map_err(|e| e.to_string())?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
        // Auto-generate ID if empty before validation (inside the write lock).
        if current.resident_id.trim().is_empty() {
            let year = Local::now().format("%Y").to_string();
            let pattern = format!("BRGY-{}-%", year);
            let max_suffix: Option<i64> = sqlx::query_scalar(
                "SELECT MAX(CAST(substr(resident_id, -5) AS INTEGER)) FROM residents WHERE resident_id LIKE ?",
            )
            .bind(&pattern)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
            current.resident_id = format!("BRGY-{}-{:05}", year, max_suffix.unwrap_or(0) + 1);
        }

        // Validation errors must ROLLBACK and remove the pre-written photo to avoid orphans.
        // Clean up orphan photo on insert failure (retry-aware).
        if let Err(e) = validate_resident_input(&current) {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            if let Some(fname) = saved_photo.as_ref() {
                let p = app_data_dir.join("photos").join(fname);
                let _ = fs::remove_file(p);
            }
            return Err(e);
        }

        // F3 (create path): fail fast on duplicate resident_id with a clear message.
        // If this is the first attempt, treat it as retryable race and retry with a fresh MAX.
        if resident_id_clash(&mut *conn, &current.resident_id, None).await? {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            let msg = "Resident ID already exists".to_string();
            let is_retryable = attempt == 0 && !explicit_id;
            if is_retryable {
                attempt += 1;
                // Clear ID so next loop re-queries MAX with BEGIN IMMEDIATE and generates a fresh suffix.
                // On each failure intermediate orphan is NOT deleted — retained for retry success.
                tracing::warn!(
                    "create_resident duplicate pre-check, retrying with fresh MAX (attempt {})",
                    attempt
                );
                current.resident_id = String::new();
                continue;
            }
            if let Some(fname) = saved_photo.as_ref() {
                let p = app_data_dir.join("photos").join(fname);
                let _ = fs::remove_file(p);
            }
            return Err(msg);
        }

        // Duplicate-person gate: same normalized name + same ISO birth date
        // (normalize_resident_strings already uppercased names; UPPER() keeps
        // legacy mixed-case rows comparable). TRIM() handles legacy trailing
        // spaces; suffix is included so JR/SR/II/III father-son pairs are not
        // collapsed. Runs in-txn so the check and the INSERT below are atomic
        // under BEGIN IMMEDIATE.
        if let Some(other) = duplicate_person_gate(&mut *conn, &current, None).await? {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            if let Some(fname) = saved_photo.as_ref() {
                let p = app_data_dir.join("photos").join(fname);
                let _ = fs::remove_file(p);
            }
            return Err(format!(
                "Possible duplicate: a resident with the same name and birth date already exists (ID: {}). Use Edit instead, or confirm this is a different person.",
                other
            ));
        }

        // FK gate: emergency_contact_resident_id must match an existing resident.
        if let Err(e) =
            check_emergency_fk_conn(&mut *conn, &current.emergency_contact_resident_id).await
        {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            if let Some(fname) = saved_photo.as_ref() {
                let p = app_data_dir.join("photos").join(fname);
                let _ = fs::remove_file(p);
            }
            return Err(e);
        }

        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        current.date_registered = Some(now.clone());
        current.date_updated = Some(now);
        current.is_active = Some(1);

        // Auto 1 year validity (per requirement - fixed, no UI). Stored ISO; display converts to DD-MM-YYYY.
        if current.valid_until.is_none() || current.valid_until.as_deref().unwrap_or("").is_empty()
        {
            let vu = Local::now().date_naive() + chrono::Duration::days(365);
            current.valid_until = Some(vu.format("%Y-%m-%d").to_string());
            let _ = vu;
        }
        let insert_result = sqlx::query(
            r#"INSERT INTO residents (
                resident_id, last_name, first_name, middle_name, suffix,
                birth_date, birth_place, gender, civil_status, citizenship,
                occupation, contact_number,
                address_street, address_purok,
                address_barangay, address_city, address_province,
                religion, blood_type, height, weight,
                philhealth, sss, tin, voters_id,
                emergency_contact_name, emergency_contact_relation, emergency_contact_number,
                photo_filename, is_active, date_registered, date_updated, remarks, valid_until,
                emergency_contact_resident_id,
                emergency_contact_address
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&current.resident_id)
        .bind(&current.last_name)
        .bind(&current.first_name)
        .bind(&current.middle_name)
        .bind(&current.suffix)
        .bind(&current.birth_date)
        .bind(&current.birth_place)
        .bind(&current.gender)
        .bind(&current.civil_status)
        .bind(&current.citizenship)
        .bind(&current.occupation)
        .bind(&current.contact_number)
        .bind(&current.address_street)
        .bind(&current.address_purok)
        .bind(&current.address_barangay)
        .bind(&current.address_city)
        .bind(&current.address_province)
        .bind(&current.religion)
        .bind(&current.blood_type)
        .bind(&current.height)
        .bind(&current.weight)
        .bind(&current.philhealth)
        .bind(&current.sss)
        .bind(&current.tin)
        .bind(&current.voters_id)
        .bind(&current.emergency_contact_name)
        .bind(&current.emergency_contact_relation)
        .bind(&current.emergency_contact_number)
        .bind(&current.photo_filename)
        .bind(current.is_active)
        .bind(&current.date_registered)
        .bind(&current.date_updated)
        .bind(&current.remarks)
        .bind(&current.valid_until)
        .bind(&current.emergency_contact_resident_id)
        .bind(&current.emergency_contact_address)
        .execute(&mut *conn)
        .await;

        match insert_result {
            Ok(result) => {
                if sqlx::query("COMMIT").execute(&mut *conn).await.is_err() {
                    if let Some(fname) = saved_photo.as_ref() {
                        let p = app_data_dir.join("photos").join(fname);
                        let _ = fs::remove_file(p);
                    }
                    return Err("Failed to commit resident".into());
                }
                return Ok(result.last_insert_rowid());
            }
            Err(e) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                let msg = e.to_string();
                let is_unique = msg.contains("UNIQUE")
                    || msg.to_lowercase().contains("unique")
                    || msg.contains("resident_id");
                if is_unique && attempt == 0 && !explicit_id {
                    attempt += 1;
                    tracing::warn!("create_resident UNIQUE violation, retrying with fresh MAX (attempt {}): {}", attempt, msg);
                    // Clean up photo only on final failure; retain for retry success -> so no delete here.
                    current.resident_id = String::new();
                    continue;
                }
                // Final failure — clean up orphan photo
                if let Some(fname) = saved_photo {
                    let p = app_data_dir.join("photos").join(fname);
                    let _ = fs::remove_file(p);
                }
                if is_unique {
                    return Err("Resident ID already exists".into());
                }
                return Err(msg);
            }
        }
    }
}

#[tauri::command]
pub async fn get_resident(
    state: State<'_, Arc<Mutex<AppState>>>,
    id: i64,
) -> Result<Resident, String> {
    let pool = { state.lock().await.pool.clone() };

    let resident = sqlx::query_as::<_, Resident>(&format!(
        "SELECT {} FROM residents WHERE id = ?",
        RESIDENT_COLUMNS
    ))
    .bind(id)
    .fetch_one(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(resident)
}

#[tauri::command]
pub async fn update_resident(
    state: State<'_, Arc<Mutex<AppState>>>,
    input: ResidentInput,
) -> Result<bool, String> {
    let mut resident = input.resident;
    // Auto-uppercase all text fields except birth_date (ISO canonical) before any validation/storage
    normalize_resident_strings(&mut resident);

    // R3 — normalize DOB before validation/storage (same helper as create).
    match normalize_birth_date(&resident.birth_date) {
        Ok(norm) => resident.birth_date = norm,
        Err(e) => return Err(e),
    }
    // P1-6: valid_until storage is canonical ISO YYYY-MM-DD (accepts legacy DD-MM-YYYY).
    if let Some(vu) = resident.valid_until.clone() {
        if !vu.trim().is_empty() {
            match normalize_valid_until(&vu) {
                Ok(norm) => resident.valid_until = Some(norm),
                Err(e) => return Err(e),
            }
        }
    }
    validate_resident_input(&resident)?;
    if let Some(ref pd) = input.photo_data {
        if pd.len() > 8 * 1024 * 1024 {
            return Err("Photo too large (max 8MB base64)".into());
        }
    }

    // Perf-save-001: new-photo CPU (decode + EXIF + resize + JPEG encode)
    // with NO locks held; bytes are written + old file removed under the
    // lock below (fast I/O only), keeping the clash-check → UPDATE gap
    // short and validation-failure paths lock-free.
    let new_photo: Option<(String, Vec<u8>)> = match input.photo_data {
        Some(pd) if !pd.is_empty() && pd.starts_with("data:") => Some(
            process_photo_bounded(pd)
                .await
                .map_err(|e| format!("Failed to save photo: {}", e))?,
        ),
        _ => None,
    };

    // M-1/M-6: clone handles, release lock before DB awaits; track new photo for orphan cleanup
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };
    let mut new_photo_written: Option<PathBuf> = None;
    // P0-2: write-new BEFORE the write txn so COMMIT never points at a
    // missing file. Orphan cleanup on every failure path below.
    // (Photo CPU above already ran lock-free; this is fast file I/O only.)
    if let Some((filename, jpeg)) = &new_photo {
        let photos_dir = app_data_dir.join("photos");
        let fname = filename.clone();
        let data = jpeg.clone();
        let new_path =
            tokio::task::spawn_blocking(move || -> Result<std::path::PathBuf, String> {
                std::fs::create_dir_all(&photos_dir)
                    .map_err(|e| format!("Failed to save photo: {}", e))?;
                let p = photos_dir.join(&fname);
                std::fs::write(&p, &data).map_err(|e| format!("Failed to save photo: {}", e))?;
                Ok(p)
            })
            .await
            .map_err(|e| format!("Photo save task failed: {}", e))??;
        new_photo_written = Some(new_path);
        resident.photo_filename = Some(filename.clone());
    }
    // C3: authoritative checks + UPDATE run atomically under BEGIN IMMEDIATE
    // on one connection (no check-then-UPDATE race). Photo bytes/file I/O
    // stay outside the txn; only fast SELECTs + UPDATE hold it.
    let mut conn = pool.acquire().await.map_err(|e| e.to_string())?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            if let Some(p) = new_photo_written.as_ref() {
                let _ = fs::remove_file(p);
            }
            e.to_string()
        })?;
    // F3: resident_id uniqueness pre-check (friendly error before hitting UNIQUE).
    if resident_id_clash(&mut *conn, &resident.resident_id, resident.id).await? {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        if let Some(p) = new_photo_written.as_ref() {
            let _ = fs::remove_file(p);
        }
        return Err("Resident ID already exists".into());
    }
    // FK gate: emergency_contact_resident_id must match an existing resident.
    if let Err(e) =
        check_emergency_fk_conn(&mut *conn, &resident.emergency_contact_resident_id).await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        if let Some(p) = new_photo_written.as_ref() {
            let _ = fs::remove_file(p);
        }
        return Err(e);
    }
    // Duplicate-person gate (update path): same check as create, excluding self.
    // TRIM() + suffix included for parity (see create gate above).
    if let Some(other) = duplicate_person_gate(&mut *conn, &resident, resident.id).await? {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        if let Some(p) = new_photo_written.as_ref() {
            let _ = fs::remove_file(p);
        }
        return Err(format!(
            "Possible duplicate: a resident with the same name and birth date already exists (ID: {}). Use Edit instead, or confirm this is a different person.",
            other
        ));
    }
    // P0-2: old file is fetched from the DB (not the stale input echo) inside
    // the txn so the read is consistent with the UPDATE below.
    let old_photo_filename: Option<String> =
        sqlx::query_scalar("SELECT photo_filename FROM residents WHERE id = ?")
            .bind(resident.id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| e.to_string())?
            .flatten();
    if new_photo.is_none() {
        // P2-12: no new upload — never trust the echoed client filename.
        // Explicit empty = operator pressed Remove Photo → clear;
        // otherwise keep the DB value (stale echoes can't overwrite it).
        let echo_empty = resident
            .photo_filename
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);
        resident.photo_filename = if echo_empty {
            None
        } else {
            old_photo_filename.clone()
        };
    }

    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    resident.date_updated = Some(now);

    let result = update_resident_row(&mut *conn, &resident).await;
    let result = match result {
        Ok(n) => n,
        Err(msg) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            if let Some(p) = new_photo_written.as_ref() {
                let _ = fs::remove_file(p);
            }
            if msg.contains("UNIQUE") || msg.to_lowercase().contains("unique") {
                return Err("Resident ID already exists".into());
            }
            return Err(msg);
        }
    };

    if result == 0 {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        if let Some(p) = new_photo_written.as_ref() {
            let _ = fs::remove_file(p);
        }
        return Ok(false);
    }
    if sqlx::query("COMMIT").execute(&mut *conn).await.is_err() {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        if let Some(p) = new_photo_written.as_ref() {
            let _ = fs::remove_file(p);
        }
        return Err("Failed to commit resident update".into());
    }
    if let Some(new_path) = new_photo_written.as_ref() {
        // P0-2: commit won — now safe to drop the replaced file (basename-
        // gated so a stale absolute/relative DB value can never escape photos/).
        if let Some(old) = old_photo_filename.as_deref() {
            if let Some(base) = photo_basename(old) {
                let old_path = app_data_dir.join("photos").join(&base);
                if old_path != *new_path && old_path.is_file() {
                    let _ = fs::remove_file(&old_path);
                }
            }
        }
    }
    Ok(result > 0)
}

#[tauri::command]
pub async fn delete_resident(
    state: State<'_, Arc<Mutex<AppState>>>,
    id: i64,
) -> Result<bool, String> {
    let pool = { state.lock().await.pool.clone() };

    // P0 FK orphan guard: prevent soft-deleting a resident that is still
    // referenced as emergency contact (emergency_contact_resident_id). The DB
    // has no FK constraint (single-table legacy), so the app must enforce it.
    let resident_id: Option<String> =
        sqlx::query_scalar("SELECT resident_id FROM residents WHERE id = ?")
            .bind(id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?
            .flatten();
    if let Some(rid) = resident_id {
        let ref_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM residents WHERE emergency_contact_resident_id = ? AND is_active = 1 AND id != ?",
        )
        .bind(&rid)
        .bind(id)
        .fetch_one(&pool)
        .await
        .map_err(|e| e.to_string())?;
        if ref_count > 0 {
            return Err(format!(
                "Cannot deactivate {}: still referenced as emergency contact by {} active resident(s). Update those records first.",
                rid, ref_count
            ));
        }
        // Legacy path where operator stored BRGY- ID in emergency_contact_number
        let legacy_ref: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM residents WHERE UPPER(TRIM(emergency_contact_number)) = UPPER(TRIM(?)) AND is_active = 1 AND id != ?",
        )
        .bind(&rid)
        .bind(id)
        .fetch_one(&pool)
        .await
        .map_err(|e| e.to_string())?;
        if legacy_ref > 0 {
            return Err(format!(
                "Cannot deactivate {}: still referenced (legacy emergency_contact_number) by {} active resident(s). Update those records first.",
                rid, legacy_ref
            ));
        }
    }

    let result = sqlx::query(
        "UPDATE residents SET is_active = 0, date_updated = datetime('now') WHERE id = ?",
    )
    .bind(id)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(result.rows_affected() > 0)
}

pub const SEARCH_RESULT_LIMIT: i64 = 2000;

#[tauri::command]
pub async fn search_residents(
    state: State<'_, Arc<Mutex<AppState>>>,
    query: String,
    purok: Option<String>,
    include_inactive: Option<bool>,
) -> Result<Vec<Resident>, String> {
    let pool = { state.lock().await.pool.clone() };
    // F2: escape LIKE wildcards in user input; ESCAPE '\' keeps % _ \\ literal.
    let search = format!("%{}%", escape_like(&query));
    // P1-2/D5: digit-normalized contact branch. digit_search_pattern strips the
    // query to digits (folding +63/63 → 09) and returns None for a name-only
    // query — binding "%%" there would satisfy the OR branches for every non-
    // NULL contact and leak the whole table, so None binds as falsy NULL.
    let digits_search = digit_search_pattern(&query);
    // P0-3: soft-deleted rows are hidden by default; callers that need the
    // Inactive/All categories pass include_inactive=true explicitly.
    let show_all = include_inactive.unwrap_or(false) as i64;
    let purok_filter = purok.unwrap_or_default();

    // ponytail: digit-strip via nested REPLACE (no new column/migration); matches the write-side 09XX-XXX-XXXX invariant.
    let sql = format!(
        r#"SELECT {} FROM residents
               WHERE (last_name LIKE ? ESCAPE '\' OR first_name LIKE ? ESCAPE '\' OR resident_id LIKE ? ESCAPE '\' OR contact_number LIKE ? ESCAPE '\'
                 OR REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(contact_number, '-', ''), ' ', ''), '(', ''), ')', ''), '+', '') LIKE ?
                 OR REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(emergency_contact_number, '-', ''), ' ', ''), '(', ''), ')', ''), '+', '') LIKE ?)
                AND (? = '' OR address_purok = ?)
                AND (? = 1 OR is_active = 1)
                ORDER BY last_name, first_name LIMIT ?"#,
        RESIDENT_COLUMNS
    );
    let residents = sqlx::query_as::<_, Resident>(&sql)
        .bind(&search)
        .bind(&search)
        .bind(&search)
        .bind(&search)
        .bind(digits_search.clone())
        .bind(digits_search)
        .bind(&purok_filter)
        .bind(&purok_filter)
        .bind(show_all)
        .bind(SEARCH_RESULT_LIMIT)
        .fetch_all(&pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(residents)
}

#[tauri::command]
pub async fn get_all_residents(
    state: State<'_, Arc<Mutex<AppState>>>,
    page: Option<i64>,
    per_page: Option<i64>,
) -> Result<Vec<Resident>, String> {
    let pool = { state.lock().await.pool.clone() };
    let per = per_page.unwrap_or(5000).clamp(1, 5000);
    let pg = page.unwrap_or(1).max(1);
    let offset = (pg - 1) * per;
    let residents = sqlx::query_as::<_, Resident>(&format!(
        "SELECT {} FROM residents WHERE is_active = 1 ORDER BY last_name, first_name LIMIT ? OFFSET ?",
        RESIDENT_COLUMNS
    ))
    .bind(per)
    .bind(offset)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(residents)
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub total: i64,
    pub this_year: i64,
    pub male: i64,
    pub female: i64,
    pub active: i64,
    pub renew: i64,
    pub lost: i64,
    pub damage: i64,
}

#[tauri::command]
pub async fn get_residents_count(state: State<'_, Arc<Mutex<AppState>>>) -> Result<i64, String> {
    let pool = { state.lock().await.pool.clone() };
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM residents WHERE is_active = 1")
        .fetch_one(&pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(n)
}

#[tauri::command]
pub async fn get_dashboard_stats(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<DashboardStats, String> {
    let pool = { state.lock().await.pool.clone() };
    let year = Local::now().format("%Y").to_string();
    // P2: single aggregate query replaces 8 round-trips — same predicates,
    // one table scan. Mirrors the previous per-status semantics exactly:
    // active bucket includes NULL/blank remarks as ACTIVE; gender/status
    // via UPPER() for legacy mixed-case rows; is_active=1 gate always.
    let row: (i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
            COUNT(*) , \
            COALESCE(SUM(CASE WHEN substr(date_registered, 1, 4) = ? THEN 1 ELSE 0 END),0), \
            COALESCE(SUM(CASE WHEN UPPER(gender) = 'MALE' THEN 1 ELSE 0 END),0), \
            COALESCE(SUM(CASE WHEN UPPER(gender) = 'FEMALE' THEN 1 ELSE 0 END),0), \
            COALESCE(SUM(CASE WHEN (remarks IS NULL OR TRIM(remarks) = '' OR UPPER(remarks) = 'ACTIVE') THEN 1 ELSE 0 END),0), \
            COALESCE(SUM(CASE WHEN UPPER(remarks) = 'RENEW' THEN 1 ELSE 0 END),0), \
            COALESCE(SUM(CASE WHEN UPPER(remarks) = 'LOST' THEN 1 ELSE 0 END),0), \
            COALESCE(SUM(CASE WHEN UPPER(remarks) = 'DAMAGE' THEN 1 ELSE 0 END),0) \
         FROM residents WHERE is_active = 1",
    )
    .bind(&year)
    .fetch_one(&pool)
    .await
    .map_err(|e| e.to_string())?;
    let (total, this_year, male, female, active, renew, lost, damage) = row;
    Ok(DashboardStats {
        total,
        this_year,
        male,
        female,
        active,
        renew,
        lost,
        damage,
    })
}

#[tauri::command]
pub async fn get_recent_residents(
    state: State<'_, Arc<Mutex<AppState>>>,
    limit: Option<i64>,
) -> Result<Vec<Resident>, String> {
    let pool = { state.lock().await.pool.clone() };
    let n = limit.unwrap_or(10).clamp(1, 50);
    let residents = sqlx::query_as::<_, Resident>(&format!(
        "SELECT {} FROM residents WHERE is_active = 1 ORDER BY date_registered DESC LIMIT ?",
        RESIDENT_COLUMNS
    ))
    .bind(n)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(residents)
}

#[tauri::command]
pub async fn get_residents_by_purok(
    state: State<'_, Arc<Mutex<AppState>>>,
    purok: String,
) -> Result<Vec<Resident>, String> {
    let pool = { state.lock().await.pool.clone() };

    let residents = sqlx::query_as::<_, Resident>(&format!(
        "SELECT {} FROM residents WHERE is_active = 1 AND address_purok = ? ORDER BY last_name, first_name",
        RESIDENT_COLUMNS
    ))
    .bind(purok)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(residents)
}

/// Storage target for resident photos: 600x800 box (aspect preserved).
/// Perf-save-001: Triangle is ~4x faster than Lanczos3 here and visually
/// identical once printed at 20.9x26.9mm/300dpi; images already at/below
/// the box skip resampling entirely (no upscale blur, no wasted CPU).
fn storage_resize(img: image::DynamicImage) -> image::DynamicImage {
    if img.width() <= 600 && img.height() <= 800 {
        return img;
    }
    img.resize(600, 800, image::imageops::FilterType::Triangle)
}

/// Perf-save-001: pure-CPU stage of photo ingest (base64 decode + EXIF
/// orient + bounded resize). No filesystem, no locks — safe to run before
/// acquiring the state Mutex / SQLite write txn.
fn process_photo_to_jpeg(base64_data: &str) -> Result<(String, Vec<u8>)> {
    if base64_data.trim().is_empty() {
        anyhow::bail!("Empty photo data");
    }
    if base64_data.len() > 8 * 1024 * 1024 {
        anyhow::bail!("Photo too large");
    }
    // Accept only PNG, JPG, JPEG as a whole — no chunking.
    let (_mime, body) = if base64_data.starts_with("data:image") {
        let parts: Vec<&str> = base64_data.splitn(2, ',').collect();
        let mime = parts[0].strip_prefix("data:").unwrap_or("");
        let mime_type = mime.split(';').next().unwrap_or("");
        match mime_type {
            "image/png" | "image/jpeg" | "image/jpg" => {}
            other => anyhow::bail!(
                "Unsupported photo format: {} (only PNG, JPG, JPEG accepted)",
                other
            ),
        }
        (mime_type, parts.get(1).unwrap_or(&base64_data).to_string())
    } else {
        ("", base64_data.to_string())
    };

    let bytes = BASE64.decode(body.trim()).map_err(|e| anyhow::anyhow!(e))?;
    if bytes.len() > 6 * 1024 * 1024 {
        anyhow::bail!("Decoded photo too large (max 6MB)");
    }

    // P0 dimension bomb guard: header check BEFORE full decode so a
    // 12000×12000 highly-compressed JPEG (still <6MB) cannot allocate ~500MB.
    const MAX_DIM: u32 = 8000;
    const MAX_PIXELS: u64 = 36_000_000; // ~108MB RGB8
                                        // image 0.24: header-only dimensions via io::Reader (no full pixel alloc).
    if let Ok(reader) = image::io::Reader::new(std::io::Cursor::new(&bytes)).with_guessed_format() {
        if let Ok((hw, hh)) = reader.into_dimensions() {
            if hw > MAX_DIM || hh > MAX_DIM || (hw as u64 * hh as u64) > MAX_PIXELS {
                anyhow::bail!(
                    "Photo dimensions too large ({}×{} > {}×{} or {}MP) — use a smaller image",
                    hw,
                    hh,
                    MAX_DIM,
                    MAX_DIM,
                    MAX_PIXELS / 1_000_000
                );
            }
        }
    }
    // §7 photo processing: normalize EXIF orientation (phone photos) before resize.
    let img = load_photo_oriented(&bytes)?;
    // Second guard after decode (covers orientation swap and cases where
    // header check was skipped due to unknown format).
    let (w, h) = (img.width(), img.height());
    if w > MAX_DIM || h > MAX_DIM || (w as u64 * h as u64) > MAX_PIXELS {
        anyhow::bail!(
            "Photo dimensions too large ({}×{} > {}×{} or {}MP) — use a smaller image",
            w,
            h,
            MAX_DIM,
            MAX_DIM,
            MAX_PIXELS / 1_000_000
        );
    }
    let resized = storage_resize(img);

    let filename = format!("{}.jpg", Uuid::new_v4());
    let mut buf = Vec::with_capacity(256 * 1024);
    {
        let mut cur = std::io::Cursor::new(&mut buf);
        resized.write_to(&mut cur, ImageFormat::Jpeg)?;
    }
    Ok((filename, buf))
}

/// Photo-ingest timing budget: the CPU stage (decode + EXIF + resize +
/// JPEG encode) runs on a blocking thread with a 10s timeout, so one
/// pathological image can't stall the save pipeline. Size is rejected
/// early — the 8MB base64 / 6MB binary caps inside process_photo_to_jpeg
/// fire before any pixel work, and the frontend refuses files > 6MB.
async fn process_photo_bounded(base64_data: String) -> Result<(String, Vec<u8>)> {
    match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::task::spawn_blocking(move || process_photo_to_jpeg(&base64_data)),
    )
    .await
    {
        Err(_) => anyhow::bail!("Photo processing timed out — image may be too large"),
        Ok(Err(join_err)) => Err(anyhow::anyhow!("Photo task failed: {}", join_err)),
        Ok(Ok(inner)) => inner,
    }
}

/// Decode image bytes with JPEG EXIF orientation normalized (§7).
/// `image::load_from_memory` ignores EXIF, so unrotated phone photos would
/// print sideways; this applies orientation 2–8 with no extra crates.
/// Non-JPEG or orientation-less bytes decode unchanged. Never panics.
pub(crate) fn load_photo_oriented(bytes: &[u8]) -> image::ImageResult<image::DynamicImage> {
    let img = image::load_from_memory(bytes)?;
    Ok(normalize_orientation(img, jpeg_exif_orientation(bytes)))
}

fn normalize_orientation(img: image::DynamicImage, o: u8) -> image::DynamicImage {
    match o {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.fliph().rotate90(),
        6 => img.rotate90(),
        7 => img.fliph().rotate270(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// Minimal JPEG APP1/TIFF EXIF orientation parser (values 1–8).
/// Returns 1 (normal) when absent, non-JPEG, or unparseable.
fn jpeg_exif_orientation(bytes: &[u8]) -> u8 {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return 1;
    }
    let mut i = 2usize;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            break;
        }
        let marker = bytes[i + 1];
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            break; // SOS / EOI: no more metadata segments
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if len < 2 {
            break;
        }
        let end = i + 2 + len;
        if end > bytes.len() {
            break;
        }
        if marker == 0xE1 {
            let body = &bytes[i + 4..end];
            if body.len() > 6 && &body[0..6] == b"Exif\0\0" {
                if let Some(o) = tiff_orientation(&body[6..]) {
                    return o;
                }
            }
        }
        i = end;
    }
    1
}

fn tiff_orientation(tiff: &[u8]) -> Option<u8> {
    if tiff.len() < 8 {
        return None;
    }
    let le = if tiff.starts_with(b"II") {
        true
    } else if tiff.starts_with(b"MM") {
        false
    } else {
        return None;
    };
    let get_u16 = |o: usize| -> Option<u16> {
        let b: [u8; 2] = tiff.get(o..o + 2)?.try_into().ok()?;
        Some(if le {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    };
    let get_u32 = |o: usize| -> Option<u32> {
        let b: [u8; 4] = tiff.get(o..o + 4)?.try_into().ok()?;
        Some(if le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    };
    if get_u16(2)? != 42 {
        return None;
    }
    let ifd0 = get_u32(4)? as usize;
    let count = get_u16(ifd0)? as usize;
    for n in 0..count {
        let e = ifd0.checked_add(2)?.checked_add(n.checked_mul(12)?)?;
        if e.checked_add(12)? > tiff.len() {
            break;
        }
        if get_u16(e)? == 0x0112 {
            let v = get_u16(e + 8)?;
            if (1..=8).contains(&v) {
                return Some(v as u8);
            }
            return None;
        }
    }
    None
}

/// Basename of a stored photo value. The DB column may hold a bare filename
/// (`abc.jpg`), a relative path (`photos/abc.jpg`) or a stale absolute path
/// (`D:\...\abc.jpg`, `/mnt/d/.../abc.jpg`) — all collapse to the final
/// component. Empty / directory-ish / traversal values yield None so callers
/// render no photo instead of reading a directory.
pub(crate) fn photo_basename(stored: &str) -> Option<String> {
    let t = stored.trim().trim_matches(|c| c == '"' || c == '\'');
    if t.is_empty() {
        return None;
    }
    let unified = t.replace('\\', "/");
    let base = unified.rsplit('/').next().unwrap_or("").trim();
    if base.is_empty() || base == "." || base == ".." || base.contains('\0') || base.contains("..")
    {
        return None;
    }
    Some(base.to_string())
}

/// Resolve resident photo bytes for preview + PDF.
/// Order: (1) absolute stored path, gated by validate_isolated_write_path so
/// reads stay inside the project tree; (2) basename under
/// `<app_data_dir>/photos`; (3) basename under `<project-root>/data/photos`
/// (covers app_data_dir skew across installs). Supports absolute + relative
/// stored values; returns None when nothing readable exists.
pub(crate) fn resolve_photo_bytes(stored: &str, app_data_dir: &PathBuf) -> Option<Vec<u8>> {
    let t = stored.trim();
    if t.is_empty() {
        return None;
    }
    // Load the at-rest key if present; falls back to raw bytes when missing
    // (e.g. tests, or before first encryption_generate_key).
    let key = load_key().ok();
    let decrypt = |b: Vec<u8>| match &key {
        Some(k) => k.decrypt_or_plaintext(&b),
        None => b,
    };
    // (1) Absolute path inside the project tree.
    let raw = PathBuf::from(t);
    if raw.is_absolute() {
        if let Ok(ok) = crate::db::validate_isolated_write_path(&raw) {
            if ok.is_file() {
                if let Ok(b) = fs::read(&ok) {
                    return Some(decrypt(b));
                }
            }
        }
    }
    // (2)+(3) Basename lookups.
    let base = photo_basename(t)?;
    let mut candidates = vec![app_data_dir.join("photos").join(&base)];
    if let Some(root) = crate::db::get_project_root() {
        candidates.push(root.join("data").join("photos").join(&base));
    }
    for p in candidates {
        if p.is_file() {
            if let Ok(b) = fs::read(&p) {
                return Some(decrypt(b));
            }
        }
    }
    None
}

pub fn get_photo_base64(filename: &str, app_data_dir: &PathBuf) -> Option<String> {
    resolve_photo_bytes(filename, app_data_dir)
        .map(|bytes| format!("data:image/jpeg;base64,{}", BASE64.encode(&bytes)))
}

#[tauri::command]
pub async fn get_resident_photo(
    state: State<'_, Arc<Mutex<AppState>>>,
    id: i64,
) -> Result<Option<String>, String> {
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };
    let resident = sqlx::query_as::<_, Resident>(&format!(
        "SELECT {} FROM residents WHERE id = ?",
        RESIDENT_COLUMNS
    ))
    .bind(id)
    .fetch_one(&pool)
    .await
    .map_err(|e| e.to_string())?;
    if let Some(fname) = resident.photo_filename {
        if let Some(b64) = get_photo_base64(&fname, &app_data_dir) {
            return Ok(Some(b64));
        }
    }
    // No fallback: without a stored upload there is no photo to render.
    Ok(None)
}

#[cfg(test)]
mod photo_tests {
    use super::*;

    fn data_dir() -> PathBuf {
        crate::db::get_app_data_dir()
    }

    #[test]
    fn basename_handles_absolute_relative_and_empty() {
        assert_eq!(photo_basename("abc123.jpg").as_deref(), Some("abc123.jpg"));
        assert_eq!(
            photo_basename("photos/abc123.jpg").as_deref(),
            Some("abc123.jpg")
        );
        assert_eq!(
            photo_basename("D:\\Barangay\\barangay-id-system\\data\\photos\\abc.jpg").as_deref(),
            Some("abc.jpg")
        );
        assert_eq!(
            photo_basename("/mnt/d/Barangay/barangay-id-system/data/photos/abc.jpg").as_deref(),
            Some("abc.jpg")
        );
        assert!(photo_basename("").is_none());
        assert!(photo_basename("   ").is_none());
        assert_eq!(photo_basename("../evil.jpg").as_deref(), Some("evil.jpg")); // traversal neutralized: basename is contained to photos dir
        assert!(photo_basename("..").is_none());
    }

    #[test]
    fn empty_filename_resolves_to_none_and_base64_falls_back() {
        let dir = data_dir();
        assert!(resolve_photo_bytes("", &dir).is_none());
        assert!(get_photo_base64("", &dir).is_none());
        assert!(resolve_photo_bytes("no-such-file-xyz.jpg", &dir).is_none());
    }

    #[test]
    fn exif_orientation_parses_app1_and_defaults_to_normal() {
        // Minimal JPEG: SOI + APP1 Exif (LE, orientation=6) + EOI.
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x22];
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]); // II, 42, IFD0@8
        jpeg.extend_from_slice(&[0x01, 0x00]); // 1 entry
        jpeg.extend_from_slice(&[
            0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00,
        ]); // tag 0x0112 = 6
        jpeg.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // next IFD
        jpeg.extend_from_slice(&[0xFF, 0xD9]);
        assert_eq!(jpeg_exif_orientation(&jpeg), 6);
        assert_eq!(jpeg_exif_orientation(&[0xFF, 0xD8, 0xFF, 0xD9]), 1); // no EXIF
        assert_eq!(jpeg_exif_orientation(b"not a jpeg"), 1);
        assert_eq!(jpeg_exif_orientation(&[]), 1);
    }

    #[test]
    fn orientation_normalization_rotates_dimensions() {
        let img = image::DynamicImage::new_rgb8(4, 2);
        let upright = normalize_orientation(img, 1);
        assert_eq!((upright.width(), upright.height()), (4, 2));
        let rotated = normalize_orientation(image::DynamicImage::new_rgb8(4, 2), 6);
        assert_eq!((rotated.width(), rotated.height()), (2, 4));
        let flipped = normalize_orientation(image::DynamicImage::new_rgb8(4, 2), 3);
        assert_eq!((flipped.width(), flipped.height()), (4, 2));
    }

    #[test]
    fn oriented_decode_roundtrips_plain_jpeg() {
        let img = image::DynamicImage::new_rgb8(6, 4);
        let mut buf = Vec::new();
        let mut cur = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cur, image::ImageFormat::Jpeg).unwrap();
        let back = load_photo_oriented(&buf).unwrap();
        assert_eq!((back.width(), back.height()), (6, 4));
    }

    #[test]
    fn storage_resize_skips_upscale_and_bounds_output() {
        // Small images pass through untouched (no upscale blur / wasted CPU).
        let small = image::DynamicImage::new_rgb8(400, 500);
        let out = storage_resize(small);
        assert_eq!((out.width(), out.height()), (400, 500));
        // Large phone photo is bounded to the 600x800 box.
        let big = image::DynamicImage::new_rgb8(4000, 3000);
        let out = storage_resize(big);
        assert!(out.width() <= 600 && out.height() <= 800);
    }

    #[test]
    fn perf_save001_phone_photo_ingest_timing() {
        // Perf-save-001 timing harness: synthetic 4000x3000 (~12MP phone)
        // JPEG through the full ingest pipeline. Prints elapsed so
        // before/after runs can be compared with --nocapture. The 1500ms
        // budget is generous (old Lanczos3-full-res path cost seconds);
        // Triangle + no-upscale must stay comfortably inside it.
        let big = image::DynamicImage::new_rgb8(4000, 3000);
        let mut buf = Vec::new();
        {
            let mut cur = std::io::Cursor::new(&mut buf);
            big.write_to(&mut cur, image::ImageFormat::Jpeg).unwrap();
        }
        let b64 = format!("data:image/jpeg;base64,{}", BASE64.encode(&buf));
        let t0 = std::time::Instant::now();
        let (name, jpeg) = process_photo_to_jpeg(&b64).unwrap();
        let dt = t0.elapsed();
        eprintln!(
            "perf-save-001 ingest 4000x3000: {:?}, out {} bytes ({})",
            dt,
            jpeg.len(),
            name
        );
        assert!(name.ends_with(".jpg"));
        let back = image::load_from_memory(&jpeg).unwrap();
        assert!(back.width() <= 600 && back.height() <= 800);
        // Timing budget enforced in optimized builds only: `cargo test`
        // runs unoptimized, where even the fast path costs seconds on 12MP
        // (see harness: debug OLD ~19.5s vs NEW ~12.3s; release 780ms vs
        // 525ms). Debug prints the number for eyeballing instead.
        #[cfg(not(debug_assertions))]
        assert!(dt.as_millis() < 1500, "photo ingest regressed: {:?}", dt);
    }
}

#[cfg(test)]
mod validity_tests {
    use super::*;

    #[test]
    fn valid_until_normalizes_all_formats_to_iso() {
        assert_eq!(normalize_valid_until("2027-01-15").unwrap(), "2027-01-15");
        assert_eq!(normalize_valid_until("15-01-2027").unwrap(), "2027-01-15");
        assert_eq!(normalize_valid_until("15/01/2027").unwrap(), "2027-01-15");
        assert_eq!(
            normalize_valid_until("  01-12-2026  ").unwrap(),
            "2026-12-01"
        );
        assert!(normalize_valid_until("").is_err());
        assert!(normalize_valid_until("not-a-date").is_err());
        assert!(normalize_valid_until("32-13-2027").is_err());
    }

    fn base_resident() -> Resident {
        Resident {
            id: Some(1),
            resident_id: "BRGY-2026-00001".to_string(),
            last_name: "DELA CRUZ".to_string(),
            first_name: "JUAN".to_string(),
            middle_name: None,
            suffix: None,
            birth_date: "1990-05-20".to_string(),
            birth_place: None,
            gender: "Male".to_string(),
            civil_status: None,
            citizenship: None,
            occupation: None,
            contact_number: Some("09171234567".to_string()),
            address_street: None,
            address_purok: Some("AVOCADO".to_string()),
            address_barangay: None,
            address_city: None,
            address_province: None,
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
            emergency_contact_address: None,
            photo_filename: None,
            is_active: Some(1),
            date_registered: None,
            date_updated: None,
            remarks: None,
            valid_until: None,
            last_pdf_path: None,
            last_pdf_generated_at: None,
        }
    }

    #[test]
    fn ph_contact_normalizes_all_forms_to_display() {
        assert_eq!(
            normalize_ph_contact("09171234567").unwrap(),
            "0917-123-4567"
        );
        assert_eq!(
            normalize_ph_contact("0917-123-4567").unwrap(),
            "0917-123-4567"
        );
        assert_eq!(
            normalize_ph_contact("0917 123 4567").unwrap(),
            "0917-123-4567"
        );
        assert_eq!(
            normalize_ph_contact("+639171234567").unwrap(),
            "0917-123-4567"
        );
        assert_eq!(
            normalize_ph_contact("639171234567").unwrap(),
            "0917-123-4567"
        );
        assert_eq!(
            normalize_ph_contact("(0917) 123-4567").unwrap(),
            "0917-123-4567"
        );
        // Non-PH / short / bad values rejected strictly.
        assert!(normalize_ph_contact("1234567").is_err());
        assert!(normalize_ph_contact("08171234567").is_err());
        assert!(normalize_ph_contact("0917123456").is_err());
        assert!(normalize_ph_contact("").is_err());
        assert!(normalize_ph_contact("abcdefghijk").is_err());
        // Lenient display formatter: PH → formatted, legacy → trimmed passthrough.
        assert_eq!(format_contact_for_display("09171234567"), "0917-123-4567");
        assert_eq!(format_contact_for_display("1234567"), "1234567");
        assert_eq!(format_contact_for_display("  "), "");
    }

    #[test]
    fn input_validation_enforces_ph_mobile_and_purok_allowlist() {
        let mut r = base_resident();
        assert!(validate_resident_input(&r).is_ok());
        // PH-looking but malformed → rejected.
        r.contact_number = Some("09171".to_string());
        assert!(validate_resident_input(&r).is_err());
        r.contact_number = Some("+639171234567".to_string());
        assert!(validate_resident_input(&r).is_ok());
        // Legacy landline still accepted (lenient path).
        r.contact_number = Some("0881234567".to_string());
        assert!(validate_resident_input(&r).is_ok());
        r.contact_number = Some("09171234567".to_string());
        // Purok allowlist enforced.
        r.address_purok = Some("rimas".to_string());
        assert!(validate_resident_input(&r).is_ok()); // case-insensitive
        r.address_purok = Some("NOWHERE".to_string());
        assert!(validate_resident_input(&r).is_err());
        r.address_purok = Some("AVOCADO".to_string());
        // Emergency name pattern + phone rule.
        r.emergency_contact_name = Some("A".to_string());
        assert!(validate_resident_input(&r).is_err());
        r.emergency_contact_name = Some("MARIA DELA CRUZ".to_string());
        assert!(validate_resident_input(&r).is_ok());
        r.emergency_contact_number = Some("09181".to_string());
        assert!(validate_resident_input(&r).is_err());
        r.emergency_contact_number = Some("09181234567".to_string());
        assert!(validate_resident_input(&r).is_ok());
        // BRGY- ID pattern skips phone validation (FK existence checked at create/update).
        r.emergency_contact_number = Some("BRGY-2026-00002".to_string());
        assert!(validate_resident_input(&r).is_ok());
    }

    #[test]
    fn completeness_gate_rejects_gaps_and_future_dob() {
        let mut r = base_resident();
        assert!(validate_resident_complete(&r).is_ok());
        r.first_name = "  ".to_string();
        let e = validate_resident_complete(&r).unwrap_err();
        assert!(e.starts_with("Incomplete record"), "got: {}", e);
        r.first_name = "JUAN".to_string();
        // Future DOB rejected at PDF time too (not just at input).
        r.birth_date = "2099-09-10".to_string();
        let e = validate_resident_complete(&r).unwrap_err();
        assert!(e.contains("birth date"), "got: {}", e);
        r.birth_date = "1990-05-20".to_string();
        r.address_purok = None;
        assert!(validate_resident_complete(&r).is_err());
        r.address_purok = Some("BUBUNAO".to_string());
        assert!(validate_resident_complete(&r).is_ok());
    }

    #[test]
    fn normalize_birth_date_rejects_120_plus_and_future() {
        let today = chrono::Local::now().date_naive();
        let oldest_allowed = today - chrono::Duration::days(365 * 120);
        // 120 years ago exactly — allowed (edge).
        assert!(normalize_birth_date(&oldest_allowed.format("%Y-%m-%d").to_string()).is_ok());
        // 120 years and 1 day ago — rejected.
        let too_old = oldest_allowed - chrono::Duration::days(1);
        let e = normalize_birth_date(&too_old.format("%Y-%m-%d").to_string()).unwrap_err();
        assert!(e.contains("120 years"), "got: {}", e);
        // Future date — rejected.
        let future = today + chrono::Duration::days(1);
        let e = normalize_birth_date(&future.format("%Y-%m-%d").to_string()).unwrap_err();
        assert!(e.contains("future"), "got: {}", e);
        // Normal valid date — accepted.
        assert_eq!(normalize_birth_date("1990-05-20").unwrap(), "1990-05-20");
        assert_eq!(normalize_birth_date("20-05-1990").unwrap(), "1990-05-20");
        assert_eq!(normalize_birth_date("20/05/1990").unwrap(), "1990-05-20");
        // Empty / invalid — rejected.
        assert!(normalize_birth_date("").is_err());
        assert!(normalize_birth_date("not-a-date").is_err());
    }

    #[test]
    fn digit_search_pattern_drops_out_name_only_queries() {
        // D5: a digit-free (name-only) query must bind NULL, not "%%" — "%%"
        // would match every non-NULL contact and leak the whole table.
        assert_eq!(digit_search_pattern("Maria"), None);
        assert_eq!(digit_search_pattern(""), None);
        // P1-2: formatted + 63-folded numbers still match the stored digits.
        assert_eq!(
            digit_search_pattern("0917-123-4567").as_deref(),
            Some("%09171234567%")
        );
        assert_eq!(
            digit_search_pattern("09171234567").as_deref(),
            Some("%09171234567%")
        );
        assert_eq!(
            digit_search_pattern("+639171234567").as_deref(),
            Some("%09171234567%")
        );
    }
}
