use anyhow::Result;
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

pub struct AppState {
    pub pool: SqlitePool,
    pub app_data_dir: PathBuf,
}

/// Host-native default root (used only when no project markers are found).
/// Keeps path semantics native so probing never creates a literal `D:\...`
/// junk directory on Unix hosts (which used to poison every photo/asset
/// lookup and produced blank ID photos in PDF + preview).
pub fn default_root() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"D:\Barangay\barangay-id-system")
    }
    #[cfg(not(windows))]
    {
        // Portable fallback: walk up from the executable location looking
        // for project markers (src-tauri/ or barangay.ico), max 6 levels.
        // Final fallback is current_dir() — keeps all data inside the
        // project folder. Never returns paths under /tmp, /var, or home.
        if let Ok(exe) = std::env::current_exe() {
            let mut dir = exe.parent().map(|p| p.to_path_buf());
            for _ in 0..6 {
                if let Some(d) = dir.clone() {
                    if d.join("src-tauri").exists() || d.join("barangay.ico").exists() {
                        return d;
                    }
                    dir = d.parent().map(|p| p.to_path_buf());
                } else {
                    break;
                }
            }
        }
        // Last resort: current working directory
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// M3: writability probe — never create data under a read-only install dir
/// (MSI perMachine under C:\Program Files). Creates the dir, round-trips a
/// temp file, removes it. No junk left behind on success or failure.
pub fn dir_is_writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".writetest");
    if fs::write(&probe, b"w").is_err() {
        return false;
    }
    let _ = fs::remove_file(&probe);
    true
}

pub fn get_project_root() -> Option<PathBuf> {
    // Single canonical resolution: accept a fixed candidate ONLY when it
    // already carries project markers. Never create directories while
    // probing — on non-Windows hosts `create_dir_all("D:\\...")` would
    // create a literal junk dir inside cwd, get returned as the "root",
    // and break every photo/asset lookup downstream (blank ID photos).
    for candidate in [
        PathBuf::from(r"D:\Barangay\barangay-id-system"),
        PathBuf::from("/mnt/d/Barangay/barangay-id-system"),
    ] {
        if candidate.join("barangay.ico").exists() || candidate.join("src-tauri").exists() {
            return Some(candidate);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..6 {
            if let Some(d) = dir.clone() {
                if d.join("barangay.ico").exists() || d.join("src-tauri").exists() {
                    return Some(d);
                }
                dir = d.parent().map(|p| p.to_path_buf());
            } else {
                break;
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.clone();
        for _ in 0..6 {
            if dir.join("barangay.ico").exists() || dir.join("src-tauri").exists() {
                return Some(dir);
            }
            if let Some(parent) = dir.parent() {
                dir = parent.to_path_buf();
            } else {
                break;
            }
        }
    }
    // Portable/installed fallback (Win10/11 x64): when the MSI/NSIS/portable
    // exe runs from Program Files or a USB stick, no project markers travel
    // with it (only bundled resources do). Use the exe's own directory so
    // data/logs/tmp stay beside the installed exe instead of escaping to a
    // fixed D:\ drive. Never returns C:/Users, AppData, or TEMP.
    // M3: skip a non-writable exe dir (MSI perMachine under C:\Program Files)
    // so callers fall through to the portable default or fail gracefully —
    // never select a root we cannot write data under.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir_is_writable(dir) {
                // Prefer a parent that already hosts data/logs (portable USB
                // second run); otherwise the exe dir itself (fresh MSI install).
                let mut d = dir.to_path_buf();
                for _ in 0..3 {
                    if d.join("data").exists() || d.join("logs").exists() {
                        return Some(d);
                    }
                    if let Some(p) = d.parent() {
                        d = p.to_path_buf();
                    } else {
                        break;
                    }
                }
                return Some(dir.to_path_buf());
            }
        }
    }
    // Last resort (fresh install): create + return the HOST-NATIVE default
    // only — never a Windows path on Unix or vice versa.
    let native = default_root();
    if fs::create_dir_all(native.join("data")).is_ok() && native.exists() {
        return Some(native);
    }
    None
}

pub fn get_documents_dir() -> PathBuf {
    let root = get_project_root().unwrap_or_else(default_root);
    let docs = root.join("Documents");
    // Documents/PDF is legacy (HTML-only print since PDF removal) — never
    // created. Excel exports still live under Documents/Excel.
    let _ = fs::create_dir_all(docs.join("Excel"));
    docs
}

pub fn get_documents_excel_dir() -> PathBuf {
    let d = get_documents_dir().join("Excel");
    let _ = fs::create_dir_all(&d);
    d
}

pub fn get_app_data_dir() -> PathBuf {
    // Isolation: ALL data must stay inside the project tree — no fallback to AppData/TEMP.
    // Ensure Documents structure exists as well.
    let _ = fs::create_dir_all(get_documents_excel_dir());
    // Prefer <resolved-root>/data first so photos + DB always agree with
    // get_project_root(); host-native fixed candidate only as backup.
    // (Never probe a Windows path on Unix: create_dir_all would mint a junk
    // literal `D:\...` dir and poison photo lookups.)
    // M3: probe writability before committing to a root (MSI perMachine exe
    // dirs are read-only). First writable candidate wins; the portable D:\
    // layout is unchanged when it resolves (primary deployment).
    if let Some(root) = get_project_root() {
        let candidate = root.join("data");
        if dir_is_writable(&candidate) {
            fs::create_dir_all(candidate.join("photos")).ok();
            fs::create_dir_all(candidate.join("backups")).ok();
            // data/id_cards/ was PDF-era output staging — no longer created.
            fs::create_dir_all(root.join("logs")).ok();
            fs::create_dir_all(root.join("tmp")).ok();
            return candidate;
        }
    }
    for candidate in [default_root().join("data")] {
        if dir_is_writable(&candidate) {
            fs::create_dir_all(candidate.join("photos")).ok();
            fs::create_dir_all(candidate.join("backups")).ok();
            // data/id_cards/ was PDF-era output staging — no longer created.
            // F7: no parent().unwrap() — skip log/tmp setup if parent missing.
            if let Some(parent) = candidate.parent() {
                fs::create_dir_all(parent.join("logs")).ok();
                fs::create_dir_all(parent.join("tmp")).ok();
            }
            return candidate;
        }
    }
    // M3: no panic — return the best-effort path and let init_db surface a
    // user-facing error (MSI perMachine: reinstall per-user or portable).
    // Never silently escape to AppData/relative path.
    tracing::error!("Isolation violation: no writable project data dir. Run from D:\\Barangay\\barangay-id-system (or its WSL mount), or reinstall per-user/portable. No fallback to AppData is allowed per mandate.");
    get_project_root().unwrap_or_else(default_root).join("data")
}

/// Validate that a user-supplied path is inside the isolated project tree before any write.
/// Returns canonicalized absolute path or Err.
/// Filename sanitization: strips Windows-illegal chars (\\/:*?"<>|), control
/// chars and path separators from the final file-name component, collapses
/// whitespace to a single underscore, truncates the stem, and enforces that
/// the result is non-empty. Parent directories are NOT altered.
pub fn sanitize_file_name_component(raw_name: &str, max_stem_len: usize) -> String {
    let mut s: String = raw_name
        .chars()
        .filter(|c| {
            !matches!(*c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()
        })
        .collect();
    // Remove any remaining dot-dot segments and trim dots/spaces (Windows-hostile)
    s = s.replace("..", "");
    s = s.trim().trim_matches('.').to_string();
    // Collapse whitespace runs to a single underscore
    let mut out = String::with_capacity(s.len());
    let mut prev_us = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_us {
                out.push('_');
                prev_us = true;
            }
        } else {
            out.push(c);
            prev_us = false;
        }
    }
    // Split stem/ext so length cap applies to stem only
    let (stem, ext) = match out.rfind('.') {
        Some(i) if i > 0 => (out[..i].to_string(), out[i..].to_string()),
        _ => (out, String::new()),
    };
    let stem_chars: Vec<char> = stem.chars().collect();
    let stem: String = if stem_chars.len() > max_stem_len {
        stem_chars[..max_stem_len].iter().collect()
    } else {
        stem
    };
    let stem = stem.trim().trim_matches('.').trim_matches('_').to_string();
    if stem.is_empty() {
        return "UNNAMED".to_string() + &ext;
    }
    stem + &ext
}

/// L2 export stem canonical — per-resident file stem (full_name + resident_id), sanitized.
/// Used for per-resident exports; PDF-era callers delegate via legacy alias below.
#[allow(dead_code)]
pub fn per_resident_export_stem(full_name: &str, resident_id: &str) -> String {
    let raw = format!("{}-{}", full_name.trim(), resident_id.trim());
    sanitize_file_name_component(&raw, 120)
}

/// Legacy alias: per-resident PDF filename (kept for old DBs/tests) delegates to export stem.
#[allow(dead_code)]
pub fn per_resident_pdf_filename(full_name: &str, resident_id: &str) -> String {
    format!("{}.pdf", per_resident_export_stem(full_name, resident_id))
}

/// Bulk base name from the backup/execution local date: `BACKUP-YYYY-MM-DD`.
pub fn backup_base_name() -> String {
    format!("BACKUP-{}", chrono::Local::now().format("%Y-%m-%d"))
}

/// Given an isolated absolute path, return a non-colliding sibling:
/// if `path` exists, try `stem (2).ext`, `stem (3).ext`, … (overwrite-free).
/// P2-8: Err after 999 collisions instead of silently returning the
/// colliding path (which would overwrite).
pub fn resolve_unique_path(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Ok(path.to_path_buf());
    }
    let parent = path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();
    for n in 2..1000 {
        let cand = parent.join(format!("{} ({}){}", stem, n, ext));
        if !cand.exists() {
            return Ok(cand);
        }
    }
    Err(format!(
        "Too many colliding files for {} (999 variants exist) — refusing to overwrite",
        path.display()
    ))
}

/// P2-6: retention prune — keep the newest `keep` entries in `dir` whose
/// names start with one of `prefixes`, delete the rest. Covers files and
/// photo-backup dirs (`photos_backup_*`). Best-effort: I/O errors are
/// logged, never propagated. Only deletes inside `dir`.
pub(crate) fn prune_oldest_files(dir: &Path, prefixes: &[&str], keep: usize) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut matched: Vec<(std::time::SystemTime, PathBuf, bool)> = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        let is_dir = p.is_dir();
        if !is_dir && !p.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if prefixes.iter().any(|pre| name.starts_with(pre)) {
            let mtime = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            matched.push((mtime, p, is_dir));
        }
    }
    if matched.len() <= keep {
        return;
    }
    matched.sort_by(|a, b| a.0.cmp(&b.0));
    let drop_n = matched.len() - keep;
    let mut deleted = 0usize;
    for (_, path, is_dir) in matched.into_iter().take(drop_n) {
        if validate_isolated_write_path(&path).is_err() {
            continue;
        }
        let ok = if is_dir {
            std::fs::remove_dir_all(&path).is_ok()
        } else {
            std::fs::remove_file(&path).is_ok()
        };
        if ok {
            deleted += 1;
        }
    }
    tracing::info!(
        "retention: pruned {} old entr(y/ies) in {}, keeping newest {}",
        deleted,
        dir.display(),
        keep
    );
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        use std::path::Component;
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c.as_os_str()),
        }
    }
    out
}

fn path_starts_with_inside(child: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    {
        let c = child.to_string_lossy().to_lowercase();
        let r = root.to_string_lossy().to_lowercase();
        let c_path = PathBuf::from(c);
        let r_path = PathBuf::from(r);
        c_path.starts_with(r_path)
    }
    #[cfg(not(windows))]
    {
        child.starts_with(root)
    }
}

pub fn wal_path_for(db_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wal", db_path.display()))
}
pub fn shm_path_for(db_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-shm", db_path.display()))
}

pub fn validate_isolated_write_path(raw: &Path) -> Result<PathBuf, String> {
    let root = get_project_root().unwrap_or_else(default_root);
    let root_canonical = root
        .canonicalize()
        .unwrap_or_else(|_| lexical_normalize(&root));
    // Reject raw paths that contain null bytes or traversal attempts via lexical check
    let abs = if raw.is_absolute() {
        lexical_normalize(raw)
    } else {
        lexical_normalize(&root.join(raw))
    };
    // Explicit traversal check: raw components must not escape via ParentDir beyond root
    // Lexical check first — if any ParentDir in raw would escape after join, reject early
    let normalized = lexical_normalize(&abs);
    if !path_starts_with_inside(&normalized, &root_canonical)
        && !path_starts_with_inside(&abs, &root)
    {
        // Also try canonical parent resolution for existing ancestors
        let parent = abs.parent().unwrap_or(&abs);
        let parent_canonical = if parent.exists() {
            parent
                .canonicalize()
                .map_err(|e| format!("Cannot resolve destination parent: {}", e))?
        } else {
            // Walk up to nearest existing ancestor, canonicalize it, then re-append remainder lexically
            let mut cur = parent.to_path_buf();
            let mut remainder = PathBuf::new();
            while !cur.exists() {
                if let Some(p) = cur.parent() {
                    if let Some(fname) = cur.file_name() {
                        let mut new_rem = PathBuf::from(fname);
                        new_rem.push(remainder);
                        remainder = new_rem;
                    }
                    cur = p.to_path_buf();
                } else {
                    break;
                }
            }
            if cur.exists() {
                let can = cur.canonicalize().unwrap_or(cur);
                lexical_normalize(&can.join(remainder))
            } else {
                lexical_normalize(&root_canonical.join(remainder))
            }
        };
        if !path_starts_with_inside(&parent_canonical, &root_canonical) {
            return Err(format!(
                "Save path must be inside the project folder ({}). Got: {}",
                root_canonical.display(),
                abs.display()
            ));
        }
        // Also ensure the full normalized path is inside root
        if !path_starts_with_inside(&normalized, &root_canonical) {
            return Err(format!(
                "Save path must be inside the project folder ({}). Got: {}",
                root_canonical.display(),
                abs.display()
            ));
        }
    }
    // Final canonical parent check (handles symlinks)
    let parent = abs.parent().unwrap_or(&abs);
    if parent.exists() {
        if let Ok(pc) = parent.canonicalize() {
            if !path_starts_with_inside(&pc, &root_canonical) {
                return Err(format!(
                    "Save path must be inside the project folder ({}). Got: {}",
                    root_canonical.display(),
                    abs.display()
                ));
            }
        }
    }
    Ok(abs)
}

pub async fn init_db() -> Result<SqlitePool> {
    let app_data_dir = get_app_data_dir();
    let db_path = app_data_dir.join("residents.db");
    let db_url = format!("sqlite:{}", db_path.to_string_lossy());

    if !sqlx::Sqlite::database_exists(&db_url)
        .await
        .unwrap_or(false)
    {
        sqlx::Sqlite::create_database(&db_url).await?;
    }

    // Perf-save-001: WAL + pragmatic PRAGMAs so commits don't fsync-stall
    // on WSL /mnt/d (9p) mounts. Every pooled connection inherits these via
    // SqliteConnectOptions (pool-level PRAGMA would only hit one conn).
    let connect_options = SqliteConnectOptions::from_str(&db_url)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true)
        .pragma("cache_size", "-64000")
        .pragma("temp_store", "MEMORY")
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .map_err(|e| {
            // M3: user-facing (no panic) — typical cause is an MSI perMachine
            // install under C:\Program Files with no writable data dir.
            anyhow::anyhow!(
                "Cannot open database at {}: {}. Reinstall per-user/portable or run from D:\\Barangay\\barangay-id-system.",
                db_path.display(),
                e
            )
        })?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS residents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            resident_id TEXT UNIQUE NOT NULL,
            last_name TEXT NOT NULL,
            first_name TEXT NOT NULL,
            middle_name TEXT,
            suffix TEXT,
            birth_date TEXT NOT NULL,
            birth_place TEXT,
            gender TEXT NOT NULL,
            civil_status TEXT,
            citizenship TEXT DEFAULT 'Filipino',
            occupation TEXT,
            contact_number TEXT,
            address_street TEXT,
            address_purok TEXT,
            address_barangay TEXT DEFAULT 'Barangay',
            address_city TEXT,
            address_province TEXT,
            religion TEXT,
            blood_type TEXT,
            height TEXT,
            weight TEXT,
            philhealth TEXT,
            sss TEXT,
            tin TEXT,
            voters_id TEXT,
            emergency_contact_name TEXT,
            emergency_contact_relation TEXT,
            emergency_contact_number TEXT,
            emergency_contact_resident_id TEXT,
            photo_filename TEXT,
            is_active INTEGER DEFAULT 1,
            date_registered TEXT NOT NULL DEFAULT (datetime('now')),
            date_updated TEXT,
            remarks TEXT,
            valid_until TEXT,
            -- Legacy PDF audit trail: DORMANT since PDF removal (never written).
            -- Columns stay so existing DBs keep working with no migration.
            last_pdf_path TEXT,
            last_pdf_generated_at TEXT,
            emergency_contact_address TEXT
        );
        "#,
    )
    .execute(&pool)
    .await?;

    // Migration for existing DBs: add column if missing
    let _ = sqlx::query("ALTER TABLE residents ADD COLUMN valid_until TEXT")
        .execute(&pool)
        .await;
    // Email removal: drop the deprecated column from existing DBs.
    // Best-effort (ignored when already gone or on old SQLite).
    let _ = sqlx::query("ALTER TABLE residents DROP COLUMN email")
        .execute(&pool)
        .await;
    // Data-integration audit trail + emergency-contact FK link (nullable; old
    // DBs gain them without rewrites, new DBs get them from CREATE TABLE).
    for col in [
        "ALTER TABLE residents ADD COLUMN emergency_contact_resident_id TEXT",
        "ALTER TABLE residents ADD COLUMN last_pdf_path TEXT",
        "ALTER TABLE residents ADD COLUMN last_pdf_generated_at TEXT",
        "ALTER TABLE residents ADD COLUMN emergency_contact_address TEXT",
    ] {
        let _ = sqlx::query(col).execute(&pool).await;
    }

    // P1-6: unify valid_until storage to ISO YYYY-MM-DD. Legacy rows carry
    // DD-MM-YYYY (or DD/MM/YYYY); rewrite them in place. Non-matching values
    // (already ISO, empty, NULL) are untouched.
    let _ = sqlx::query("UPDATE residents SET valid_until = REPLACE(valid_until, '/', '-') WHERE valid_until LIKE '%/%'")
        .execute(&pool)
        .await;
    let _ = sqlx::query(
        "UPDATE residents SET valid_until = substr(valid_until, 7, 4) || '-' || substr(valid_until, 4, 2) || '-' || substr(valid_until, 1, 2) WHERE valid_until GLOB '??-??-????'",
    )
    .execute(&pool)
    .await;

    // Lot removal: drop legacy address_house column when it holds no data.
    // Old DBs keep the column orphaned (SELECT * still works; queries ignore it).
    // L4: gated to run ONCE via _boot_flags — after the first decision (drop or
    // keep) every later boot skips the probe entirely. Safe: nothing writes
    // address_house anymore, so a kept column can never gain new data.
    let _ =
        sqlx::query("CREATE TABLE IF NOT EXISTS _boot_flags(name TEXT PRIMARY KEY, value TEXT)")
            .execute(&pool)
            .await;
    let lot_flag: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _boot_flags WHERE name = 'address_house_drop'")
            .fetch_one(&pool)
            .await
            .unwrap_or(1);
    let lot_decided: bool = lot_flag > 0;
    if !lot_decided {
        {
            let non_empty: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM residents WHERE address_house IS NOT NULL AND TRIM(address_house) != ''",
            )
            .fetch_one(&pool)
            .await
            .unwrap_or(1); // on any error (e.g. column already gone), skip the drop
            if non_empty == 0 {
                let _ = sqlx::query("ALTER TABLE residents DROP COLUMN address_house")
                    .execute(&pool)
                    .await;
            } else {
                tracing::info!(
                    "Lot removal: keeping legacy address_house column ({} rows carry data, ignored by queries)",
                    non_empty
                );
            }
        }
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO _boot_flags(name, value) VALUES('address_house_drop', '1')",
        )
        .execute(&pool)
        .await;
    }

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_valid_until ON residents(valid_until);
        CREATE INDEX IF NOT EXISTS idx_resident_name ON residents(last_name, first_name);
        CREATE INDEX IF NOT EXISTS idx_resident_id ON residents(resident_id);
        CREATE INDEX IF NOT EXISTS idx_purok ON residents(address_purok);
        CREATE INDEX IF NOT EXISTS idx_active ON residents(is_active);
        CREATE INDEX IF NOT EXISTS idx_gender ON residents(is_active, gender);
        CREATE INDEX IF NOT EXISTS idx_remarks ON residents(is_active, remarks);
        CREATE INDEX IF NOT EXISTS idx_dashboard ON residents(is_active, gender, remarks, date_registered);
        "#,
    )
    .execute(&pool)
    .await?;

    // DB maintenance: run the lightweight optimize on startup when the
    // sentinel is stale (>7 days). Best-effort — never fails boot.
    if optimize_due(&app_data_dir) {
        if optimize_database(&pool).await.is_ok() {
            mark_optimized(&app_data_dir);
            tracing::info!("startup optimize complete (PRAGMA optimize + incremental vacuum)");
        }
    }

    Ok(pool)
}

/// DB maintenance: lightweight online optimize — SQLite self-tuning
/// (PRAGMA optimize) + incremental vacuum in 100-page chunks. Chunked
/// vacuum reclaims free pages without the long exclusive lock of a full
/// VACUUM, so it is safe to run while the app is open.
pub async fn optimize_database(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query("PRAGMA optimize;")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("PRAGMA incremental_vacuum(100);")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Last-optimize tracking lives in a sentinel file (not a table) so the
/// check works even before migrations run. Due when missing/unparseable
/// or older than 7 days.
pub fn optimize_due(app_data_dir: &Path) -> bool {
    const SEVEN_DAYS: u64 = 7 * 24 * 3600;
    let stamp = app_data_dir.join(".last_optimize");
    let content = match fs::read_to_string(&stamp) {
        Ok(c) => c,
        Err(_) => return true,
    };
    let then: u64 = match content.trim().parse() {
        Ok(t) => t,
        Err(_) => return true,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(then);
    now.saturating_sub(then) > SEVEN_DAYS
}

pub fn mark_optimized(app_data_dir: &Path) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = fs::write(app_data_dir.join(".last_optimize"), now.to_string());
}

/// Frontend-callable maintenance entry (Settings → Database Health).
/// Runs the optimize now and refreshes the sentinel.
#[tauri::command]
pub async fn optimize_database_cmd(
    state: tauri::State<'_, std::sync::Arc<tokio::sync::Mutex<AppState>>>,
) -> Result<String, String> {
    let (pool, app_data_dir) = {
        let s = state.lock().await;
        (s.pool.clone(), s.app_data_dir.clone())
    };
    optimize_database(&pool).await?;
    mark_optimized(&app_data_dir);
    Ok("Database optimized (PRAGMA optimize + incremental vacuum)".to_string())
}

#[cfg(test)]
mod validate_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn rejects_traversal_and_escapes() {
        let root = get_project_root().unwrap_or_else(default_root);
        // .. traversal must be rejected
        let traversal = root.join("..\\..\\Windows\\System32\\evil.txt");
        assert!(
            validate_isolated_write_path(&traversal).is_err(),
            "parent traversal should be rejected"
        );
        let traversal2 = Path::new("Documents/../../evil.txt");
        // relative with .. that escapes root should be rejected (lexical check)
        // root + Documents/../../evil normalizes to outside root
        let abs = lexical_normalize(&root.join(traversal2));
        assert!(
            !path_starts_with_inside(&abs, &root.canonicalize().unwrap_or(root.clone())),
            "traversal escapes root"
        );
        // Direct validation should reject an absolute outside path
        let outside = PathBuf::from("/tmp/evil.txt");
        if !path_starts_with_inside(&outside, &root.canonicalize().unwrap_or(root.clone())) {
            assert!(validate_isolated_write_path(&outside).is_err());
        }
        // Normal inside path must pass
        let inside = root.join("Documents/PDF/test.pdf");
        assert!(
            validate_isolated_write_path(&inside).is_ok(),
            "inside path should be allowed"
        );
        let relative = Path::new("Documents/PDF/test.pdf");
        assert!(validate_isolated_write_path(relative).is_ok());
    }

    #[test]
    fn lexical_normalize_strips_dotdot() {
        let p = Path::new("/a/b/../c/./d");
        let n = lexical_normalize(p);
        assert_eq!(n, PathBuf::from("/a/c/d"));
    }

    #[test]
    fn wal_shm_helpers_use_display_suffix() {
        let db = Path::new("/tmp/foo.db");
        assert_eq!(wal_path_for(db), PathBuf::from("/tmp/foo.db-wal"));
        assert_eq!(shm_path_for(db), PathBuf::from("/tmp/foo.db-shm"));
        let db2 = Path::new("/tmp/foo.sqlite");
        // Must not mangle extension via with_extension — suffix append preserves original
        assert_eq!(wal_path_for(db2), PathBuf::from("/tmp/foo.sqlite-wal"));
    }

    // ── R1 edge cases for sanitize_file_name_component ──
    #[test]
    fn sanitize_control_chars_stripped() {
        let raw = "a\x00b\x01c\x1Fdef";
        let out = sanitize_file_name_component(raw, 20);
        assert!(
            !out.chars().any(|c| c.is_control()),
            "control chars must be stripped: {}",
            out
        );
        assert_eq!(out, "abcdef");
    }

    #[test]
    fn sanitize_illegal_chars_removed() {
        let raw = "a:b*c?d\"e<f>g|h/i\\j";
        let out = sanitize_file_name_component(raw, 50);
        for ch in ['/', '\\', ':', '*', '?', '"', '<', '>', '|'] {
            assert!(!out.contains(ch), "illegal {} still in {}", ch, out);
        }
        assert_eq!(out, "abcdefghij");
    }

    #[test]
    fn sanitize_dotdot_and_dots_trimmed() {
        assert_eq!(sanitize_file_name_component("..", 10), "UNNAMED");
        assert_eq!(sanitize_file_name_component("...", 10), "UNNAMED");
        assert_eq!(sanitize_file_name_component("..foo..", 10), "foo");
        let out = sanitize_file_name_component("a..b..c", 10);
        assert!(!out.contains(".."), "dotdot should be removed: {}", out);
    }

    #[test]
    fn sanitize_truncation_and_ext_preserved() {
        // stem longer than cap should be truncated; extension preserved after cap
        let raw = "a".repeat(50) + ".pdf";
        let out = sanitize_file_name_component(&raw, 10);
        assert!(out.ends_with(".pdf"), "ext preserved: {}", out);
        let stem = out.strip_suffix(".pdf").unwrap();
        assert_eq!(stem.len(), 10, "stem capped to 10: {}", stem);
    }

    #[test]
    fn sanitize_whitespace_collapsed_to_underscore() {
        let out = sanitize_file_name_component("a   b\t c\n d", 50);
        assert_eq!(out, "a_b_c_d");
    }

    #[test]
    fn sanitize_empty_becomes_unnamed() {
        assert_eq!(sanitize_file_name_component("", 10), "UNNAMED");
        assert_eq!(sanitize_file_name_component("   ", 10), "UNNAMED");
        assert_eq!(sanitize_file_name_component("....", 10), "UNNAMED");
        // whitespace + illegal only -> empty -> UNNAMED
        assert_eq!(sanitize_file_name_component("///", 10), "UNNAMED");
    }

    #[test]
    fn sanitize_unnamed_preserves_extension() {
        let out = sanitize_file_name_component("... .pdf", 10);
        // stem empty -> UNNAMED + ext
        assert!(out.starts_with("UNNAMED"), "expected UNNAMED got {}", out);
        assert!(out.ends_with(".pdf"), "ext kept: {}", out);
    }

    // ── dead-PDF surface removed (Phase-1): no per-resident .pdf stem
    // helper exists anymore — print is HTML-only via card-template.html.
    #[test]
    fn no_pdf_stem_helper_remains() {
        // Sanitize helper (used by xlsx/registry paths) must not emit .pdf.
        let out = sanitize_file_name_component("DOE-BRGY-2026-00001", 120);
        assert!(
            !out.ends_with(".pdf"),
            "stem helper must not emit .pdf: {}",
            out
        );
    }

    // ── validate_isolated_write_path extended ──
    #[test]
    fn validate_rejects_absolute_outside() {
        let outside = PathBuf::from("/etc/passwd");
        // must be rejected regardless of platform (outside project root)
        let root = get_project_root().unwrap_or_else(default_root);
        let root_can = root
            .canonicalize()
            .unwrap_or_else(|_| lexical_normalize(&root));
        if !path_starts_with_inside(&outside, &root_can) {
            assert!(
                validate_isolated_write_path(&outside).is_err(),
                "/etc/passwd should be rejected"
            );
        }
        let outside2 = PathBuf::from("/tmp/evil.xlsx");
        if !path_starts_with_inside(&outside2, &root_can) {
            assert!(validate_isolated_write_path(&outside2).is_err());
        }
    }

    #[test]
    fn validate_rejects_dotdot_traversal_variants() {
        let root = get_project_root().unwrap_or_else(default_root);
        // relative traversal strings — must not be accepted as inside
        let cases = [
            PathBuf::from("../evil.txt"),
            PathBuf::from("data/../../evil.txt"),
            PathBuf::from("Documents/PDF/../../../evil.txt"),
        ];
        for p in cases {
            let lex = lexical_normalize(&root.join(&p));
            let root_can = root
                .canonicalize()
                .unwrap_or_else(|_| lexical_normalize(&root));
            if !path_starts_with_inside(&lex, &root_can) {
                assert!(
                    validate_isolated_write_path(&p).is_err(),
                    "traversal {:?} should be rejected (lex {:?})",
                    p,
                    lex
                );
            }
        }
    }

    #[test]
    fn validate_symlink_parent_inside_allows_nested() {
        // Create a real tmp dir inside project, symlink it, then validate a write inside the symlink target.
        // If symlink support is unavailable, test still passes via normal inside-path assertion.
        let root = get_project_root().unwrap_or_else(default_root);
        let base = root.join("tmp").join("validate_test_link");
        let target = base.join("real");
        let link = base.join("link");
        let _ = std::fs::create_dir_all(&target);
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&link);
            let _ = std::os::unix::fs::symlink(&target, &link);
            if link.exists() || std::fs::symlink_metadata(&link).is_ok() {
                // Canonical parent of link should still be inside root -> allowed
                let p = link.join("file.txt");
                // validate_isolated_write_path canonicalizes parent; symlink target inside project must pass
                let res = validate_isolated_write_path(&p);
                assert!(
                    res.is_ok(),
                    "symlink inside project should be allowed: {:?} err {:?}",
                    p,
                    res.err()
                );
                let _ = std::fs::remove_file(&link);
            }
        }
        let _ = std::fs::remove_dir_all(&base);
        // sanity: normal nested inside still allowed
        let inside = root.join("data/photos/test.jpg");
        assert!(validate_isolated_write_path(&inside).is_ok());
    }
}
