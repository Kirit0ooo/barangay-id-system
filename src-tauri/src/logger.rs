use std::fs;
use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Last-resort logs dir when no project location resolves (OS temp). Used
/// only after error logging — normal runs always resolve inside the project.
fn fallback_logs_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("barangay-id-logs");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Returns the isolated logs directory inside the project.
/// All logs MUST stay inside D:\Barangay\barangay-id-system\logs
/// NEVER inside src-tauri (watched by Tauri) -> prevents rebuild loop
/// P2-9: returns Result instead of panicking — callers fall back to the OS
/// temp dir with an error log when nothing project-local resolves.
pub fn get_logs_dir() -> Result<PathBuf, String> {
    // Isolation: prefer <resolved-project-root>/logs so logs always agree
    // with get_project_root(). Fixed D:\ candidates come LAST with an
    // existence-check only — never create_dir_all while probing them, or a
    // non-Windows host would mint a literal `D:\...` junk dir and escape
    // isolation (same bug class as fixed in db.rs get_project_root).
    if let Some(root) = crate::db::get_project_root() {
        let candidate = root.join("logs");
        if fs::create_dir_all(&candidate).is_ok() {
            return Ok(candidate);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.clone();
        for _ in 0..6 {
            if dir.join("barangay.ico").exists() || dir.join("src-tauri").exists() {
                let candidate = dir.join("logs");
                if fs::create_dir_all(&candidate).is_ok() {
                    return Ok(candidate);
                }
            }
            if let Some(parent) = dir.parent() {
                dir = parent.to_path_buf();
            } else {
                break;
            }
        }
    }
    // Fixed candidates last, existence-check only — never create.
    for candidate in [
        PathBuf::from(r"D:\Barangay\barangay-id-system\logs"),
        PathBuf::from("/mnt/d/Barangay/barangay-id-system/logs"),
    ] {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("Isolation violation: cannot resolve project logs dir. Ensure the program is run from D:\\Barangay\\barangay-id-system.".into())
}

/// M5: cap daily `app.log.YYYY-MM-DD` retention — keep the 30 newest
/// rotated files, never delete today's active log. Runs at startup.
/// Best-effort: any I/O error is logged, never panics.
fn prune_old_daily_logs(logs_dir: &std::path::Path) {
    const KEEP: usize = 30;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let today_name = format!("app.log.{}", today);
    let entries = match fs::read_dir(logs_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut rotated: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("app.log.") && name != today_name && entry.path().is_file() {
            rotated.push((name, entry.path()));
        }
    }
    if rotated.len() <= KEEP {
        return;
    }
    // Lexical sort works: suffixes are zero-padded YYYY-MM-DD dates.
    rotated.sort_by(|a, b| a.0.cmp(&b.0));
    let drop_n = rotated.len() - KEEP;
    let mut deleted = 0usize;
    for (_, path) in rotated.into_iter().take(drop_n) {
        // Isolation: only delete inside the resolved logs dir.
        if crate::db::validate_isolated_write_path(&path).is_ok() {
            if fs::remove_file(&path).is_ok() {
                deleted += 1;
            }
        }
    }
    tracing::info!(
        "log retention: pruned {} old daily log(s), keeping newest {}",
        deleted,
        KEEP
    );
}

/// P2-6: prune launcher transcripts — keep the 20 newest run-*.log files.
/// Runs at startup alongside daily-log retention. Best-effort, never panics.
fn prune_run_logs(logs_dir: &std::path::Path) {
    crate::db::prune_oldest_files(logs_dir, &["run-"], 20);
}

/// Initialize logging to both console and isolated file.
/// Returns WorkerGuard that must be kept alive for the program lifetime.
pub fn init() -> WorkerGuard {
    // P2-9: fall back to OS-temp logs dir (with an error log) instead of panicking.
    let logs_dir = match get_logs_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("logging: {} — using temp fallback", e);
            let fb = fallback_logs_dir();
            eprintln!("logging: fallback dir {}", fb.display());
            fb
        }
    };
    fs::create_dir_all(&logs_dir).ok();
    // P2-6: bound launcher transcript retention.
    prune_run_logs(&logs_dir);
    // M5: bound daily rotation retention (keeps 30 newest, never today).
    prune_old_daily_logs(&logs_dir);

    // File appender: logs/app.log with daily rotation, isolated
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "app.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Filter: INFO by default, override with RUST_LOG env
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // Console layer
    let console_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_ansi(true);

    // File layer (no ansi, more detail)
    let file_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_ansi(false)
        .with_writer(non_blocking);

    // Try to init, ignore if already initialized (e.g., tests)
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .try_init();

    tracing::info!("Logging initialized - logs dir: {}", logs_dir.display());
    tracing::info!("Isolation: all logs stay inside D:\\Barangay\\barangay-id-system\\logs");

    guard
}

/// Helper to append a frontend log line to the isolated logs file.
/// Called via Tauri invoke from JS. Rotates when >5MB (keeps 3 generations).
pub fn append_frontend_log(level: &str, message: &str) {
    // P2-9: unresolvable logs dir → skip the file write, still emit via tracing.
    let logs_dir = match get_logs_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("frontend log skipped ({}): {}", e, message);
            match level {
                "error" => tracing::error!("[frontend] {}", message),
                "warn" => tracing::warn!("[frontend] {}", message),
                _ => tracing::info!("[frontend] {}", message),
            }
            return;
        }
    };
    let path = logs_dir.join("frontend.log");
    // L-7: 5MB rotation — keep frontend.log bounded
    const MAX: u64 = 5 * 1024 * 1024;
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > MAX {
            // Rotate: .3 -> delete, .2 -> .3, .1 -> .2, current -> .1
            let _ = fs::remove_file(logs_dir.join("frontend.log.3"));
            let _ = fs::rename(
                logs_dir.join("frontend.log.2"),
                logs_dir.join("frontend.log.3"),
            );
            let _ = fs::rename(
                logs_dir.join("frontend.log.1"),
                logs_dir.join("frontend.log.2"),
            );
            let _ = fs::rename(&path, logs_dir.join("frontend.log.1"));
            tracing::info!("frontend.log rotated (>5MB)");
        }
    }
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    // M5: single-line log discipline — strip CR/LF so one invoke call can
    // never forge extra log lines; cap at 8000 chars so a single message
    // cannot blow past the 5MB rotation check between writes.
    let clean_level: String = level
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(16)
        .collect();
    let mut clean_msg = message.replace('\r', " ").replace('\n', " ");
    if clean_msg.len() > 8000 {
        clean_msg.truncate(8000);
    }
    let line = format!(
        "[{}] [frontend] [{}] {}\n",
        now,
        clean_level.to_uppercase(),
        clean_msg
    );
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
    }
    match level {
        "error" => tracing::error!("[frontend] {}", clean_msg),
        "warn" => tracing::warn!("[frontend] {}", clean_msg),
        _ => tracing::info!("[frontend] {}", clean_msg),
    }
}
