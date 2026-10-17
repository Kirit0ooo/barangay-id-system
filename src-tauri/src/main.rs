//! Print flow is HTML-only (card-template.html → browser print on CR100
//! 98.6x66.8mm). No PDF generation exists in this binary; "Save as PDF"
//! is via the browser print dialog. Documents/PDF is legacy (never created).
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod backup;
mod config;
mod crypto;
mod db;
mod encryption;
mod export;
mod id_card;
mod logger;
mod residents;
mod scheduled;

use db::AppState;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tauri::command]
fn log_frontend(level: String, message: String) -> Result<(), String> {
    logger::append_frontend_log(&level, &message);
    Ok(())
}

#[tauri::command]
fn get_logs_dir() -> Result<String, String> {
    Ok(logger::get_logs_dir()?.to_string_lossy().to_string())
}

/// Legacy shim: Documents/PDF is no longer created or used (HTML-only print).
/// Kept as a registered command so old frontends fail soft with a message
/// instead of "unknown command"; returns an explanatory error.
#[tauri::command]
fn get_documents_pdf_dir() -> Result<String, String> {
    Err("PDF export removed: use Preview → Print (CR100), or Save as PDF via the browser print dialog".to_string())
}

#[tauri::command]
fn get_documents_excel_dir() -> Result<String, String> {
    Ok(db::get_documents_excel_dir().to_string_lossy().to_string())
}

#[tauri::command]
fn get_documents_dir() -> Result<String, String> {
    Ok(db::get_documents_dir().to_string_lossy().to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Pin OS temp dirs to project-local tmp/ per isolation mandate
    if let Some(root) = db::get_project_root() {
        let tmp = root.join("tmp");
        let _ = std::fs::create_dir_all(&tmp);
        std::env::set_var("TMP", &tmp);
        std::env::set_var("TEMP", &tmp);
        std::env::set_var("TMPDIR", &tmp);
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", tmp.join("webview2"));
        let _ = std::fs::create_dir_all(tmp.join("webview2"));
    }
    // Initialize isolated file logging - guard must be kept alive
    let _guard = logger::init();
    tracing::info!("Barangay ID System starting...");

    // Initialize database — decrypt encrypted DB first if present.
    let app_data_dir = db::get_app_data_dir();
    let db_path = app_data_dir.join("residents.db");
    let db_enc_path = app_data_dir.join("residents.db.enc");
    if db_enc_path.exists() && !db_path.exists() {
        tracing::info!("Encrypted database found at startup — decrypting before init_db");
        crypto::decrypt_database_files(&db_path)
            .map_err(|e| format!("Failed to decrypt database at startup: {e}"))?;
    }
    let pool = db::init_db().await?;
    // Ensure Documents/Excel exists (isolated). Documents/PDF is legacy:
    // never created; print is HTML-only, "Save as PDF" via browser dialog.
    let docs_excel = db::get_documents_excel_dir();
    tracing::info!("Data dir: {}", app_data_dir.display());
    // P2-9: log the fallback instead of panicking when unresolvable.
    match logger::get_logs_dir() {
        Ok(d) => tracing::info!("Logs dir: {}", d.display()),
        Err(e) => tracing::error!("Logs dir unresolved ({}), file logging degraded", e),
    }
    tracing::info!("Documents Excel dir: {}", docs_excel.display());
    let state = Arc::new(Mutex::new(AppState { pool, app_data_dir }));

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            // Resident operations
            residents::create_resident,
            residents::get_resident,
            residents::update_resident,
            residents::delete_resident,
            residents::search_residents,
            residents::get_all_residents,
            residents::get_residents_count,
            residents::get_dashboard_stats,
            residents::get_recent_residents,
            residents::get_residents_by_purok,
            residents::generate_next_id,
            residents::get_resident_photo,
            // Civic config (F15) + ID-card QR for HTML-only print
            config::get_civic_config,
            config::set_civic_config,
            config::get_short_barangay,
            config::open_path,
            id_card::get_qr_data_url,
            id_card::get_cards_batch,
            // Backup
            backup::backup_database,
            backup::restore_database,
            // Export
            export::export_to_excel,
            export::import_from_excel,
            scheduled::scheduled_backup,
            scheduled::photo_gc,
            db::optimize_database_cmd,
            // Encryption at rest
            encryption::encryption_generate_key,
            encryption::encryption_status,
            encryption::encryption_encrypt_database,
            encryption::encryption_decrypt_database,
            encryption::encryption_encrypt_photos,
            encryption::encryption_decrypt_photos,
            // Logging
            log_frontend,
            get_logs_dir,
            get_documents_pdf_dir,
            get_documents_excel_dir,
            get_documents_dir,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            // F6: graceful fail-closed — log + exit(1) instead of panic stack.
            tracing::error!("Tauri runtime failed: {}", e);
            eprintln!("Tauri runtime failed: {}", e);
            std::process::exit(1);
        });

    Ok(())
}
