//! F15: Centralized civic/officer configuration.
//! All civic strings previously hardcoded across id_card.rs / index.html / app.js
//! now resolve through this single module. Values come from (in priority order):
//! 1. `<project-root>/config/civic.json` (operator-editable, isolated inside project)
//! 2. Built-in defaults below.
//! Frontend mirrors these via `settings` (barangayName/cityName/provinceName/punongName)
//! persisted in localStorage; backend remains source of truth for PDF rendering.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CivicConfig {
    pub republic: String,
    pub province_line: String,
    pub barangay_full: String,
    pub barangay_short: String,
    pub city_line: String,
    pub card_title: String,
    pub punong_name: String,
    pub punong_title: String,
    pub emergency_defaults: EmergencyDefaults,
    /// When true, scheduled backup runs photo_gc in dry-run mode (scan + log
    /// only, no deletions). Operators who prefer manual-only GC can set this
    /// in config/civic.json; default false keeps scheduled auto-delete.
    #[serde(default)]
    pub photo_gc_dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyDefaults {
    pub name: String,
    pub relation: String,
    pub contact: String,
}

impl Default for CivicConfig {
    fn default() -> Self {
        Self {
            republic: "Republic of the Philippines".to_string(),
            province_line: "Province of Samar • Municipality of Basey".to_string(),
            barangay_full: "BARANGAY (NOUVELAS OCCIDENTAL) BALUD".to_string(),
            barangay_short: "BALUD".to_string(),
            city_line: "Basey, Samar".to_string(),
            card_title: "BARANGAY RESIDENT'S CARD".to_string(),
            punong_name: "RAYMUND ROSILLER R. DE VEYRA".to_string(),
            punong_title: "PUNONG BARANGAY".to_string(),
            emergency_defaults: EmergencyDefaults {
                name: "JUANNA DELA CRUZ".to_string(),
                relation: "SISTER".to_string(),
                contact: "09987654321".to_string(),
            },
            photo_gc_dry_run: false,
        }
    }
}

fn config_file_path() -> Option<PathBuf> {
    crate::db::get_project_root().map(|r| r.join("config").join("civic.json"))
}

/// P17 content validation for operator-editable civic text: max 2000 chars,
/// no HTML markup (`<`/`>` rejected so stored strings can never inject tags
/// into the card template or settings UI).
fn validate_civic_text(field: &str, v: &str) -> Result<(), String> {
    if v.chars().count() > 2000 {
        return Err(format!(
            "{} too long ({} chars > 2000)",
            field,
            v.chars().count()
        ));
    }
    if v.contains('<') || v.contains('>') {
        return Err(format!(
            "{} must not contain HTML markup ('<' / '>')",
            field
        ));
    }
    Ok(())
}

/// Load operator overrides from `<root>/config/civic.json` (partial JSON merged
/// over defaults). Never panics; missing/invalid file => defaults.
pub fn load_civic_config() -> CivicConfig {
    let mut cfg = CivicConfig::default();
    if let Some(path) = config_file_path() {
        match fs::read(&path) {
            Ok(bytes) => {
                const MAX_CIVIC_JSON: usize = 64 * 1024;
                if bytes.len() > MAX_CIVIC_JSON {
                    tracing::warn!(
                        "civic.json too large ({} bytes > {}), using defaults: {}",
                        bytes.len(),
                        MAX_CIVIC_JSON,
                        path.display()
                    );
                    return cfg;
                }
                match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(partial) => {
                        if let Some(o) = partial.as_object() {
                            // P17: each text field validated (2000 chars, no HTML);
                            // invalid file values keep defaults with a warning.
                            let take = |field: &str, dst: &mut String| {
                                if let Some(v) = o.get(field).and_then(|v| v.as_str()) {
                                    match validate_civic_text(field, v) {
                                        Ok(()) => *dst = v.to_string(),
                                        Err(e) => tracing::warn!(
                                            "civic.json field rejected ({}), keeping default: {}",
                                            e,
                                            path.display()
                                        ),
                                    }
                                }
                            };
                            take("republic", &mut cfg.republic);
                            take("province_line", &mut cfg.province_line);
                            take("barangay_full", &mut cfg.barangay_full);
                            take("barangay_short", &mut cfg.barangay_short);
                            take("city_line", &mut cfg.city_line);
                            take("card_title", &mut cfg.card_title);
                            take("punong_name", &mut cfg.punong_name);
                            take("punong_title", &mut cfg.punong_title);
                            // photo_gc_dry_run is a bool; missing => false (delete on).
                            if let Some(v) = o.get("photo_gc_dry_run").and_then(|v| v.as_bool()) {
                                cfg.photo_gc_dry_run = v;
                            }
                            // Dead-PDF surface removed (Phase-1):
                            // template-override keys are no longer read —
                            // no render path ever consumed them.
                        } else {
                            tracing::warn!(
                                "civic.json is not a JSON object, using defaults: {}",
                                path.display()
                            );
                            eprintln!(
                                "Warning: civic.json is not a JSON object, using defaults: {}",
                                path.display()
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse civic.json ({}), using defaults: {}",
                            e,
                            path.display()
                        );
                        eprintln!(
                            "Warning: Failed to parse civic.json ({}), using defaults: {}",
                            e,
                            path.display()
                        );
                    }
                }
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                tracing::warn!(
                    "Failed to read civic.json ({}), using defaults: {}",
                    e,
                    path.display()
                );
                eprintln!(
                    "Warning: Failed to read civic.json ({}), using defaults: {}",
                    e,
                    path.display()
                );
            }
            Err(_) => {} // missing file is normal — use defaults silently
        }
    }
    cfg
}

/// Short barangay token for one-line addresses.
/// Prefers explicit config short name; falls back to last-token heuristic
/// for legacy free-text settings values.
pub fn short_barangay(barangay: &str, cfg: &CivicConfig) -> String {
    let b = barangay.trim();
    if b.is_empty() {
        return cfg.barangay_short.clone();
    }
    // Exact short-name hits return as-is. NOTE: never shortcut on
    // barangay_full here — the configured full name itself is paren-form
    // ("BARANGAY (NOUVELAS OCCIDENTAL) BALUD") and must compose to
    // "NOUVELAS OCCIDENTAL (BALUD)" like every other paren input (and like
    // the frontend offline fallback), not collapse to barangay_short.
    if b.eq_ignore_ascii_case(&cfg.barangay_short) {
        return cfg.barangay_short.clone();
    }
    // Paren-first: extract content inside the FIRST pair of parentheses:
    // "Barangay (Nouvelas Occidental) Balud" → "NOUVELAS OCCIDENTAL (BALUD)".
    // Falls back to the last-token heuristic when no (non-empty) parens.
    if let Some(open) = b.find('(') {
        if let Some(rel) = b[open + 1..].find(')') {
            let inner = b[open + 1..open + 1 + rel].trim();
            let after = b[open + 1 + rel + 1..].trim();
            if !inner.is_empty() {
                let upper = inner.to_uppercase();
                let short: String = upper.chars().take(40).collect();
                if !short.is_empty() {
                    if !after.is_empty() {
                        return format!("{} ({})", short, after.to_uppercase());
                    }
                    return short;
                }
            }
        }
    }
    // Fallback: return full string uppercased (preserves multi-word names like "Nouvelas Occidental")
    let upper = b.to_uppercase();
    let short: String = upper.chars().take(40).collect();
    if short.is_empty() {
        cfg.barangay_short.clone()
    } else {
        short
    }
}

#[tauri::command]
pub async fn get_civic_config() -> Result<CivicConfig, String> {
    Ok(load_civic_config())
}

/// P1-7: single server-side short-barangay implementation for frontend use.
/// Frontend calls this instead of its local BALUD-substring heuristic;
/// local heuristic remains only as offline fallback.
#[tauri::command]
pub async fn get_short_barangay(barangay: String) -> Result<String, String> {
    Ok(short_barangay(&barangay, &load_civic_config()))
}

/// Write operator-editable civic config to `<root>/config/civic.json`.
/// Partial update: only provided fields are changed; others keep current values.
/// Validates isolation (path must be inside project root) and size limit.
#[tauri::command]
pub async fn set_civic_config(partial: serde_json::Value) -> Result<CivicConfig, String> {
    let path = config_file_path().ok_or("Cannot resolve project root for config path")?;

    // Ensure config directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }

    // Load current config as base
    let mut cfg = load_civic_config();

    // Apply partial updates (only known fields). P17: each text value is
    // content-validated (2000 chars, no HTML).
    if let Some(o) = partial.as_object() {
        let take = |field: &str, dst: &mut String| -> Result<(), String> {
            if let Some(v) = o.get(field).and_then(|v| v.as_str()) {
                validate_civic_text(field, v)?;
                *dst = v.to_string();
            }
            Ok(())
        };
        take("republic", &mut cfg.republic)?;
        take("province_line", &mut cfg.province_line)?;
        take("barangay_full", &mut cfg.barangay_full)?;
        take("barangay_short", &mut cfg.barangay_short)?;
        take("city_line", &mut cfg.city_line)?;
        take("card_title", &mut cfg.card_title)?;
        take("punong_name", &mut cfg.punong_name)?;
        take("punong_title", &mut cfg.punong_title)?;
        // photo_gc_dry_run: bool toggle — operators can enable dry-run mode
        // via Settings/UI so scheduled backup only scans orphans without deleting.
        if let Some(v) = o.get("photo_gc_dry_run").and_then(|v| v.as_bool()) {
            cfg.photo_gc_dry_run = v;
        }
    }

    // Validate size
    const MAX_CIVIC_JSON: usize = 64 * 1024;
    let json =
        serde_json::to_string_pretty(&cfg).map_err(|e| format!("Serialization failed: {}", e))?;
    if json.len() > MAX_CIVIC_JSON {
        return Err(format!(
            "Config too large ({} bytes > {})",
            json.len(),
            MAX_CIVIC_JSON
        ));
    }

    // Validate isolation before write
    crate::db::validate_isolated_write_path(&path)
        .map_err(|e| format!("Config path isolation error: {}", e))?;

    // Atomic write: write to temp then rename
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("Write failed: {}", e))?;
    fs::rename(&tmp_path, &path).map_err(|e| format!("Rename failed: {}", e))?;

    tracing::info!("civic.json updated at {}", path.display());
    Ok(cfg)
}

#[tauri::command]
pub async fn open_path(path: String) -> Result<bool, String> {
    use std::process::Command;
    let p = std::path::PathBuf::from(&path);

    // Validate isolation
    crate::db::validate_isolated_write_path(&p)
        .map_err(|e| format!("Path isolation error: {}", e))?;

    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .args(["/select,", &path])
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| format!("Failed to open finder: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        // Try xdg-open on parent directory (select not widely supported)
        let parent = p.parent().unwrap_or(&p);
        Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {}", e))?;
    }

    Ok(true)
}

#[cfg(test)]
mod short_barangay_tests {
    use super::*;

    // P4/P12 parity: paren-first "INNER (AFTER)", else full-string fallback
    // (mirrors frontend shortBarangay/barangayCardDisplay in app.js).
    #[test]
    fn short_barangay_paren_first_and_full_string_fallback() {
        let cfg = CivicConfig::default();
        assert_eq!(
            short_barangay("Barangay (Nouvelas Occidental) Balud", &cfg),
            "NOUVELAS OCCIDENTAL (BALUD)"
        );
        assert_eq!(
            short_barangay("Barangay (Nouvelas Occidental)", &cfg),
            "NOUVELAS OCCIDENTAL"
        );
        // No parens → full string uppercased (never last-token).
        assert_eq!(short_barangay("Barangay Balud", &cfg), "BARANGAY BALUD");
        assert_eq!(short_barangay("Balud", &cfg), "BALUD");
        assert_eq!(short_barangay("BARANGAY", &cfg), "BARANGAY");
        assert_eq!(short_barangay("", &cfg), "BALUD");
    }

    // P17: civic text validation rejects HTML + over-long values.
    #[test]
    fn civic_text_rejects_html_and_overlong() {
        assert!(validate_civic_text("punong_name", "JUAN DELA CRUZ").is_ok());
        assert!(validate_civic_text("punong_name", "<script>alert(1)</script>").is_err());
        assert!(validate_civic_text("republic", "a > b").is_err());
        assert!(validate_civic_text("city_line", &"x".repeat(2001)).is_err());
        assert!(validate_civic_text("city_line", &"x".repeat(2000)).is_ok());
    }

    // P2-7: photo_gc_dry_run defaults false, parses from JSON, round-trips.
    #[test]
    fn photo_gc_dry_run_default_and_json() {
        // Default: false (scheduled backup deletes orphans).
        assert!(!CivicConfig::default().photo_gc_dry_run);

        // Missing field in JSON => false.
        let json = r#"{"republic":"Test"}"#;
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        let mut cfg = CivicConfig::default();
        if let Some(o) = v.as_object() {
            if let Some(val) = o.get("photo_gc_dry_run").and_then(|v| v.as_bool()) {
                cfg.photo_gc_dry_run = val;
            }
        }
        assert!(!cfg.photo_gc_dry_run);

        // Explicit true => true.
        let json = r#"{"photo_gc_dry_run": true}"#;
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        let mut cfg = CivicConfig::default();
        if let Some(o) = v.as_object() {
            if let Some(val) = o.get("photo_gc_dry_run").and_then(|v| v.as_bool()) {
                cfg.photo_gc_dry_run = val;
            }
        }
        assert!(cfg.photo_gc_dry_run);

        // Round-trip serialization preserves the bool.
        let mut cfg = CivicConfig::default();
        cfg.photo_gc_dry_run = true;
        let s = serde_json::to_string(&cfg).unwrap();
        let back: CivicConfig = serde_json::from_str(&s).unwrap();
        assert!(back.photo_gc_dry_run);
    }
}
