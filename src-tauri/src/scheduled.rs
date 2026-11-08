use crate::db::{validate_isolated_write_path, AppState};
use chrono::Local;
use sqlx::Row;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

/// Result of a scheduled backup, returned to the frontend caller.
#[derive(Debug, serde::Serialize)]
pub struct ScheduledBackupResult {
    /// Absolute path of the written backup file.
    pub path: String,
    /// Frames flushed from the WAL before the copy, for diagnostics.
    pub wal_checkpoint: String,
}

/// Result of a photo garbage-collection scan.
#[derive(Debug, serde::Serialize)]
pub struct PhotoGcResult {
    /// Files found in `data/photos` (top level; subdirectories are not scanned).
    pub total_files: usize,
    /// Files matched by a resident's `photo_filename`.
    pub referenced: usize,
    /// Unreferenced files found.
    pub orphan_count: usize,
    /// Up to 50 orphan names, with a tail entry counting the remainder.
    pub orphans: Vec<String>,
    /// Orphans actually removed (0 when the command ran in scan-only mode).
    pub deleted: usize,
}

/// Allowlist for the VACUUM INTO fallback path: SQLite offers no bind
/// parameter for the INTO filename, so the interpolated path must be
/// provably machine-generated — exactly `scheduled_YYYYMMDD_HHMMSS.db`
/// (timestamp digits + one underscore; no quotes, no separators, no suffix).
/// Pure function, unit-tested below.
fn is_machine_backup_name(file_name: &str) -> bool {
    let base = match file_name.strip_suffix(".db") {
        Some(b) => b,
        None => return false,
    };
    let stamp = match base.strip_prefix("scheduled_") {
        Some(s) => s,
        None => return false,
    };
    stamp.len() == 15
        && stamp.bytes().enumerate().all(|(i, b)| {
            if i == 8 {
                b == b'_'
            } else {
                b.is_ascii_digit()
            }
        })
}

/// R5 — scheduled backup: WAL checkpoint + isolated copy to
/// `data/backups/scheduled_YYYYMMDD_HHMMSS.db`.
///
/// Driven by a frontend `setInterval` (24h) while the app stays open; fully
/// offline, no cron daemon.
///
/// # Returns
///
/// The written path plus WAL checkpoint diagnostics.
///
/// # Errors
///
/// * `data/residents.db` does not exist yet.
/// * The WAL checkpoint fails.
/// * The isolated path check fails.
/// * All three copy attempts fail **and** the `VACUUM INTO` fallback fails or its
///   destination name is not machine-generated.
///
/// # Side Effects
///
/// Checkpoints the WAL, writes a `.db` backup (retrying a locked copy, then
/// falling back to `VACUUM INTO`), prunes each retention group to its newest 30
/// entries, and runs [`photo_gc`] — deleting orphans unless
/// `photo_gc_dry_run` is set in `config/civic.json`. A photo-GC failure is
/// ignored so it can never fail the backup itself.
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
    // Flush the WAL before copying so residents.db alone is a complete
    // snapshot. A busy checkpoint aborts the backup: continuing would copy a
    // file missing every transaction still held in the WAL.
    let checkpointed_frames = crate::db::checkpoint_wal_truncate(&pool).await?;
    let ck_msg = format!("checkpointed {} frame(s)", checkpointed_frames);

    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let dest = app_data_dir
        .join("backups")
        .join(format!("scheduled_{}.db", ts));
    validate_isolated_write_path(&dest)
        .map_err(|e| format!("Scheduled backup path isolation error: {}", e))?;
    if let Some(parent) = dest.parent() {
        let p = parent.to_path_buf();
        tokio::task::spawn_blocking(move || std::fs::create_dir_all(&p).map_err(|e| e.to_string()))
            .await
            .map_err(|e| format!("Backup task failed (join): {}", e))?
            .map_err(|e| format!("Backup task failed: {}", e))?;
    }
    // Windows hot-backup: the live DB can be briefly locked by the pool /
    // AV indexer (os error 32 seen in run logs). Retry the plain copy, then
    // fall back to `VACUUM INTO` which produces a consistent snapshot
    // through the SQLite connection instead of the filesystem.
    // P1 sync-blocking fix: copy and sleep off the async runtime.
    let mut copy_err = String::new();
    let mut copied = false;
    for attempt in 1..=3 {
        let src = source_db.clone();
        let dst = dest.clone();
        // P0 atomic: copy to a .tmp sibling then rename (mirrors backup_database)
        // so an interrupted run never leaves a truncated scheduled_*.db that looks
        // valid by name. fs::copy overwrites the tmp, so retries are safe.
        let copy_res = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let tmp = {
                let mut p = dst.clone();
                let name = format!(
                    "{}.tmp",
                    dst.file_name()
                        .ok_or("Invalid backup path")?
                        .to_string_lossy()
                );
                p.set_file_name(name);
                p
            };
            std::fs::copy(&src, &tmp).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &dst).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string());
        match copy_res {
            Ok(Ok(())) => {
                copied = true;
                break;
            }
            Ok(Err(e)) => {
                copy_err = e;
                tracing::warn!(
                    "scheduled_backup copy attempt {}/3 failed: {}",
                    attempt,
                    copy_err
                );
                tokio::time::sleep(std::time::Duration::from_millis(250 * attempt as u64)).await;
            }
            Err(e) => {
                copy_err = e;
                tracing::warn!(
                    "scheduled_backup copy attempt {}/3 join failed: {}",
                    attempt,
                    copy_err
                );
                tokio::time::sleep(std::time::Duration::from_millis(250 * attempt as u64)).await;
            }
        }
    }
    if !copied {
        // Hot-backup fallback: VACUUM INTO writes a compacted, checkpointed
        // copy via the live connection — no filesystem lock contention.
        // SQLite has no bind parameter for the INTO filename, so the
        // allowlist gate above proves the interpolated path is
        // machine-generated before any quoting happens.
        let fname = dest.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !is_machine_backup_name(fname) {
            return Err(
                "Scheduled backup: refusing VACUUM INTO fallback — destination filename is not machine-generated"
                    .into(),
            );
        }
        // P0 atomic: VACUUM INTO a .tmp then rename. VACUUM INTO errors if the
        // target already exists (unlike fs::copy), so clear any stale tmp a prior
        // crashed run left behind — otherwise the safety fallback fails exactly
        // when it's needed. The allowlist gate above still validates the FINAL
        // dest name; tmp is only a transient staging name derived from it.
        let tmp_path = dest.with_extension("db.tmp");
        let tmp_cleanup = tmp_path.clone();
        tokio::task::spawn_blocking(move || {
            let _ = std::fs::remove_file(&tmp_cleanup);
        })
        .await
        .map_err(|e| format!("Scheduled backup tmp cleanup task failed: {}", e))?;
        let tmp_str = tmp_path.to_string_lossy().replace('\'', "''");
        let vacuum_sql = format!("VACUUM INTO '{}'", tmp_str);
        sqlx::query(&vacuum_sql)
            .execute(&pool)
            .await
            .map_err(|e| {
                format!(
                    "Scheduled backup copy failed after 3 retries ({}); VACUUM INTO fallback also failed: {}",
                    copy_err,
                    e
                )
            })?;
        let tmp_rename = tmp_path.clone();
        let dst_rename = dest.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::rename(&tmp_rename, &dst_rename).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| format!("Scheduled backup rename failed (join): {}", e))?
        .map_err(|e| format!("Scheduled backup rename failed: {}", e))?;
        tracing::info!(
            "scheduled_backup used VACUUM INTO fallback after copy retries: {}",
            dest.display()
        );
    }
    // No -wal/-shm sidecars are copied: the TRUNCATE checkpoint above makes
    // the .db file alone a complete snapshot, -shm is volatile shared memory
    // that is meaningless outside the live process, and a post-checkpoint
    // -wal is empty. Skipping them keeps backups self-contained.
    tracing::info!("scheduled_backup created at {}", dest.display());
    // P2-6: retention — keep 30 newest backups (shares budget with manual backups).
    if let Some(parent) = dest.parent() {
        crate::db::prune_oldest_files(parent, &["residents_backup_", "scheduled_"], 30);
        crate::db::prune_oldest_files(parent, &["config_backup_"], 30);
    }
    // P2-7: photo GC — delete orphan photos after each scheduled backup so the
    // photo dir never accumulates unreferenced files between manual GC runs.
    // Respect civic config toggle: dry_run=true scans+logs only (no deletes).
    let dry_run = crate::config::load_civic_config().photo_gc_dry_run;
    let _ = photo_gc(state.clone(), Some(!dry_run)).await;

    Ok(ScheduledBackupResult {
        path: dest.to_string_lossy().to_string(),
        wal_checkpoint: ck_msg,
    })
}

/// R5 — photo GC: find files in `data/photos` that no resident references.
///
/// # Arguments
///
/// * `delete` — `Some(true)` removes the orphans, `Some(false)` / `None` only
///   reports them. The Settings button passes `true`; the scheduled backup
///   passes the inverse of `photo_gc_dry_run`.
///
/// # Returns
///
/// A [`PhotoGcResult`] with the file counts and the orphan names.
///
/// # Errors
///
/// Returns an error when the residents query or the directory scan fails. A
/// missing `data/photos` directory is not an error — an empty result is returned.
/// Individual delete failures are logged as warnings and skipped.
///
/// # Side Effects
///
/// Reads the database and the photos directory; deletes orphan files when
/// `delete` is true, each gated by the isolation check so nothing outside the
/// project tree can be removed. Name matching is case-folded for Windows.
///
/// # Panics
///
/// Never. A file that disappears between the scan and the delete is logged.
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
    let rows: Vec<Option<String>> =
        sqlx::query("SELECT photo_filename FROM residents WHERE photo_filename IS NOT NULL")
            .fetch_all(&pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| r.get::<Option<String>, _>(0))
            .collect();
    let mut referenced: HashSet<String> = HashSet::new();
    for opt in rows {
        if let Some(v) = opt {
            if let Some(base) = crate::residents::photo_basename(&v) {
                // M4: Windows FS is case-insensitive — fold so ABC.JPG does
                // not read as orphan of abc.jpg.
                referenced.insert(base.to_lowercase());
            } else {
                // fallback: raw trimming
                let t = v.trim().to_string();
                if !t.is_empty() {
                    referenced.insert(t.to_lowercase());
                }
            }
        }
    }
    // P1 sync-blocking fix: read_dir off the async runtime.
    let (total, orphans) = {
        let dir = photos_dir.clone();
        let refs = referenced.clone();
        tokio::task::spawn_blocking(move || -> Result<(usize, Vec<String>), String> {
            let mut total = 0usize;
            let mut orphans: Vec<String> = Vec::new();
            for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
                let e = entry.map_err(|e| e.to_string())?;
                if e.file_type().map_err(|e| e.to_string())?.is_file() {
                    total += 1;
                    let name = e.file_name().to_string_lossy().to_string();
                    if !refs.contains(&name.to_lowercase()) {
                        orphans.push(name);
                    }
                }
            }
            Ok((total, orphans))
        })
        .await
        .map_err(|e| format!("photo_gc task failed (join): {}", e))?
        .map_err(|e| format!("photo_gc task failed: {}", e))?
    };
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
                let p_clone = p.clone();
                let name_clone = name.clone();
                let res = tokio::task::spawn_blocking(move || std::fs::remove_file(&p_clone))
                    .await
                    .map_err(|e| e.to_string());
                match res {
                    Ok(Ok(_)) => {
                        deleted += 1;
                        tracing::info!("photo_gc deleted orphan {}", name_clone);
                    }
                    Ok(Err(e)) => tracing::warn!("photo_gc failed to delete {}: {}", name_clone, e),
                    Err(e) => tracing::warn!("photo_gc join failed for {}: {}", name_clone, e),
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

#[cfg(test)]
mod scheduled_tests {
    use super::*;

    #[test]
    fn machine_backup_name_allows_only_timestamped() {
        assert!(is_machine_backup_name("scheduled_20260919_120000.db"));
        assert!(is_machine_backup_name("scheduled_20000101_000000.db"));
        assert!(!is_machine_backup_name("scheduled_20260919_120000.db-wal"));
        assert!(!is_machine_backup_name("scheduled_20260919_120000.db-shm"));
        assert!(!is_machine_backup_name(
            "residents_backup_20260919_120000.db"
        ));
        assert!(!is_machine_backup_name("scheduled_20260919-120000.db"));
        assert!(!is_machine_backup_name("scheduled_20260919120000.db"));
        assert!(!is_machine_backup_name("scheduled_' OR '1'='1.db"));
        assert!(!is_machine_backup_name("scheduled_20260919_120000.db "));
        assert!(!is_machine_backup_name(""));
        assert!(!is_machine_backup_name("scheduled_.db"));
    }
}
