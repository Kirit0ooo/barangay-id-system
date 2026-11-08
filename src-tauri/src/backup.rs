//! Manual backup and restore of the resident database.
//!
//! Both commands copy files, so every path is first gated by
//! [`validate_isolated_write_path`] — nothing may be written or read outside the
//! project tree. Copies are staged as `<name>.tmp` and renamed into place so an
//! interrupted run never leaves a truncated `.db` that still looks valid.

use crate::db::{
    checkpoint_wal_truncate, get_project_root, shm_path_for, validate_isolated_write_path,
    wal_path_for, AppState,
};
use anyhow::Result;
use chrono::Local;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

/// Copy the live database (and its photo directory) into a backup file.
///
/// # Arguments
///
/// * `backup_path` — Explicit destination file. When `None`, a timestamped
///   `data/backups/residents_backup_YYYYMMDD_HHMMSS.db` is used.
///
/// # Returns
///
/// The absolute path of the written `.db` backup.
///
/// # Errors
///
/// * The destination resolves outside the project tree.
/// * `data/residents.db` does not exist yet (nothing has been registered).
/// * The copy or rename fails (for example the destination is locked).
///
/// # Side Effects
///
/// * Flushes the SQLite WAL first; a busy checkpoint aborts the backup rather
///   than writing a snapshot that silently omits committed transactions.
/// * Creates directories and writes the `.db` file, plus a sibling
///   `photos_backup_<backup name>/` directory; a photo-copy failure is logged
///   as a warning and does not fail the command.
/// * Prunes older backups, keeping the newest 30 of each retention group.
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

    // P1 sync-blocking fix: create_dir_all off the async runtime.
    let dir_clone = backup_dir.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir_clone).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Backup task failed (join): {}", e))?
    .map_err(|e| format!("Backup task failed: {}", e))?;

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

    // Flush the WAL before copying so residents.db alone is a complete
    // snapshot; no pool lock is held during the copy itself.
    checkpoint_wal_truncate(&pool).await?;
    // P0 atomic backup: write to .tmp then rename so a crash never leaves a
    // truncated .db that looks valid. All blocking I/O off the async runtime.
    let src_clone = source_db.clone();
    let dst_clone = destination.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        if let Some(parent) = dst_clone.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = {
            let mut p = dst_clone.clone();
            let name = format!(
                "{}.tmp",
                dst_clone
                    .file_name()
                    .ok_or("Invalid backup path")?
                    .to_string_lossy()
            );
            p.set_file_name(name);
            p
        };
        std::fs::copy(&src_clone, &tmp).map_err(|e| format!("Backup failed: {}", e))?;
        std::fs::rename(&tmp, &dst_clone).map_err(|e| format!("Backup rename failed: {}", e))?;
        // No -wal/-shm sidecars: the TRUNCATE checkpoint above makes the .db
        // alone a complete snapshot (mirrors scheduled_backup policy) — -shm
        // is volatile shared memory, never a meaningful backup artifact.
        Ok(())
    })
    .await
    .map_err(|e| format!("Backup task failed (join): {}", e))?
    .map_err(|e| format!("Backup task failed: {}", e))?;

    let photos_dir = app_data_dir.join("photos");
    if photos_dir.exists() {
        let photos_backup = destination.with_file_name(format!(
            "photos_backup_{}",
            destination
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        let src_photos = photos_dir.clone();
        let dst_photos = photos_backup.clone();
        tokio::task::spawn_blocking(move || copy_dir_all(&src_photos, &dst_photos))
            .await
            .map_err(|e| format!("Photo backup task failed (join): {}", e))?
            .map_err(|e| format!("Photo backup task failed: {}", e))?;
    }

    // P2-6: retention — keep 30 newest backups (regular + scheduled share the
    // budget; safety backups are pruned separately in restore_database).
    crate::db::prune_oldest_files(&backup_dir, &["residents_backup_", "scheduled_"], 30);
    crate::db::prune_oldest_files(&backup_dir, &["photos_backup_"], 30);

    Ok(destination.to_string_lossy().to_string())
}

/// Replace the live database with a backup file, then reopen the connection pool.
///
/// # Arguments
///
/// * `backup_file` — Existing `.db` backup inside the project tree.
///
/// # Returns
///
/// `true` once the file was copied and the pool reopened with the same WAL
/// pragmas as [`crate::db::init_db`].
///
/// # Errors
///
/// * `backup_file` is missing or resolves outside the project tree.
/// * The safety copy, the restore copy, or the pool reopen fails. The pool
///   reopen case is the only one where the data was already replaced: the
///   message asks the operator to restart the app.
///
/// # Side Effects
///
/// * Checkpoints and closes the current pool, then overwrites
///   `data/residents.db` and removes its `-wal`/`-shm` sidecars.
/// * Writes a `safety_backup_<timestamp>.db` copy of the current database
///   before overwriting, and keeps the newest 10 safety backups.
/// * Replaces `state.pool` with a fresh connection pool.
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
    // P1 sync-blocking fix: create_dir_all off the async runtime
    {
        let sb = safety_backup.clone();
        tokio::task::spawn_blocking(move || {
            let parent = sb.parent().ok_or("Restore failed: invalid backup dir")?;
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| format!("Restore task failed: {}", e))??;
    }
    if destination.exists() {
        // The safety copy is the operator's rollback path, so it must hold
        // every committed row: a busy checkpoint aborts the restore here.
        checkpoint_wal_truncate(&pool).await?;
        // Atomic safety backup: copy to tmp then rename
        let src_safety = destination.clone();
        let dst_safety = safety_backup.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let tmp = {
                let mut p = dst_safety.clone();
                let name = format!(
                    "{}.tmp",
                    dst_safety
                        .file_name()
                        .ok_or("Invalid backup path")?
                        .to_string_lossy()
                );
                p.set_file_name(name);
                p
            };
            std::fs::copy(&src_safety, &tmp).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &dst_safety).map_err(|e| e.to_string())?;
            let wal = wal_path_for(&src_safety);
            if wal.exists() {
                let _ = std::fs::copy(&wal, wal_path_for(&dst_safety));
                let shm = shm_path_for(&src_safety);
                if shm.exists() {
                    let _ = std::fs::copy(&shm, shm_path_for(&dst_safety));
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| format!("Restore task failed: {}", e))??;
    } else {
        checkpoint_wal_truncate(&pool).await?;
    }

    // P2-6: retention — keep 10 newest safety backups.
    crate::db::prune_oldest_files(&app_data_dir.join("backups"), &["safety_backup_"], 10);

    // Close pool before overwriting the DB file
    pool.close().await;

    // Remove WAL/SHM sidecars so the next open starts clean (blocking off runtime)
    {
        let dw = wal_path_for(&destination);
        let ds = shm_path_for(&destination);
        tokio::task::spawn_blocking(move || {
            let _ = std::fs::remove_file(&dw);
            let _ = std::fs::remove_file(&ds);
        })
        .await
        .map_err(|e| format!("Restore task failed: {}", e))?;
    }

    // P0 atomic restore: copy to tmp then rename so live DB is never truncated
    {
        let src_r = source.clone();
        let dst_r = destination.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let tmp = {
                let mut p = dst_r.clone();
                let name = format!(
                    "{}.tmp",
                    dst_r
                        .file_name()
                        .ok_or("Invalid backup path")?
                        .to_string_lossy()
                );
                p.set_file_name(name);
                p
            };
            std::fs::copy(&src_r, &tmp).map_err(|e| {
                tracing::error!("Restore copy failed: {}", e);
                format!("Restore failed: {}", e)
            })?;
            std::fs::rename(&tmp, &dst_r).map_err(|e| {
                tracing::error!("Restore rename failed: {}", e);
                format!("Restore failed: {}", e)
            })?;
            Ok(())
        })
        .await
        .map_err(|e| format!("Restore task failed: {}", e))??;
    }

    // If the backup had wal/shm sidecars, also restore them (rare — checkpointed backups shouldn't need them, but handle consistently)
    {
        let src_wal = wal_path_for(&source);
        let src_shm = shm_path_for(&source);
        let dst_wal = wal_path_for(&destination);
        let dst_shm = shm_path_for(&destination);
        tokio::task::spawn_blocking(move || {
            if src_wal.exists() {
                let _ = std::fs::copy(&src_wal, &dst_wal);
                if src_shm.exists() {
                    let _ = std::fs::copy(&src_shm, &dst_shm);
                }
            }
        })
        .await
        .map_err(|e| format!("Restore task failed: {}", e))?;
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

/// Recursively copy every file under `src` into `dst`, creating directories as
/// needed. Blocking; callers run it on a blocking thread. Runs off the async
/// runtime and is best-effort at the call site, where a failure only warns.
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
