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
    /// Optional operator-editable template overrides (project-isolated paths).
    /// When set, resolved via `validate_isolated_write_path` against the
    /// project root — no absolute D:\\ escape, no traversal.
    #[serde(default)]
    pub card_template_front: Option<String>,
    #[serde(default)]
    pub card_template_back: Option<String>,
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
            card_template_front: None,
            card_template_back: None,
        }
    }
}

fn config_file_path() -> Option<PathBuf> {
    crate::db::get_project_root().map(|r| r.join("config").join("civic.json"))
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
                            if let Some(v) = o.get("republic").and_then(|v| v.as_str()) {
                                cfg.republic = v.to_string();
                            }
                            if let Some(v) = o.get("province_line").and_then(|v| v.as_str()) {
                                cfg.province_line = v.to_string();
                            }
                            if let Some(v) = o.get("barangay_full").and_then(|v| v.as_str()) {
                                cfg.barangay_full = v.to_string();
                            }
                            if let Some(v) = o.get("barangay_short").and_then(|v| v.as_str()) {
                                cfg.barangay_short = v.to_string();
                            }
                            if let Some(v) = o.get("city_line").and_then(|v| v.as_str()) {
                                cfg.city_line = v.to_string();
                            }
                            if let Some(v) = o.get("card_title").and_then(|v| v.as_str()) {
                                cfg.card_title = v.to_string();
                            }
                            if let Some(v) = o.get("punong_name").and_then(|v| v.as_str()) {
                                cfg.punong_name = v.to_string();
                            }
                            if let Some(v) = o.get("punong_title").and_then(|v| v.as_str()) {
                                cfg.punong_title = v.to_string();
                            }
                            if let Some(v) = o.get("card_template_front").and_then(|v| v.as_str()) {
                                let t = v.trim();
                                if !t.is_empty() {
                                    cfg.card_template_front = Some(t.to_string());
                                }
                            }
                            if let Some(v) = o.get("card_template_back").and_then(|v| v.as_str()) {
                                let t = v.trim();
                                if !t.is_empty() {
                                    cfg.card_template_back = Some(t.to_string());
                                }
                            }
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
    if b.eq_ignore_ascii_case(&cfg.barangay_full) || b.eq_ignore_ascii_case(&cfg.barangay_short) {
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

    // Apply partial updates (only known fields)
    if let Some(o) = partial.as_object() {
        if let Some(v) = o.get("republic").and_then(|v| v.as_str()) {
            cfg.republic = v.to_string();
        }
        if let Some(v) = o.get("province_line").and_then(|v| v.as_str()) {
            cfg.province_line = v.to_string();
        }
        if let Some(v) = o.get("barangay_full").and_then(|v| v.as_str()) {
            cfg.barangay_full = v.to_string();
        }
        if let Some(v) = o.get("barangay_short").and_then(|v| v.as_str()) {
            cfg.barangay_short = v.to_string();
        }
        if let Some(v) = o.get("city_line").and_then(|v| v.as_str()) {
            cfg.city_line = v.to_string();
        }
        if let Some(v) = o.get("card_title").and_then(|v| v.as_str()) {
            cfg.card_title = v.to_string();
        }
        if let Some(v) = o.get("punong_name").and_then(|v| v.as_str()) {
            cfg.punong_name = v.to_string();
        }
        if let Some(v) = o.get("punong_title").and_then(|v| v.as_str()) {
            cfg.punong_title = v.to_string();
        }
        if let Some(v) = o.get("card_template_front").and_then(|v| v.as_str()) {
            let t = v.trim();
            if !t.is_empty() {
                cfg.card_template_front = Some(t.to_string());
            }
        }
        if let Some(v) = o.get("card_template_back").and_then(|v| v.as_str()) {
            let t = v.trim();
            if !t.is_empty() {
                cfg.card_template_back = Some(t.to_string());
            }
        }
    }

    // Validate size
    const MAX_CIVIC_JSON: usize = 64 * 1024;
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| format!("Serialization failed: {}", e))?;
    if json.len() > MAX_CIVIC_JSON {
        return Err(format!("Config too large ({} bytes > {})", json.len(), MAX_CIVIC_JSON));
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