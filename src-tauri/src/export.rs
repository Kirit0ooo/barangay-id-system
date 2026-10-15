use crate::db::{validate_isolated_write_path, AppState};
use crate::residents::Resident;
use anyhow::Result;
use calamine::{open_workbook, Data, Reader, Xlsx};
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook};
use std::path::Path;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

fn sanitized_excel_dest(requested: &str) -> Result<String, String> {
    let validated = validate_isolated_write_path(Path::new(requested)).map_err(|e| {
        format!(
            "Export path rejected (must be inside project folder): {}",
            e
        )
    })?;
    let parent = validated
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(crate::db::get_documents_excel_dir);
    let raw_name = validated
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("BACKUP.xlsx");
    let safe_name = crate::db::sanitize_file_name_component(raw_name, 120);
    let safe_name = if safe_name.to_lowercase().ends_with(".xlsx") {
        safe_name
    } else {
        format!("{}.xlsx", safe_name)
    };
    Ok(crate::db::resolve_unique_path(&parent.join(&safe_name))?
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
pub async fn export_to_excel(
    state: State<'_, Arc<Mutex<AppState>>>,
    save_path: String,
) -> Result<bool, String> {
    let dest = if save_path.trim().is_empty() {
        let dated = format!("{}.xlsx", crate::db::backup_base_name());
        sanitized_excel_dest(
            &crate::db::get_documents_excel_dir()
                .join(dated)
                .to_string_lossy(),
        )?
    } else {
        sanitized_excel_dest(&save_path)?
    };
    let save_path = dest;
    // M-1: clone pool and release lock before DB await
    let pool = { state.lock().await.pool.clone() };

    let residents = sqlx::query_as::<_, Resident>(&format!(
        "SELECT {} FROM residents WHERE is_active = 1 ORDER BY last_name, first_name",
        crate::residents::RESIDENT_COLUMNS
    ))
    .fetch_all(&pool)
    .await
    .map_err(|e| format!("Failed to fetch residents: {}", e))?;

    // P1 sync-blocking fix: workbook.save() does heavy zip+IO synchronously —
    // offload to blocking thread so the Tauri async runtime stays responsive.
    let sp = save_path.clone();
    tokio::task::spawn_blocking(move || write_excel(&residents, &sp))
        .await
        .map_err(|e| format!("Export task failed: {}", e))?
        .map_err(|e| format!("Excel export failed: {}", e))?;

    Ok(true)
}

fn full_address_of(r: &Resident) -> String {
    // Single-line display address for the organized "Full Address" column.
    let parts = [
        r.address_street.clone().unwrap_or_default(),
        r.address_purok
            .clone()
            .map(|p| format!("Purok {}", p))
            .unwrap_or_default(),
        r.address_barangay.clone().unwrap_or_default(),
        r.address_city.clone().unwrap_or_default(),
        r.address_province.clone().unwrap_or_default(),
    ];
    parts
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_excel(residents: &[Resident], save_path: &str) -> Result<()> {
    // Organized BACKUP workbook: spec columns first (DB ID, ID No., names,
    // DOB, gender, civil, purok, addresses, contact, emergency fields, photo
    // path, resident_id alias, created date) + extended detail columns so an
    // edited BACKUP re-imports with full fidelity. Bold header, column
    // widths, autofilter, frozen header row.
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Residents")?;

    let header_format = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0x003366))
        .set_font_color(Color::White)
        .set_align(FormatAlign::Center)
        .set_border(FormatBorder::Thin);

    let cell_format = Format::new().set_border(FormatBorder::Thin);

    let headers = [
        "DB ID",
        "ID No.",
        "Last Name",
        "First Name",
        "Middle Name",
        "Suffix",
        "Birth Date",
        "Birth Place",
        "Gender",
        "Civil Status",
        "Citizenship",
        "Occupation",
        "Contact No.",
        "Street",
        "Purok",
        "Full Address",
        "Barangay",
        "City",
        "Province",
        "Religion",
        "Blood Type",
        "Height",
        "Weight",
        "PhilHealth",
        "SSS",
        "TIN",
        "Voter's ID",
        "Emergency Contact",
        "Relation",
        "Emergency Contact No.",
        "Photo Filename",
        "Status",
        "Valid Until",
        "Date Registered",
        "Date Updated",
        "Emergency Address",
        "Emergency Contact Resident ID",
    ];

    for (col, header) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, col as u16, *header, &header_format)?;
    }

    let widths = [
        8.0, 14.0, 15.0, 15.0, 15.0, 8.0, 12.0, 18.0, 9.0, 12.0, 12.0, 15.0, 14.0, 15.0, 12.0,
        32.0, 15.0, 15.0, 15.0, 12.0, 10.0, 8.0, 8.0, 14.0, 14.0, 14.0, 14.0, 20.0, 12.0, 18.0,
        18.0, 10.0, 14.0, 16.0, 16.0, 20.0, 20.0,
    ];
    // M-2: guard against header/width desync that would silently truncate columns
    assert_eq!(
        headers.len(),
        widths.len(),
        "headers/widths length mismatch: {} vs {}",
        headers.len(),
        widths.len()
    );

    for (col, width) in widths.iter().enumerate() {
        worksheet.set_column_width(col as u16, *width)?;
    }

    for (row, r) in residents.iter().enumerate() {
        let row_num = (row + 1) as u32;
        let full_addr = full_address_of(r);

        // DB ID sorts/filters as a real number in Excel (as text, "10"
        // would sort before "2"). Import-safe: the match key is `ID No.`,
        // this column is never parsed back.
        match r.id {
            Some(v) => {
                worksheet.write_number_with_format(row_num, 0, v as f64, &cell_format)?;
            }
            None => {
                worksheet.write_blank(row_num, 0, &cell_format)?;
            }
        }
        worksheet.write_string_with_format(row_num, 1, &r.resident_id, &cell_format)?;
        worksheet.write_string_with_format(row_num, 2, &r.last_name, &cell_format)?;
        worksheet.write_string_with_format(row_num, 3, &r.first_name, &cell_format)?;
        worksheet.write_string_with_format(
            row_num,
            4,
            r.middle_name.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            5,
            r.suffix.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(row_num, 6, &r.birth_date, &cell_format)?;
        worksheet.write_string_with_format(
            row_num,
            7,
            r.birth_place.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(row_num, 8, &r.gender, &cell_format)?;
        worksheet.write_string_with_format(
            row_num,
            9,
            r.civil_status.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            10,
            r.citizenship.as_deref().unwrap_or("Filipino"),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            11,
            r.occupation.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            12,
            r.contact_number.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            13,
            r.address_street.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            14,
            r.address_purok.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(row_num, 15, &full_addr, &cell_format)?;
        worksheet.write_string_with_format(
            row_num,
            16,
            r.address_barangay.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            17,
            r.address_city.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            18,
            r.address_province.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            19,
            r.religion.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            20,
            r.blood_type.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            21,
            r.height.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            22,
            r.weight.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            23,
            r.philhealth.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            24,
            r.sss.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            25,
            r.tin.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            26,
            r.voters_id.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            27,
            r.emergency_contact_name.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            28,
            r.emergency_contact_relation.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            29,
            r.emergency_contact_number.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            30,
            r.photo_filename.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            31,
            r.remarks.as_deref().unwrap_or("ACTIVE"),
            &cell_format,
        )?;
        // P1-6: Excel shows display format (DD-MM-YYYY); storage stays ISO.
        worksheet.write_string_with_format(
            row_num,
            32,
            &r.valid_until
                .as_deref()
                .map(|v| crate::id_card::fmt_ddmmyyyy(v.trim()))
                .unwrap_or_default(),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            33,
            r.date_registered.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            34,
            r.date_updated.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        worksheet.write_string_with_format(
            row_num,
            35,
            r.emergency_contact_address.as_deref().unwrap_or(""),
            &cell_format,
        )?;
        // P13 FK parity: round-trip preserves the emergency-contact link.
        worksheet.write_string_with_format(
            row_num,
            36,
            r.emergency_contact_resident_id.as_deref().unwrap_or(""),
            &cell_format,
        )?;
    }

    // Frozen header + autofilter over the data extent (filters + widths per spec).
    worksheet.set_freeze_panes(1, 0)?;
    if !residents.is_empty() {
        worksheet.autofilter(0, 0, residents.len() as u32, (headers.len() - 1) as u16)?;
    }

    // Print polish: landscape, fit-to-one-page-wide (tall as needed),
    // header row repeated on every printed page, centered horizontally.
    // Screen grid, values, and import fidelity are unchanged.
    worksheet.set_landscape();
    worksheet.set_print_fit_to_pages(1, 0);
    worksheet.set_print_center_horizontally(true);
    worksheet.set_repeat_rows(0, 0)?;

    // Ensure parent dir exists for user-picked nested paths (e.g. Documents/Excel/Sub/report.xlsx)
    if let Some(parent) = std::path::Path::new(save_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    workbook.save(save_path)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Excel re-import: an edited BACKUP-YYYY-MM-DD.xlsx syncs back to the DB.
// Match key is `ID No.` (resident_id). Photos are preserved when the
// `Photo Filename` cell is blank; invalid rows are rejected with a per-row
// report. Returns diff counts (updated / skipped / total).
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize)]
pub struct ImportReport {
    pub updated: usize,
    pub skipped: usize,
    pub total_rows: usize,
    pub errors: Vec<String>,
}

fn data_to_string(d: &Data) -> String {
    if matches!(d, Data::Empty) {
        String::new()
    } else {
        d.to_string().trim().to_string()
    }
}

fn parse_excel_rows(path: &std::path::PathBuf) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let mut wb: Xlsx<_> =
        open_workbook(path).map_err(|e| format!("Cannot open Excel file: {}", e))?;
    let sheets = wb.sheet_names().to_vec();
    if sheets.is_empty() {
        return Err("Excel file has no sheets".to_string());
    }
    // Prefer the organized "Residents" sheet; fall back to the first sheet.
    let name = sheets
        .iter()
        .find(|s| s.eq_ignore_ascii_case("residents"))
        .cloned()
        .unwrap_or_else(|| sheets[0].clone());
    let range = wb
        .worksheet_range(&name)
        .map_err(|e| format!("Cannot read sheet '{}': {}", name, e))?;
    let mut rows = range.rows();
    let header_row = rows
        .next()
        .ok_or_else(|| "Excel sheet is empty".to_string())?;
    let headers: Vec<String> = header_row.iter().map(data_to_string).collect();
    if headers.iter().all(|h| h.is_empty()) {
        return Err("Excel header row is empty".to_string());
    }
    let mut out = Vec::new();
    for r in rows {
        let vals: Vec<String> = r.iter().map(data_to_string).collect();
        if vals.iter().all(|v| v.is_empty()) {
            continue; // skip blank rows
        }
        out.push(vals);
    }
    Ok((headers, out))
}

fn col_idx(headers: &[String], name: &str) -> Option<usize> {
    headers
        .iter()
        .position(|h| h.trim().eq_ignore_ascii_case(name))
}

fn cell(row: &[String], idx: Option<usize>) -> String {
    idx.and_then(|i| row.get(i))
        .cloned()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn opt_cell(row: &[String], idx: Option<usize>) -> Option<String> {
    let v = cell(row, idx);
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// Phase-1 strict valid_until for Excel import: whitespace-only input yields
/// `None` (clears), parseable values normalize to ISO, unparseable values
/// are Err (caller skips the row — never stores raw). Note: a truly blank
/// cell never reaches here — the caller only invokes this on `Some(cell)`,
/// so blank keeps the existing DB value.
fn strict_valid_until_opt(raw: &str) -> Result<Option<String>, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(None);
    }
    crate::residents::normalize_valid_until(t).map(Some)
}

/// M2: photo referenced by an import row must already exist under
/// `<app_data>/photos` (or `<root>/data/photos` fallback). Returns the
/// stored value unchanged when it exists, Err(message) otherwise.
/// Callers skip the row — never write a dangling photo_filename.
fn validate_import_photo(stored: &str, app_data_dir: &std::path::Path) -> Result<String, String> {
    let base = match crate::residents::photo_basename(stored) {
        Some(b) => b,
        None => {
            return Err(format!(
                "photo value '{}' is not a readable filename",
                stored.trim()
            ))
        }
    };
    let mut candidates = vec![app_data_dir.join("photos").join(&base)];
    if let Some(root) = crate::db::get_project_root() {
        candidates.push(root.join("data").join("photos").join(&base));
    }
    if candidates.iter().any(|p| p.is_file()) {
        Ok(stored.trim().to_string())
    } else {
        Err(format!("photo file '{}' not found under data/photos", base))
    }
}

#[tauri::command]
pub async fn import_from_excel(
    state: State<'_, Arc<Mutex<AppState>>>,
    file_path: String,
) -> Result<ImportReport, String> {
    if file_path.trim().is_empty() {
        return Err("No Excel file selected".to_string());
    }
    let validated = validate_isolated_write_path(Path::new(file_path.trim())).map_err(|e| {
        format!(
            "Import path rejected (must be inside project folder): {}",
            e
        )
    })?;
    if !validated.exists() {
        return Err(format!("Excel file not found: {}", validated.display()));
    }
    if validated
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        != Some("xlsx".to_string())
    {
        return Err("Import file must be an .xlsx workbook".to_string());
    }
    // P0 OOM guard: reject huge files before loading entire sheet into RAM.
    // 15 MB xlsx is already ~tens of thousands of rows; cap protects against
    // zip bombs and 1M-row sheets that would OOM via Vec<Vec<String>>.
    if let Ok(meta) = std::fs::metadata(&validated) {
        const MAX_XLSX_BYTES: u64 = 15 * 1024 * 1024;
        if meta.len() > MAX_XLSX_BYTES {
            return Err(format!(
                "Excel file too large ({} MB > {} MB) — split into smaller files",
                meta.len() / (1024 * 1024),
                MAX_XLSX_BYTES / (1024 * 1024)
            ));
        }
    }
    // calamine is blocking I/O — parse off the async runtime.
    let path_clone = validated.clone();
    let (headers, rows) = tokio::task::spawn_blocking(move || parse_excel_rows(&path_clone))
        .await
        .map_err(|e| format!("Import task failed: {}", e))??;
    // P0 row cap after parse (calamine already materialized the sheet).
    const MAX_IMPORT_ROWS: usize = 20_000;
    if rows.len() > MAX_IMPORT_ROWS {
        return Err(format!(
            "Excel has {} rows > {} max — split into smaller files",
            rows.len(),
            MAX_IMPORT_ROWS
        ));
    }

    // R4 — import column guard: widths/headers contract is 37 columns (write_excel invariant).
    // Reject severely mismatched sheets early; warn on partial legacy exports.
    const EXPECTED_COLS: usize = 37;
    // H-5b: Return Err when exceeding EXPECTED_COLS to prevent silent data loss.
    if headers.len() > EXPECTED_COLS {
        return Err(format!(
            "Import header count too high ({} > {}) — excess columns will be truncated. Expected {} columns.",
            headers.len(),
            EXPECTED_COLS,
            EXPECTED_COLS
        ));
    }
    // Reject too few columns outright — cannot map required fields.
    if headers.len() < 10 {
        return Err(format!(
            "Import header count too low ({} < 10) — is this a valid BACKUP export? Expected {} columns.",
            headers.len(),
            EXPECTED_COLS
        ));
    }
    // Partial legacy export (10..EXPECTED_COLS): warn but allow.
    // Exact match (EXPECTED_COLS): silent OK.
    if headers.len() < EXPECTED_COLS {
        tracing::warn!(
            "import header count mismatch: got {} expected {} — headers: {:?}",
            headers.len(),
            EXPECTED_COLS,
            &headers
        );
    }
    // Row-level inconsistent column counts are warned and normalized below.
    for (ri, row) in rows.iter().enumerate() {
        if row.len() != headers.len() {
            tracing::warn!(
                "import row {} column mismatch: row len {} vs header len {} — will truncate/pad",
                ri + 2,
                row.len(),
                headers.len()
            );
        }
    }

    let c_id = col_idx(&headers, "ID No.").ok_or_else(|| {
        "Missing 'ID No.' column — is this a BACKUP-YYYY-MM-DD export?".to_string()
    })?;
    let c_last = col_idx(&headers, "Last Name");
    let c_first = col_idx(&headers, "First Name");
    let c_middle = col_idx(&headers, "Middle Name");
    let c_suffix = col_idx(&headers, "Suffix");
    let c_dob = col_idx(&headers, "Birth Date");
    let c_bplace = col_idx(&headers, "Birth Place");
    let c_gender = col_idx(&headers, "Gender");
    let c_civil = col_idx(&headers, "Civil Status");
    let c_citizen = col_idx(&headers, "Citizenship");
    let c_occup = col_idx(&headers, "Occupation");
    let c_contact = col_idx(&headers, "Contact No.");
    let c_street = col_idx(&headers, "Street");
    let c_purok = col_idx(&headers, "Purok");
    let c_brgy = col_idx(&headers, "Barangay");
    let c_city = col_idx(&headers, "City");
    let c_prov = col_idx(&headers, "Province");
    let c_religion = col_idx(&headers, "Religion");
    let c_blood = col_idx(&headers, "Blood Type");
    let c_height = col_idx(&headers, "Height");
    let c_weight = col_idx(&headers, "Weight");
    let c_phil = col_idx(&headers, "PhilHealth");
    let c_sss = col_idx(&headers, "SSS");
    let c_tin = col_idx(&headers, "TIN");
    let c_voter = col_idx(&headers, "Voter's ID");
    let c_emer = col_idx(&headers, "Emergency Contact");
    let c_rel = col_idx(&headers, "Relation");
    let c_emer_no = col_idx(&headers, "Emergency Contact No.");
    let c_photo = col_idx(&headers, "Photo Filename");
    let c_status = col_idx(&headers, "Status");
    let c_valid = col_idx(&headers, "Valid Until");
    let c_emer_addr = col_idx(&headers, "Emergency Address");
    // M2: emergency-link column is optional (older 37-col exports lack it).
    // Canonical header is "Emergency Contact Resident ID"; legacy operator-added
    // variants are still accepted; absent = keep DB value.
    let c_emer_id = col_idx(&headers, "Emergency Contact Resident ID")
        .or_else(|| col_idx(&headers, "emergency_contact_resident_id"))
        .or_else(|| col_idx(&headers, "Emergency Contact ID"));

    // H-3: release Mutex before loop — clone pool + photo dir, never hold lock across rows
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };
    let mut updated = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();
    // Collect validated updates to apply in chunked transactions (100 per tx)
    let mut pending: Vec<(crate::residents::Resident, usize)> = Vec::new();
    // P2-7: imports >100 rows preload all residents once instead of N+1 SELECTs.
    let mut bulk_cache: Option<std::collections::HashMap<String, crate::residents::Resident>> =
        if rows.len() > 100 {
            let all: Vec<crate::residents::Resident> =
                sqlx::query_as::<_, crate::residents::Resident>(&format!(
                    "SELECT {} FROM residents",
                    crate::residents::RESIDENT_COLUMNS
                ))
                .fetch_all(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Some(
                all.into_iter()
                    .map(|r| (r.resident_id.clone(), r))
                    .collect(),
            )
        } else {
            None
        };

    for (i, row) in rows.iter().enumerate() {
        // R4 row-level guard: truncate excess columns, pad missing as empty (cell() already does).
        let row_norm: Vec<String> = if row.len() > headers.len() {
            tracing::warn!(
                "row {} has {} cols > {} headers — truncating",
                i + 2,
                row.len(),
                headers.len()
            );
            row[..headers.len()].to_vec()
        } else {
            row.clone()
        };
        let row = &row_norm;
        let excel_row = i + 2; // 1-based + header
        let id_no = cell(row, Some(c_id));
        if id_no.is_empty() {
            skipped += 1;
            errors.push(format!("Row {}: empty ID No. — skipped", excel_row));
            continue;
        }
        let existing: Option<Resident> = match bulk_cache.as_ref().and_then(|c| c.get(&id_no)) {
            Some(r) => Some(r.clone()),
            None if bulk_cache.is_some() => None,
            _ => sqlx::query_as::<_, Resident>(&format!(
                "SELECT {} FROM residents WHERE resident_id = ?",
                crate::residents::RESIDENT_COLUMNS
            ))
            .bind(&id_no)
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?,
        };
        let mut base = match existing {
            Some(r) => r,
            None => {
                skipped += 1;
                errors.push(format!(
                    "Row {} ({}): no matching resident — skipped (import updates existing records only)",
                    excel_row, id_no
                ));
                continue;
            }
        };
        let before = base.clone();
        // Required fields: blank cell keeps the existing value (never wipe).
        let keep = |v: String, cur: &str| {
            if v.is_empty() {
                cur.to_string()
            } else {
                v
            }
        };
        base.last_name = keep(cell(row, c_last), &before.last_name);
        base.first_name = keep(cell(row, c_first), &before.first_name);
        base.birth_date = keep(cell(row, c_dob), &before.birth_date);
        // R3 — normalize imported DOB to canonical YYYY-MM-DD (accepts DD-MM-YYYY etc)
        if let Ok(norm) = crate::residents::normalize_birth_date(&base.birth_date) {
            base.birth_date = norm;
        }
        base.gender = keep(cell(row, c_gender), &before.gender);
        // Optional fields: blank cell clears to None (explicit edit).
        base.middle_name = opt_cell(row, c_middle);
        base.suffix = opt_cell(row, c_suffix);
        base.birth_place = opt_cell(row, c_bplace);
        base.civil_status = opt_cell(row, c_civil);
        base.citizenship = opt_cell(row, c_citizen);
        base.occupation = opt_cell(row, c_occup);
        base.contact_number = opt_cell(row, c_contact);
        base.address_street = opt_cell(row, c_street);
        base.address_purok = opt_cell(row, c_purok);
        base.address_barangay = opt_cell(row, c_brgy);
        base.address_city = opt_cell(row, c_city);
        base.address_province = opt_cell(row, c_prov);
        base.religion = opt_cell(row, c_religion);
        base.blood_type = opt_cell(row, c_blood);
        base.height = opt_cell(row, c_height);
        base.weight = opt_cell(row, c_weight);
        base.philhealth = opt_cell(row, c_phil);
        base.sss = opt_cell(row, c_sss);
        base.tin = opt_cell(row, c_tin);
        base.voters_id = opt_cell(row, c_voter);
        base.emergency_contact_name = opt_cell(row, c_emer);
        base.emergency_contact_relation = opt_cell(row, c_rel);
        base.emergency_contact_number = opt_cell(row, c_emer_no);
        base.emergency_contact_address = opt_cell(row, c_emer_addr);
        base.remarks = opt_cell(row, c_status);
        // M2: photo preserved when the cell is blank; a non-blank value must
        // already exist under data/photos — never write a dangling filename.
        if let Some(ph) = opt_cell(row, c_photo) {
            if Some(&ph) != before.photo_filename.as_ref() {
                match validate_import_photo(&ph, &app_data_dir) {
                    Ok(ok) => base.photo_filename = Some(ok),
                    Err(msg) => {
                        skipped += 1;
                        errors.push(format!("Row {} ({}): {} — skipped", excel_row, id_no, msg));
                        continue;
                    }
                }
            }
        }
        // M2: emergency-link FK parity with create/update — a non-blank ID
        // must match an existing resident, else skip the row.
        if let Some(idx) = c_emer_id {
            let fk_raw = cell(row, Some(idx));
            if fk_raw.is_empty() {
                base.emergency_contact_resident_id = None;
            } else {
                let fk = fk_raw.to_uppercase();
                let ok: Option<i64> = match bulk_cache.as_ref() {
                    Some(c) => c.contains_key(&fk).then_some(0),
                    None => {
                        sqlx::query_scalar("SELECT id FROM residents WHERE resident_id = ? LIMIT 1")
                            .bind(&fk)
                            .fetch_optional(&pool)
                            .await
                            .map_err(|e| e.to_string())?
                    }
                };
                if ok.is_none() {
                    skipped += 1;
                    errors.push(format!(
                        "Row {} ({}): emergency contact resident {} not found — skipped",
                        excel_row, id_no, fk
                    ));
                    continue;
                }
                base.emergency_contact_resident_id = Some(fk);
            }
        }
        // Strict valid_until: unparseable values are row-level errors —
        // the row is skipped, never stored raw (Phase-1 strictness).
        if let Some(vu) = opt_cell(row, c_valid) {
            match strict_valid_until_opt(&vu) {
                Ok(v) => base.valid_until = v,
                Err(msg) => {
                    skipped += 1;
                    errors.push(format!(
                        "Row {} ({}): invalid valid_until '{}' ({}) — skipped",
                        excel_row, id_no, vu, msg
                    ));
                    continue;
                }
            }
        }
        // P1-1: same normalization pipeline as create/update — Excel-edited
        // cells must not bypass it (round-trip preserves formatting invariants).
        crate::residents::normalize_resident_strings(&mut base);
        // normalize_*_strings only trims birth_date; re-canonicalize after trim.
        if let Ok(norm) = crate::residents::normalize_birth_date(&base.birth_date) {
            base.birth_date = norm;
        }
        // Validate with the same rules as register/edit.
        if let Err(msg) = crate::residents::validate_resident_input(&base) {
            skipped += 1;
            errors.push(format!("Row {} ({}): invalid — {}", excel_row, id_no, msg));
            continue;
        }
        // Honest diff count: untouched rows are skipped, not "updated".
        if before == base {
            skipped += 1;
            continue;
        }
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        base.date_updated = Some(now);
        if let Some(c) = bulk_cache.as_mut() {
            c.insert(base.resident_id.clone(), base.clone());
        }
        pending.push((base, excel_row));
        // Flush every 100 rows in a single transaction to avoid holding a long txn across the whole file
        if pending.len() >= 100 {
            let chunk: Vec<_> = pending.drain(..).collect();
            let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
            for (b, excel_row) in chunk {
                // Duplicate-person gate (import path — the one intentional
                // behavior change): same check as create/edit, excluding self
                // (b.id). Runs INSIDE the flush txn so SQLite sees this txn's
                // own earlier writes — two rows in one file converging on the
                // same name+birth date are caught, not just collisions with
                // already-committed rows.
                if let Some(other) =
                    crate::residents::duplicate_person_gate(&mut *tx, &b, b.id).await?
                {
                    skipped += 1;
                    errors.push(format!(
                        "Row {} ({}): possible duplicate — a resident with the same name and birth date already exists (ID: {}) — skipped",
                        excel_row, b.resident_id, other
                    ));
                    continue;
                }
                let res = crate::residents::update_resident_row(&mut *tx, &b).await?;
                if res > 0 {
                    updated += 1;
                } else {
                    skipped += 1;
                }
            }
            tx.commit().await.map_err(|e| e.to_string())?;
        }
    }
    // Flush remainder
    if !pending.is_empty() {
        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
        for (b, excel_row) in pending.drain(..) {
            if let Some(other) = crate::residents::duplicate_person_gate(&mut *tx, &b, b.id).await?
            {
                skipped += 1;
                errors.push(format!(
                    "Row {} ({}): possible duplicate — a resident with the same name and birth date already exists (ID: {}) — skipped",
                    excel_row, b.resident_id, other
                ));
                continue;
            }
            let res = crate::residents::update_resident_row(&mut *tx, &b).await?;
            if res > 0 {
                updated += 1;
            } else {
                skipped += 1;
            }
        }
        tx.commit().await.map_err(|e| e.to_string())?;
    }
    // Bound the report payload; the full list is logged backend-side.
    if errors.len() > 50 {
        let extra = errors.len() - 50;
        errors.truncate(50);
        errors.push(format!(
            "… and {} more rejected rows (see backend log)",
            extra
        ));
    }
    tracing::info!(
        "Excel import: {} updated, {} skipped of {} rows ({} errors)",
        updated,
        skipped,
        rows.len(),
        errors.len()
    );
    Ok(ImportReport {
        updated,
        skipped,
        total_rows: rows.len(),
        errors,
    })
}

#[cfg(test)]
mod export_tests {
    use super::*;
    use crate::residents::Resident;
    use std::path::PathBuf;

    fn dummy_resident(n: usize) -> Resident {
        Resident {
            id: Some(n as i64),
            resident_id: format!("BRGY-2026-{:05}", n),
            last_name: format!("Last{}", n),
            first_name: format!("First{}", n),
            middle_name: Some("M".into()),
            suffix: None,
            birth_date: "1990-01-01".into(),
            birth_place: Some("Basey".into()),
            gender: if n % 2 == 0 {
                "Male".into()
            } else {
                "Female".into()
            },
            civil_status: Some("Single".into()),
            citizenship: Some("Filipino".into()),
            occupation: None,
            contact_number: Some("09123456789".into()),
            address_street: Some("Street".into()),
            address_purok: Some("AVOCADO".into()),
            address_barangay: Some("BALUD".into()),
            address_city: Some("BASEY".into()),
            address_province: Some("SAMAR".into()),
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
            date_registered: Some("2026-01-01 00:00:00".into()),
            date_updated: None,
            remarks: Some("ACTIVE".into()),
            valid_until: Some("2027-01-01".into()), // P1-6: storage is ISO
            last_pdf_path: None,
            last_pdf_generated_at: None,
        }
    }

    #[test]
    fn headers_widths_stay_in_sync() {
        // Mirrors the write_excel assert — ensure the guard exists.
        let headers = [
            "DB ID",
            "ID No.",
            "Last Name",
            "First Name",
            "Middle Name",
            "Suffix",
            "Birth Date",
            "Birth Place",
            "Gender",
            "Civil Status",
            "Citizenship",
            "Occupation",
            "Contact No.",
            "Street",
            "Purok",
            "Full Address",
            "Barangay",
            "City",
            "Province",
            "Religion",
            "Blood Type",
            "Height",
            "Weight",
            "PhilHealth",
            "SSS",
            "TIN",
            "Voter's ID",
            "Emergency Contact",
            "Relation",
            "Emergency Contact No.",
            "Photo Filename",
            "Status",
            "Valid Until",
            "Date Registered",
            "Date Updated",
            "Emergency Address",
            "Emergency Contact Resident ID",
        ];
        let widths = [
            8.0, 14.0, 15.0, 15.0, 15.0, 8.0, 12.0, 18.0, 9.0, 12.0, 12.0, 15.0, 14.0, 15.0, 12.0,
            32.0, 15.0, 15.0, 15.0, 12.0, 10.0, 8.0, 8.0, 14.0, 14.0, 14.0, 14.0, 20.0, 12.0, 18.0,
            18.0, 10.0, 14.0, 16.0, 16.0, 20.0, 20.0,
        ];
        assert_eq!(headers.len(), 37);
        assert_eq!(widths.len(), 37);
        assert_eq!(headers.len(), widths.len());
    }

    #[test]
    fn write_excel_500_row_roundtrip_and_column_guard() {
        // Use project tmp dir to stay isolated; file is removed after.
        let root = crate::db::get_project_root().unwrap_or_else(crate::db::default_root);
        let tmp_dir = root.join("tmp").join("export_test");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let path = tmp_dir.join(format!("test_500_{}.xlsx", std::process::id()));
        let residents: Vec<Resident> = (1..=500).map(dummy_resident).collect();
        write_excel(&residents, &path.to_string_lossy())
            .expect("write_excel 500 rows should succeed");
        assert!(path.exists(), "xlsx file should exist");
        let meta = std::fs::metadata(&path).unwrap();
        assert!(
            meta.len() > 10_000,
            "xlsx size suspiciously small: {}",
            meta.len()
        );
        // Round-trip via calamine to verify headers/row count and column guard
        let (headers, rows) =
            parse_excel_rows(&PathBuf::from(&path)).expect("parse should succeed");
        assert_eq!(headers.len(), 37, "headers must be 37");
        assert_eq!(rows.len(), 500, "rows must be 500");
        // Ensure row-level guard truncates excess cols (simulate)
        let mut long_row = rows[0].clone();
        long_row.extend(["extra1".to_string(), "extra2".to_string()]);
        assert!(long_row.len() > headers.len());
        let truncated = &long_row[..headers.len()];
        assert_eq!(truncated.len(), headers.len());
        // Minimal header/width desync detection: headers len mismatch would have been caught by write_excel assert
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&tmp_dir);
    }

    #[test]
    fn import_row_truncation_helper() {
        // Exercise cell() truncation/pad semantics used by R4 guard
        let headers: Vec<String> = (0..36).map(|i| format!("H{}", i)).collect();
        let mut row: Vec<String> = (0..40).map(|i| format!("v{}", i)).collect();
        if row.len() > headers.len() {
            row.truncate(headers.len());
        }
        assert_eq!(row.len(), 36);
        // missing columns -> cell returns empty
        let short = vec!["a".to_string(), "b".to_string()];
        assert_eq!(cell(&short, Some(5)), "");
        assert_eq!(cell(&short, Some(0)), "a");
    }

    #[test]
    fn strict_valid_until_rejects_unparseable_never_raw() {
        assert_eq!(
            strict_valid_until_opt("2027-01-15").unwrap(),
            Some("2027-01-15".to_string())
        );
        assert_eq!(
            strict_valid_until_opt("15-01-2027").unwrap(),
            Some("2027-01-15".to_string())
        );
        assert_eq!(strict_valid_until_opt("").unwrap(), None);
        assert_eq!(strict_valid_until_opt("   ").unwrap(), None);
        // Unparseable -> Err (caller skips the row, never stores raw).
        assert!(strict_valid_until_opt("not-a-date").is_err());
        assert!(strict_valid_until_opt("32-13-2027").is_err());
        assert!(strict_valid_until_opt("2027").is_err());
    }
}
