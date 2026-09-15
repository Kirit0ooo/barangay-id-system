use crate::db::{validate_isolated_write_path, AppState};
use chrono::Local;
use std::collections::HashSet;
use std::fs;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

#[derive(Debug, serde::Serialize)]
pub struct ScheduledBackupResult {
    pub path: String,
    pub wal_checkpoint: String,
}

#[derive(Debug, serde::Serialize)]
pub struct PhotoGcResult {
    pub total_files: usize,
    pub referenced: usize,
    pub orphan_count: usize,
    pub orphans: Vec<String>,
    pub deleted: usize,
}

/// R5 — scheduled backup: WAL checkpoint + isolated fs::copy to data/backups/scheduled_YYYYMMDD_HHMMSS.db
/// Called via frontend setInterval (24h) when app stays open. 100% offline, no cron daemon.
#[tauri::command]
pub async fn scheduled_backup(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<ScheduledBackupResult, String> {
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };
    let source_db = app_data_dir.join("residents.db");
    if !source_db.exists() {
        return Err("Database file not found — no data to backup yet".into());
    }
    // WAL checkpoint to ensure -wal is flushed before copy
    let ck = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;
    let ck_msg = format!("{:?}", ck.rows_affected());

    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let dest = app_data_dir
        .join("backups")
        .join(format!("scheduled_{}.db", ts));
    validate_isolated_write_path(&dest)
        .map_err(|e| format!("Scheduled backup path isolation error: {}", e))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&source_db, &dest).map_err(|e| format!("Scheduled backup copy failed: {}", e))?;
    // Also copy WAL/SHM if present (checkpoint should have truncated, but copy for completeness)
    let wal = crate::db::wal_path_for(&source_db);
    if wal.exists() {
        let _ = fs::copy(&wal, crate::db::wal_path_for(&dest));
        let shm = crate::db::shm_path_for(&source_db);
        if shm.exists() {
            let _ = fs::copy(&shm, crate::db::shm_path_for(&dest));
        }
    }
    tracing::info!("scheduled_backup created at {}", dest.display());
    // P2-6: retention — keep 30 newest backups (shares budget with manual backups).
    if let Some(parent) = dest.parent() {
        crate::db::prune_oldest_files(parent, &["residents_backup_", "scheduled_"], 30);
    }
    Ok(ScheduledBackupResult {
        path: dest.to_string_lossy().to_string(),
        wal_checkpoint: ck_msg,
    })
}

/// R5 — photo GC: scan data/photos for orphan files not referenced in residents.photo_filename
/// Logs count; only deletes when `delete` flag is true (Settings manual GC button passes true).
#[tauri::command]
pub async fn photo_gc(
    state: State<'_, Arc<Mutex<AppState>>>,
    delete: Option<bool>,
) -> Result<PhotoGcResult, String> {
    let do_delete = delete.unwrap_or(false);
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };
    let photos_dir = app_data_dir.join("photos");
    if !photos_dir.exists() {
        return Ok(PhotoGcResult {
            total_files: 0,
            referenced: 0,
            orphan_count: 0,
            orphans: vec![],
            deleted: 0,
        });
    }
    // Collect referenced filenames (bare names)
    let rows: Vec<Option<String>> =
        sqlx::query_scalar("SELECT photo_filename FROM residents WHERE photo_filename IS NOT NULL")
            .fetch_all(&pool)
            .await
            .map_err(|e| e.to_string())?;
    let mut referenced: HashSet<String> = HashSet::new();
    for opt in rows {
        if let Some(v) = opt {
            if let Some(base) = crate::residents::photo_basename(&v) {
                referenced.insert(base);
            } else {
                // fallback: raw trimming
                let t = v.trim().to_string();
                if !t.is_empty() {
                    referenced.insert(t);
                }
            }
        }
    }
    let mut total = 0usize;
    let mut orphans: Vec<String> = Vec::new();
    for entry in fs::read_dir(&photos_dir).map_err(|e| e.to_string())? {
        let e = entry.map_err(|e| e.to_string())?;
        if e.file_type().map_err(|e| e.to_string())?.is_file() {
            total += 1;
            let name = e.file_name().to_string_lossy().to_string();
            if !referenced.contains(&name) {
                orphans.push(name);
            }
        }
    }
    let orphan_count = orphans.len();
    let referenced_count = total.saturating_sub(orphan_count);
    tracing::info!(
        "photo_gc scan: total={} referenced={} orphan={} delete={}",
        total,
        referenced_count,
        orphan_count,
        do_delete
    );
    let mut deleted = 0usize;
    if do_delete && !orphans.is_empty() {
        for name in &orphans.clone() {
            let p = photos_dir.join(name);
            // Validate isolated before delete (stay inside project)
            if validate_isolated_write_path(&p).is_ok() && p.is_file() {
                match fs::remove_file(&p) {
                    Ok(_) => {
                        deleted += 1;
                        tracing::info!("photo_gc deleted orphan {}", name);
                    }
                    Err(e) => tracing::warn!("photo_gc failed to delete {}: {}", name, e),
                }
            }
        }
        tracing::info!("photo_gc deleted {}/{} orphans", deleted, orphan_count);
    } else if !do_delete && orphan_count > 0 {
        tracing::info!("photo_gc orphans (not deleted): {:?}", orphans);
    }
    // Cap orphans list returned to frontend to avoid huge payload
    let returned = if orphans.len() > 50 {
        let mut v = orphans[..50].to_vec();
        v.push(format!("... and {} more", orphans.len() - 50));
        v
    } else {
        orphans
    };
    Ok(PhotoGcResult {
        total_files: total,
        referenced: referenced_count,
        orphan_count,
        orphans: returned,
        deleted,
    })
}
