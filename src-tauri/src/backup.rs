use crate::db::{
    get_project_root, shm_path_for, validate_isolated_write_path, wal_path_for, AppState,
};
use anyhow::Result;
use chrono::Local;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

#[tauri::command]
pub async fn backup_database(
    state: State<'_, Arc<Mutex<AppState>>>,
    backup_path: Option<String>,
) -> Result<String, String> {
    // M-1: clone handles and release Mutex before any await/I-O
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };

    let source_db = app_data_dir.join("residents.db");
    let backup_dir = app_data_dir.join("backups");

    fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;

    let destination = if let Some(path) = backup_path {
        let raw = PathBuf::from(path);
        validate_isolated_write_path(&raw).map_err(|e| {
            format!(
                "Backup path rejected (must be inside project folder): {}",
                e
            )
        })?
    } else {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        backup_dir.join(format!("residents_backup_{}.db", timestamp))
    };

    validate_isolated_write_path(&destination)
        .map_err(|e| format!("Backup destination isolation error: {}", e))?;

    if !source_db.exists() {
        return Err("Database file not found - no data to backup yet".to_string());
    }

    // Ensure WAL is checkpointed before copy; no pool lock held during copy
    let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&source_db, &destination).map_err(|e| format!("Backup failed: {}", e))?;
    // H-5: use format!-based -wal/-shm paths to avoid with_extension double-extension bug
    let wal = wal_path_for(&source_db);
    let shm = shm_path_for(&source_db);
    if wal.exists() {
        let _ = fs::copy(&wal, wal_path_for(&destination));
        if shm.exists() {
            let _ = fs::copy(&shm, shm_path_for(&destination));
        }
    }

    let photos_dir = app_data_dir.join("photos");
    if photos_dir.exists() {
        let photos_backup = destination.with_file_name(format!(
            "photos_backup_{}",
            destination
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        if let Err(e) = copy_dir_all(&photos_dir, &photos_backup) {
            eprintln!("Warning: Could not backup photos: {}", e);
        }
    }

    // P2-6: retention — keep 30 newest backups (regular + scheduled share the
    // budget; safety backups are pruned separately in restore_database).
    crate::db::prune_oldest_files(&backup_dir, &["residents_backup_", "scheduled_"], 30);
    crate::db::prune_oldest_files(&backup_dir, &["photos_backup_"], 30);

    Ok(destination.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn restore_database(
    state: State<'_, Arc<Mutex<AppState>>>,
    backup_file: String,
) -> Result<bool, String> {
    let source_raw = PathBuf::from(&backup_file);
    let source = validate_isolated_write_path(&source_raw)
        .map_err(|e| format!("Restore source must be inside project folder: {}", e))?;
    if !source.exists() {
        return Err("Backup file not found".to_string());
    }
    if let Some(root) = get_project_root() {
        let root_can = root.canonicalize().unwrap_or(root.clone());
        let src_can = source.canonicalize().unwrap_or(source.clone());
        if !src_can.starts_with(&root_can)
            && !source.starts_with(&root_can)
            && !source.starts_with(&root)
        {
            return Err(format!(
                "Restore source must be inside project folder ({}). Got: {}",
                root_can.display(),
                source.display()
            ));
        }
    }

    // H-1: checkpoint WAL and close pool before copy — clone handles, checkpoint, close, then copy with no live handles
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };
    let destination = app_data_dir.join("residents.db");

    let safety_backup = app_data_dir.join("backups").join(format!(
        "safety_backup_{}.db",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    fs::create_dir_all(
        safety_backup
            .parent()
            .ok_or("Restore failed: invalid backup dir")?,
    )
    .map_err(|e| e.to_string())?;
    if destination.exists() {
        let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&pool)
            .await;
        fs::copy(&destination, &safety_backup).map_err(|e| e.to_string())?;
        // Copy safety WAL/SHM if present
        let wal = wal_path_for(&destination);
        if wal.exists() {
            let _ = fs::copy(&wal, wal_path_for(&safety_backup));
            let shm = shm_path_for(&destination);
            if shm.exists() {
                let _ = fs::copy(&shm, shm_path_for(&safety_backup));
            }
        }
    } else {
        let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&pool)
            .await;
    }

    // P2-6: retention — keep 10 newest safety backups.
    crate::db::prune_oldest_files(&app_data_dir.join("backups"), &["safety_backup_"], 10);

    // Close pool before overwriting the DB file
    pool.close().await;

    // Remove WAL/SHM sidecars so the next open starts clean
    let dest_wal = wal_path_for(&destination);
    let dest_shm = shm_path_for(&destination);
    let _ = fs::remove_file(&dest_wal);
    let _ = fs::remove_file(&dest_shm);

    fs::copy(&source, &destination).map_err(|e| {
        tracing::error!("Restore copy failed: {}", e);
        format!("Restore failed: {}", e)
    })?;

    // If the backup had wal/shm sidecars, also restore them (rare — checkpointed backups shouldn't need them, but handle consistently)
    let src_wal = wal_path_for(&source);
    let src_shm = shm_path_for(&source);
    if src_wal.exists() {
        let _ = fs::copy(&src_wal, &dest_wal);
        if src_shm.exists() {
            let _ = fs::copy(&src_shm, &dest_shm);
        }
    }

    // Reopen pool — rebuild with same WAL settings used in db::init_db
    let db_url = format!("sqlite:{}", destination.to_string_lossy());
    let reopened = async {
        use sqlx::sqlite::{
            SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
        };
        use std::str::FromStr;
        use std::time::Duration;
        let opts = SqliteConnectOptions::from_str(&db_url)
            .map_err(|e| e.to_string())?
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5))
            .foreign_keys(true)
            .pragma("cache_size", "-64000")
            .pragma("temp_store", "MEMORY")
            .create_if_missing(true);
        SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await
            .map_err(|e| e.to_string())
    }
    .await;

    match reopened {
        Ok(new_pool) => {
            let mut guard = state.lock().await;
            guard.pool = new_pool;
            tracing::info!(
                "Restore completed and pool reopened: {}",
                destination.display()
            );
        }
        Err(e) => {
            tracing::error!(
                "Restore copied but pool reopen failed ({}). Restart required: {}",
                e,
                destination.display()
            );
            eprintln!(
                "Restore copied but pool reopen failed ({}). Please restart the app.",
                e
            );
            return Err(format!(
                "Database restored but pool reopen failed ({}). Please restart the app.",
                e
            ));
        }
    }

    Ok(true)
}

fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}
