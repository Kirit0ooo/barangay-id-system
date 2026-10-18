//! Tauri commands for encryption at rest.
//!
//! All commands are async and take `State<'_, Arc<Mutex<AppState>>>` so they
//! resolve `app_data_dir` from the running app state — never from
//! `get_project_root()` (which can differ under MSI perMachine installs).
//!
//! Naming convention (used by all five call sites):
//!   Plaintext:   data/residents.db  +  data/residents.db-wal + -shm
//!   Encrypted:   data/residents.db.enc  (WAL/SHM sidecars deleted, not
//!                encrypted — SQLite validates WAL magic on open)
//! Encrypt: rename db→db.enc, encrypt that file in place, delete sidecars,
//!          then exit so next launch decrypts before init_db.
//! Decrypt: decrypt db.enc in place, rename back to db, delete sidecars,
//!          then reopen pool.

use crate::db::{shm_path_for, wal_path_for, AppState};
use sqlx::query;
use std::fs;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

/// Generate a new 256-bit AES key in `.crypto-key/key.bin`.
/// Fails if a key already exists.
#[tauri::command]
pub async fn encryption_generate_key(
    _state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    // key dir derives from app_data_dir parent — same resolver crypto uses.
    if crate::crypto::key_exists() {
        return Err("encryption key already exists".to_string());
    }
    crate::crypto::generate_key()?;
    Ok("Encryption key generated at .crypto-key/key.bin".to_string())
}

/// Report encryption status: key exists?, DB encrypted?, photos encrypted?
#[tauri::command]
pub async fn encryption_status(state: State<'_, Arc<Mutex<AppState>>>) -> Result<String, String> {
    let app_data_dir = {
        let s = state.lock().await;
        s.app_data_dir.clone()
    };
    // key dir derives from app_data_dir parent — same resolver crypto uses.
    let key_exists = crate::crypto::key_exists();

    let db_path = app_data_dir.join("residents.db");
    let db_enc_path = db_path.with_extension("db.enc");
    let photos_dir = app_data_dir.join("photos");

    let db_plaintext = db_path.exists();
    let db_encrypted = db_enc_path.exists();

    let mut photo_plain = 0usize;
    let mut photo_enc = 0usize;
    if photos_dir.exists() {
        for entry in std::fs::read_dir(&photos_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_file() {
                let data = std::fs::read(&path).map_err(|e| e.to_string())?;
                if crate::crypto::CryptoKey::is_encrypted(&data) {
                    photo_enc += 1;
                } else {
                    photo_plain += 1;
                }
            }
        }
    }

    Ok(format!(
        "key_exists={key_exists}, db_plaintext={db_plaintext}, db_encrypted={db_encrypted}, photos_plaintext={photo_plain}, photos_encrypted={photo_enc}"
    ))
}

/// Encrypt `data/residents.db` plus WAL/SHM sidecars handling.
/// Renames db→db.enc first, encrypts db.enc in place, deletes sidecars
/// (empty after checkpoint), then exits the process so next launch
/// decrypts before `init_db`.
#[tauri::command]
pub async fn encryption_encrypt_database(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };
    let db_path = app_data_dir.join("residents.db");
    let db_enc_path = db_path.with_extension("db.enc");
    if !db_path.exists() {
        return Err("database file not found".to_string());
    }
    if db_enc_path.exists() {
        return Err("database already encrypted".to_string());
    }
    // Flush WAL to the main DB before rename + encrypt so no uncommitted
    // pages are lost. Then delete sidecars — SQLite validates WAL magic on
    // open, so an encrypted-wal sidecar would make the DB unopenable.
    let _ = query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await;
    let wal = wal_path_for(&db_path);
    let shm = shm_path_for(&db_path);
    let _ = std::fs::remove_file(&wal);
    let _ = std::fs::remove_file(&shm);
    // Close pool before touching files (Windows sharing violation otherwise).
    pool.close().await;
    // Encrypt plaintext db → db.enc.tmp (plaintext db untouched until the
    // ciphertext is fully written). Then rename tmp → db.enc, then remove db.
    // Every failure point leaves either intact plaintext or complete ciphertext.
    let enc_tmp = db_enc_path.with_extension("db.enc.tmp");
    crate::crypto::encrypt_file_to_tmp(&db_path, &enc_tmp)?;
    fs::rename(&enc_tmp, &db_enc_path).map_err(|e| format!("rename tmp→db.enc: {e}"))?;
    let _ = std::fs::remove_file(&db_path);
    // Exit so next launch decrypts before init_db.
    std::process::exit(0);
}

/// Decrypt `data/residents.db.enc` back to `data/residents.db`.
#[tauri::command]
pub async fn encryption_decrypt_database(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_data_dir = {
        let s = state.lock().await;
        s.app_data_dir.clone()
    };
    let db_path = app_data_dir.join("residents.db");
    let db_enc_path = db_path.with_extension("db.enc");
    if !db_enc_path.exists() {
        return Err("encrypted database not found".to_string());
    }
    // Decrypt db.enc → db via shared helper (same logic as startup self-heal).
    crate::crypto::decrypt_database_files(&db_path)?;
    Ok("Database decrypted to residents.db".to_string())
}

/// Encrypt all plaintext photos in `data/photos/` in place.
#[tauri::command]
pub async fn encryption_encrypt_photos(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_data_dir = {
        let s = state.lock().await;
        s.app_data_dir.clone()
    };
    let photos_dir = app_data_dir.join("photos");
    if !photos_dir.exists() {
        return Err("photos directory not found".to_string());
    }
    let count = crate::crypto::encrypt_dir_in_place(&photos_dir)?;
    Ok(format!("Encrypted {} photo(s)", count))
}

/// Decrypt all encrypted photos in `data/photos/` in place.
#[tauri::command]
pub async fn encryption_decrypt_photos(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_data_dir = {
        let s = state.lock().await;
        s.app_data_dir.clone()
    };
    let photos_dir = app_data_dir.join("photos");
    if !photos_dir.exists() {
        return Err("photos directory not found".to_string());
    }
    let count = crate::crypto::decrypt_dir_in_place(&photos_dir)?;
    Ok(format!("Decrypted {} photo(s)", count))
}
