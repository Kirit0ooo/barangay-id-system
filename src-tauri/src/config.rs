//! F15: Centralized civic/officer configuration.
//! All civic strings previously hardcoded across id_card.rs / index.html / app.js
//! now resolve through this single module. Values come from (in priority order):
//! 1. `<project-root>/config/civic.json` (operator-editable, isolated inside project)
//! 2. Built-in defaults below.
//! Frontend mirrors these via `settings` (barangayName/cityName/provinceName/punongName)
//! persisted in localStorage; backend remains source of truth for PDF rendering.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// P15 minimal-QR toggle: when true the card QR carries only
    /// `{id, valid, status}` + HMAC-SHA256 `sig` (see id_card.rs).
    #[serde(default)]
    pub qr_minimal_mode: bool,
    /// P15 HMAC secret for minimal-QR signatures. Effective value resolves
    /// in `load_civic_config()`: the gitignored sidecar
    /// `config/qr_hmac_secret` wins; the embedded `civic.json` value is a
    /// legacy fallback only (never written by `rotate`/`set` anymore).
    /// Never hardcoded, never logged.
    /// Generated via `rotate_qr_hmac_secret` (32 random bytes, hex).
    #[serde(default)]
    pub qr_hmac_secret: Option<String>,
    /// Verification portal base URL for signed URL-QRs, e.g.
    /// `https://verify.example.org/v`. Operator-set once the static portal
    /// (`verify/` in this repo) is hosted; QRs encode `{base}#{payload}`.
    /// None (default) = fail closed: signing refuses until configured, so
    /// the program never mints QRs pointing at a placeholder domain.
    #[serde(default)]
    pub verify_base_url: Option<String>,
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
            qr_minimal_mode: false,
            qr_hmac_secret: None,
            verify_base_url: None,
        }
    }
}

fn config_file_path() -> Option<PathBuf> {
    crate::db::get_project_root().map(|r| r.join("config").join("civic.json"))
}

/// Project-isolated sidecar holding ONLY the QR HMAC secret (raw hex).
/// Gitignored (see .gitignore) so the secret never lands in version control;
/// `civic.json` stays tracked for the operator-editable civic text.
/// Returns None when the file is missing or malformed — callers fall back
/// to the legacy embedded `civic.json` value, if any.
fn qr_secret_file_path() -> Option<PathBuf> {
    crate::db::get_project_root().map(|r| r.join("config").join("qr_hmac_secret"))
}

fn valid_qr_secret(s: &str) -> bool {
    let t = s.trim();
    t.len() >= 64 && t.len() <= 128 && t.chars().all(|c| c.is_ascii_hexdigit())
}

fn load_qr_secret_sidecar() -> Option<String> {
    let path = qr_secret_file_path()?;
    let raw = fs::read_to_string(&path).ok()?;
    // Bound the read: a real secret is 64 hex chars; anything huge is junk.
    if raw.len() > 1024 {
        tracing::warn!(
            "qr_hmac_secret sidecar too large, ignored: {}",
            path.display()
        );
        return None;
    }
    let t = raw.trim().to_string();
    if valid_qr_secret(&t) {
        Some(t)
    } else {
        if !t.is_empty() {
            tracing::warn!(
                "qr_hmac_secret sidecar malformed, ignored: {}",
                path.display()
            );
        }
        None
    }
}

fn sidecar_secret_present() -> bool {
    load_qr_secret_sidecar().is_some()
}

/// Pure decision: provision a secret only when minimal mode is on and no
/// secret exists anywhere yet (sidecar or legacy embedded). Unit-tested.
fn needs_qr_secret_provision(qr_minimal_mode: bool, effective_secret: Option<&str>) -> bool {
    qr_minimal_mode
        && effective_secret
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
}

/// Persist `secret` to the gitignored sidecar (atomic tmp + rename,
/// isolation-validated). Shared by rotate + first-enable auto-provision.
fn write_qr_secret_sidecar(secret: &str) -> Result<(), String> {
    let secret_path = qr_secret_file_path().ok_or("Cannot resolve project root for config path")?;
    if let Some(parent) = secret_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    crate::db::validate_isolated_write_path(&secret_path)
        .map_err(|e| format!("Secret path isolation error: {}", e))?;
    let secret_tmp = secret_path.with_extension("tmp");
    fs::write(&secret_tmp, secret).map_err(|e| format!("Secret write failed: {}", e))?;
    fs::rename(&secret_tmp, &secret_path).map_err(|e| format!("Secret rename failed: {}", e))?;
    Ok(())
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

/// Public half of a verification signing key (publishable by design —
/// verifiers and the static portal embed these; forgery still needs the
/// private seed, which never leaves `config/id_verify_private`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyKey {
    /// 64-hex Ed25519 public key.
    pub public: String,
    /// Local ISO creation time (`YYYY-MM-DD HH:MM:SS`).
    pub created: String,
}

/// Publishable trust store (`config/id_verify_trust.json`, TRACKED).
/// Old public keys are never removed on rotation so previously printed
/// cards keep verifying; `active_kid` selects the signing key.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerifyTrust {
    #[serde(default)]
    pub active_kid: Option<String>,
    #[serde(default)]
    pub keys: BTreeMap<String, VerifyKey>,
}

/// Info returned to the operator (never includes private material).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyKeyInfo {
    pub kid: String,
    pub public: String,
}

fn verify_trust_path() -> Option<PathBuf> {
    crate::db::get_project_root().map(|r| r.join("config").join("id_verify_trust.json"))
}

fn verify_private_path() -> Option<PathBuf> {
    crate::db::get_project_root().map(|r| r.join("config").join("id_verify_private"))
}

/// kid = first 8 hex chars of the public key. Stable per key; listed in
/// the trust store so rotation never invalidates old cards.
pub fn verify_kid_for_public(public_hex: &str) -> String {
    public_hex.chars().take(8).collect()
}

fn verify_now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Load the publishable trust store. Never panics; missing/invalid => empty.
pub fn load_verify_trust() -> VerifyTrust {
    if let Some(path) = verify_trust_path() {
        if let Ok(bytes) = fs::read(&path) {
            if bytes.len() <= 64 * 1024 {
                if let Ok(t) = serde_json::from_slice::<VerifyTrust>(&bytes) {
                    return t;
                }
            }
        }
    }
    VerifyTrust::default()
}

fn save_verify_trust(t: &VerifyTrust) -> Result<(), String> {
    let path = verify_trust_path().ok_or("Cannot resolve project root for config path")?;
    const MAX_TRUST_JSON: usize = 64 * 1024;
    let json =
        serde_json::to_string_pretty(t).map_err(|e| format!("Serialization failed: {}", e))?;
    if json.len() > MAX_TRUST_JSON {
        return Err(format!(
            "Trust store too large ({} bytes > {})",
            json.len(),
            MAX_TRUST_JSON
        ));
    }
    crate::db::validate_isolated_write_path(&path)
        .map_err(|e| format!("Trust path isolation error: {}", e))?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("Write failed: {}", e))?;
    fs::rename(&tmp_path, &path).map_err(|e| format!("Rename failed: {}", e))?;
    Ok(())
}

/// Record a public key in the trust store (keeps old keys) and mark it active.
fn upsert_trust_key(kid: &str, public_hex: &str) -> Result<(), String> {
    let mut trust = load_verify_trust();
    trust.keys.entry(kid.to_string()).or_insert(VerifyKey {
        public: public_hex.to_string(),
        created: verify_now_iso(),
    });
    // If the kid somehow reappears with different bytes, keep the first
    // published value (verifiers may have pinned it) and just re-activate.
    trust.active_kid = Some(kid.to_string());
    save_verify_trust(&trust)
}

/// Ensure an active Ed25519 signing keypair exists (auto-provision on first
/// use, same rationale as the HMAC auto-provision). Returns (kid, seed).
/// Private material is never logged.
pub fn ensure_verify_keypair() -> Result<(String, [u8; 32]), String> {
    use rand::RngCore;
    if let Some(path) = verify_private_path() {
        if let Ok(raw) = fs::read_to_string(&path) {
            let t = raw.trim();
            if t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()) {
                let v = hex::decode(t).map_err(|e| e.to_string())?;
                let seed: [u8; 32] = v
                    .try_into()
                    .map_err(|_| "id_verify_private bad length".to_string())?;
                let public = ed25519_dalek::SigningKey::from_bytes(&seed)
                    .verifying_key()
                    .to_bytes();
                let kid = verify_kid_for_public(&hex::encode(public));
                // Self-heal: re-list the key if the trust file was lost.
                upsert_trust_key(&kid, &hex::encode(public))?;
                return Ok((kid, seed));
            }
            if !t.is_empty() {
                tracing::warn!("id_verify_private malformed — provisioning fresh keypair");
            }
        }
    }
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    write_verify_private(&seed)?;
    let public = ed25519_dalek::SigningKey::from_bytes(&seed)
        .verifying_key()
        .to_bytes();
    let public_hex = hex::encode(public);
    let kid = verify_kid_for_public(&public_hex);
    upsert_trust_key(&kid, &public_hex)?;
    tracing::info!("verification signing keypair provisioned (kid {})", kid);
    Ok((kid, seed))
}

/// Persist a private seed to the gitignored sidecar (atomic tmp + rename).
fn write_verify_private(seed: &[u8; 32]) -> Result<(), String> {
    let path = verify_private_path().ok_or("Cannot resolve project root for config path")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    crate::db::validate_isolated_write_path(&path)
        .map_err(|e| format!("Private key path isolation error: {}", e))?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, hex::encode(seed)).map_err(|e| format!("Write failed: {}", e))?;
    fs::rename(&tmp_path, &path).map_err(|e| format!("Rename failed: {}", e))?;
    Ok(())
}

/// Generate a fresh signing keypair, archive the previous public key in the
/// trust store (old cards keep verifying), and activate the new kid.
/// Returns the new public half only — private material never leaves.
#[tauri::command]
pub async fn rotate_verify_key() -> Result<VerifyKeyInfo, String> {
    use rand::RngCore;
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    write_verify_private(&seed)?;
    let public = ed25519_dalek::SigningKey::from_bytes(&seed)
        .verifying_key()
        .to_bytes();
    let public_hex = hex::encode(public);
    let kid = verify_kid_for_public(&public_hex);
    upsert_trust_key(&kid, &public_hex)?;
    tracing::info!("verification signing key rotated (kid {})", kid);
    Ok(VerifyKeyInfo {
        kid,
        public: public_hex,
    })
}

/// Publishable trust store for the Settings UI and portal deployment.
#[tauri::command]
pub async fn get_verify_trust() -> Result<VerifyTrust, String> {
    Ok(load_verify_trust())
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
                            // Dead-PDF surface removed (Phase-1):
                            // template-override keys are no longer read —
                            // no render path ever consumed them.
                            // P15 minimal-QR toggle + legacy embedded secret.
                            // The embedded secret is a backward-compat fallback
                            // only; the gitignored sidecar wins (see below).
                            if let Some(v) = o.get("qr_minimal_mode").and_then(|v| v.as_bool()) {
                                cfg.qr_minimal_mode = v;
                            }
                            // Verification portal base URL (https only — phone
                            // cameras open it; the page handles PII-adjacent
                            // data). Warn-and-ignore on file load; set_civic_config
                            // rejects invalid values outright.
                            if let Some(v) = o.get("verify_base_url").and_then(|v| v.as_str()) {
                                let t = v.trim();
                                if !t.is_empty() {
                                    match validate_verify_base_url(t) {
                                        Ok(()) => cfg.verify_base_url = Some(t.to_string()),
                                        Err(e) => tracing::warn!(
                                            "civic.json verify_base_url rejected ({}), ignored: {}",
                                            e,
                                            path.display()
                                        ),
                                    }
                                }
                            }
                            if let Some(v) = o.get("qr_hmac_secret").and_then(|v| v.as_str()) {
                                let t = v.trim();
                                if !t.is_empty() {
                                    if valid_qr_secret(t) {
                                        cfg.qr_hmac_secret = Some(t.to_string());
                                    } else {
                                        tracing::warn!(
                                            "civic.json qr_hmac_secret malformed, ignored: {}",
                                            path.display()
                                        );
                                    }
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
    // Secret resolution: the gitignored `config/qr_hmac_secret` sidecar wins
    // over any legacy embedded `civic.json` value, so rotating/migrating the
    // secret never requires touching the tracked file.
    if let Some(sidecar) = load_qr_secret_sidecar() {
        cfg.qr_hmac_secret = Some(sidecar);
    }
    cfg
}

/// Portal URL rule: short, no HTML (inherits the civic-text rule), and
/// https-scheme only — phone cameras open it and the page renders
/// PII-adjacent record data, so plaintext http is refused.
fn validate_verify_base_url(v: &str) -> Result<(), String> {
    validate_civic_text("verify_base_url", v)?;
    if v.starts_with("https://") && v.len() > "https://".len() && !v.contains(' ') {
        Ok(())
    } else {
        Err("verify_base_url must start with https:// and contain no spaces".into())
    }
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
        if let Some(v) = o.get("qr_minimal_mode").and_then(|v| v.as_bool()) {
            cfg.qr_minimal_mode = v;
        }
        if let Some(v) = o.get("verify_base_url").and_then(|v| v.as_str()) {
            let t = v.trim();
            if t.is_empty() {
                cfg.verify_base_url = None;
            } else {
                validate_verify_base_url(t)?;
                cfg.verify_base_url = Some(t.to_string());
            }
        }
        // P15: the HMAC secret is never set directly — use rotate_qr_hmac_secret.
        if o.get("qr_hmac_secret").is_some() {
            tracing::warn!("set_civic_config: qr_hmac_secret ignored (use rotate_qr_hmac_secret)");
        }
    }

    // Validate size
    const MAX_CIVIC_JSON: usize = 64 * 1024;
    // Auto-provision on first enable: minimal mode with no secret anywhere
    // would HMAC with an empty key — forgeable by anyone who reads the docs.
    // Generate transparently (same routine as rotate); disabling minimal
    // mode later keeps the secret so old cards keep verifying.
    if needs_qr_secret_provision(cfg.qr_minimal_mode, cfg.qr_hmac_secret.as_deref()) {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let secret = hex::encode(bytes);
        write_qr_secret_sidecar(&secret)?;
        cfg.qr_hmac_secret = Some(secret);
        tracing::info!("qr_hmac_secret auto-generated on minimal-mode enable");
    }
    // Secret hygiene: when the gitignored sidecar holds the secret, never
    // re-embed it into the tracked civic.json (load_civic_config resolves
    // the effective value from the sidecar on every read).
    if sidecar_secret_present() {
        cfg.qr_hmac_secret = None;
    }
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

/// P15: generate a fresh 32-byte HMAC secret (hex), persist to the
/// gitignored `config/qr_hmac_secret` sidecar (never to tracked civic.json),
/// and strip any legacy embedded value. Returns the secret.
/// The secret is never logged — only its presence is traced.
#[tauri::command]
pub async fn rotate_qr_hmac_secret() -> Result<String, String> {
    use rand::RngCore;
    let path = config_file_path().ok_or("Cannot resolve project root for config path")?;
    let secret_path = qr_secret_file_path().ok_or("Cannot resolve project root for config path")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let mut cfg = load_civic_config();
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let secret = hex::encode(bytes);
    // Sidecar first, then strip any legacy embedded copy.
    write_qr_secret_sidecar(&secret)?;
    // Then strip any legacy embedded copy so the tracked file holds no secret.
    cfg.qr_hmac_secret = None;
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
    crate::db::validate_isolated_write_path(&path)
        .map_err(|e| format!("Config path isolation error: {}", e))?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("Write failed: {}", e))?;
    fs::rename(&tmp_path, &path).map_err(|e| format!("Rename failed: {}", e))?;
    tracing::info!("qr_hmac_secret rotated at {}", secret_path.display());
    Ok(secret)
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

    // Secret hygiene: sidecar values must be >= 64 hex chars (rotate writes
    // 64); short/non-hex/empty values are rejected so a corrupt sidecar can
    // never silently become the HMAC key. Also enforce a 128-char upper bound
    // to prevent DoS via oversized secrets written to disk.
    #[test]
    fn qr_secret_validation_accepts_only_hex() {
        assert!(valid_qr_secret(&"ab".repeat(32))); // exactly 64-hex (rotate output)
        assert!(!valid_qr_secret(&"ab".repeat(16))); // 32 chars < 64 minimum
        assert!(!valid_qr_secret("  abcd1234abcd1234abcd1234abcd1234  ")); // 32 chars after trim
        assert!(!valid_qr_secret(""));
        assert!(!valid_qr_secret("   "));
        assert!(!valid_qr_secret("short"));
        assert!(!valid_qr_secret(&"zz".repeat(32))); // 64 chars but non-hex
        assert!(!valid_qr_secret("abcd1234abcd1234abcd1234abcd123 <script>"));
        // >128 chars rejected
        assert!(!valid_qr_secret(&"ab".repeat(129)));
    }

    // Auto-provision matrix: only minimal-mode + no-secret-anywhere mints a
    // key. Anything else (mode off, or any existing secret incl. legacy
    // embedded) must not touch the sidecar — especially never on plain
    // civic-text saves while minimal mode is already on with a key.
    #[test]
    fn qr_provision_decision_matrix() {
        assert!(needs_qr_secret_provision(true, None));
        assert!(needs_qr_secret_provision(true, Some("")));
        assert!(needs_qr_secret_provision(true, Some("   ")));
        assert!(!needs_qr_secret_provision(
            true,
            Some("abcd1234abcd1234abcd1234abcd1234")
        ));
        assert!(!needs_qr_secret_provision(false, None));
        assert!(!needs_qr_secret_provision(
            false,
            Some("abcd1234abcd1234abcd1234abcd1234")
        ));
    }
}
