//! Encryption at rest for the Barangay ID System.
//!
//! Uses AES-256-GCM (authenticated encryption) with a 256-bit symmetric key
//! stored in the project-local `.crypto-key/key.bin`.
//!
//! Threat model: protects `data/residents.db` and `data/photos/*` from being
//! read by anyone who copies the files without also copying `.crypto-key/`.
//! Does NOT protect against an attacker who has both the key and the data
//! (the key file sits beside the data by operator request).
//!
//! Encrypted file format:
//!   [8-byte magic "BRGYENC1"][1-byte version=1][12-byte nonce][ciphertext + 16-byte GCM tag]
//!
//! The magic prefix lets callers distinguish encrypted files from plaintext
//! so corrupt/truncated ciphertext is never mistakenly passed through.
//!
//! PHOTO ENCRYPTION STRATEGY (in-place, no suffix rename):
//!   Photos are encrypted under their existing `photo_filename` (e.g. `abc.jpg`).
//!   `get_photo_base64` (id_card.rs) tries decrypt-first, falls back to
//!   plaintext — so `photo_basename`, `photo_gc`, `validate_import_photo`
//!   and all filename references keep working unchanged. No `.enc` suffix,
//!   no false-orphan trap for photo_gc.
//!
//! DB ENCRYPTION STRATEGY (startup decrypt / on-demand encrypt):
//!   At Tauri startup, before `init_db()`, if `residents.db.enc` exists it is
//!   decrypted to `residents.db` first (WAL/SHM sidecars decrypted too),
//!   then init proceeds normally. The DB runs plaintext while the app is open
//!   (SQLite needs it). The operator manually triggers encrypt via Settings;
//!   encrypt writes `.enc` files then calls `app.exit(0)` so the next launch
//!   decrypts before init_db.
//!
//! All file writes use tmp-then-rename (crash-safe, restartable).

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use std::fs;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

use crate::db;

const KEY_SIZE: usize = 32;
const NONCE_SIZE: usize = 12;
const TAG_SIZE: usize = 16;
const KEY_DIR_NAME: &str = ".crypto-key";
const KEY_FILE_NAME: &str = "key.bin";
// Magic prefix: 8 bytes + 1 version byte = 9-byte header before nonce.
const MAGIC: &[u8; 8] = b"BRGYENC1";
const VERSION: u8 = 1;
const HEADER_SIZE: usize = MAGIC.len() + 1; // 9

/// In-memory AES-256-GCM key, zeroized on drop.
pub struct CryptoKey(Zeroizing<[u8; KEY_SIZE]>);

impl CryptoKey {
    /// Encrypt a byte slice, returning magic+version+nonce+ciphertext+tag.
    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let cipher = Aes256Gcm::new_from_slice(&*self.0).expect("AES-256 key must be 32 bytes");
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .expect("AES-GCM encryption must not fail");
        let mut result = Vec::with_capacity(HEADER_SIZE + NONCE_SIZE + ciphertext.len());
        result.extend_from_slice(MAGIC);
        result.push(VERSION);
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        result
    }

    /// Decrypt a byte slice in magic+version+nonce+ciphertext+tag format.
    /// Returns Err if not our format (missing magic, wrong version, too
    /// short, or GCM auth failure).
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        if data.len() < HEADER_SIZE + NONCE_SIZE + TAG_SIZE {
            return Err("not an encrypted file (too short)".to_string());
        }
        if &data[..MAGIC.len()] != MAGIC {
            return Err("not an encrypted file (bad magic)".to_string());
        }
        if data[MAGIC.len()] != VERSION {
            return Err(format!(
                "unsupported encryption version {}",
                data[MAGIC.len()]
            ));
        }
        let cipher = Aes256Gcm::new_from_slice(&*self.0).expect("AES-256 key must be 32 bytes");
        let (nonce_bytes, ct_tag) = data.split_at(HEADER_SIZE + NONCE_SIZE);
        let nonce = Nonce::from_slice(&nonce_bytes[MAGIC.len() + 1..]);
        let ct_and_tag = ct_tag;
        cipher
            .decrypt(nonce, ct_and_tag)
            .map_err(|e| format!("decryption failed: {e}"))
    }

    /// Returns true if `data` looks like our encrypted format (magic + version).
    pub fn is_encrypted(data: &[u8]) -> bool {
        data.len() >= HEADER_SIZE && &data[..MAGIC.len()] == MAGIC && data[MAGIC.len()] == VERSION
    }

    /// Try decrypt; if it's not our encrypted format, return the original
    /// bytes as Ok — used for photo fallback where plaintext + ciphertext
    /// coexist.
    #[allow(dead_code)]
    pub fn decrypt_or_plaintext(&self, data: &[u8]) -> Vec<u8> {
        if Self::is_encrypted(data) {
            match self.decrypt(data) {
                Ok(plain) => plain,
                Err(_) => data.to_vec(),
            }
        } else {
            data.to_vec()
        }
    }
}

/// Resolve the `.crypto-key` directory adjacent to `app_data_dir` (i.e. at
/// project root: `app_data_dir/../.crypto-key`). Single source of truth for
/// key location — generation and loading must always agree.
pub fn crypto_key_dir() -> Result<PathBuf, String> {
    let app_data_dir = crate::db::get_app_data_dir();
    let dir = app_data_dir.join("..").join(KEY_DIR_NAME);
    fs::create_dir_all(&dir).map_err(|e| format!("create .crypto-key dir: {e}"))?;
    Ok(dir)
}

/// Resolve the key file path adjacent to `app_data_dir`.
pub fn crypto_key_path() -> Result<PathBuf, String> {
    Ok(crypto_key_dir()?.join(KEY_FILE_NAME))
}

/// Check whether `.crypto-key/key.bin` exists adjacent to `app_data_dir`.
pub fn key_exists() -> bool {
    crypto_key_path().map(|p| p.exists()).unwrap_or(false)
}

/// Generate a fresh random 256-bit key and write it adjacent to `app_data_dir`.
/// Returns an error if a key already exists.
pub fn generate_key() -> Result<Vec<u8>, String> {
    let path = crypto_key_path()?;
    generate_key_to_path(&path)
}

fn generate_key_to_path(path: &Path) -> Result<Vec<u8>, String> {
    if path.exists() {
        return Err("encryption key already exists".to_string());
    }
    let mut key_bytes = [0u8; KEY_SIZE];
    OsRng.fill_bytes(&mut key_bytes);
    fs::write(path, &key_bytes).map_err(|e| format!("write key file: {e}"))?;
    Ok(key_bytes.to_vec())
}

/// Load the key adjacent to `app_data_dir`.
/// Returns an error if missing — callers must not silently regenerate.
pub fn load_key() -> Result<CryptoKey, String> {
    let path = crypto_key_path()?;
    load_key_from_path(&path)
}

/// Load the key from a specific path.
pub fn load_key_from_path(path: &Path) -> Result<CryptoKey, String> {
    if !path.exists() {
        return Err(format!(
            "encryption key not found at {}. Generate it first with \
             encryption_generate_key, and back up .crypto-key/ — without it \
             your encrypted data is unrecoverable.",
            path.display()
        ));
    }
    let key_bytes = fs::read(path).map_err(|e| format!("read key file: {e}"))?;
    if key_bytes.len() != KEY_SIZE {
        return Err(format!(
            "invalid key size: {} bytes, expected {}",
            key_bytes.len(),
            KEY_SIZE
        ));
    }
    let mut arr = [0u8; KEY_SIZE];
    arr.copy_from_slice(&key_bytes[..KEY_SIZE]);
    Ok(CryptoKey(Zeroizing::new(arr)))
}

/// Convenience: check whether key exists at the given directory
/// (used by `encryption_status`).
/// Reserved for future encryption-status UI.
#[allow(dead_code)]
pub fn key_exists_with_dir(dir: &Path) -> bool {
    dir.join(KEY_FILE_NAME).exists()
}

/// Encrypt a file in place using tmp-then-rename (crash-safe).
/// Skips if already encrypted.
/// Reserved for encrypted-backup path (pre-encryption plaintext backups gap).
#[allow(dead_code)]
pub fn encrypt_file_in_place(path: &Path) -> Result<(), String> {
    let crypto = load_key()?;
    let plaintext = fs::read(path).map_err(|e| format!("read {path:?}: {e}"))?;
    if CryptoKey::is_encrypted(&plaintext) {
        return Ok(()); // already encrypted
    }
    let ciphertext = crypto.encrypt(&plaintext);
    let tmp = path.with_extension(
        path.extension()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
            + ".tmp",
    );
    fs::write(&tmp, &ciphertext).map_err(|e| format!("write tmp {tmp:?}: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename {path:?}: {e}"))?;
    Ok(())
}

/// Decrypt a file in place using tmp-then-rename (crash-safe).
pub fn decrypt_file_in_place(path: &Path) -> Result<(), String> {
    let crypto = load_key()?;
    let ciphertext = fs::read(path).map_err(|e| format!("read {path:?}: {e}"))?;
    let plaintext = crypto.decrypt(&ciphertext)?;
    let tmp = path.with_extension(
        path.extension()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
            + ".tmp",
    );
    fs::write(&tmp, &plaintext).map_err(|e| format!("write tmp {tmp:?}: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename {path:?}: {e}"))?;
    Ok(())
}

/// Encrypt `src` (plaintext) → `dst` (ciphertext tmp) without touching `src`.
/// Used by the DB encrypt command so every failure point leaves either intact
/// plaintext or complete ciphertext — never a mislabelled half-state.
pub fn encrypt_file_to_tmp(src: &Path, dst: &Path) -> Result<(), String> {
    let crypto = load_key()?;
    let plaintext = fs::read(src).map_err(|e| format!("read {src:?}: {e}"))?;
    if CryptoKey::is_encrypted(&plaintext) {
        return Err("source already encrypted".to_string());
    }
    let ciphertext = crypto.encrypt(&plaintext);
    fs::write(dst, &ciphertext).map_err(|e| format!("write tmp {dst:?}: {e}"))?;
    Ok(())
}

/// Decrypt `data/residents.db.enc` back to `data/residents.db`.
/// Removes plaintext sidecars so init_db starts clean — no plaintext WAL
/// pages replay over decrypted DB.
pub fn decrypt_database_files(db_path: &Path) -> Result<(), String> {
    let enc_path = db_path.with_extension("db.enc");
    if !enc_path.is_file() {
        return Err("encrypted database not found".to_string());
    }
    decrypt_file_in_place(&enc_path)?;
    // Rename decrypted file from `residents.db.enc` → `residents.db`
    fs::rename(&enc_path, db_path).map_err(|e| format!("rename decrypted DB: {e}"))?;
    // Remove any leftover plaintext sidecars (crash residue) so init_db
    // starts clean — no plaintext WAL pages replay over decrypted DB.
    for side in [db::wal_path_for(db_path), db::shm_path_for(db_path)] {
        if side.is_file() {
            let _ = fs::remove_file(&side);
        }
    }
    Ok(())
}

/// Encrypt every file in a directory in place (skips already-encrypted).
/// Crashes safe via per-file tmp+rename.
pub fn encrypt_dir_in_place(dir: &Path) -> Result<usize, String> {
    let crypto = load_key()?;
    let mut count = 0usize;
    for entry in fs::read_dir(dir).map_err(|e| format!("read dir {dir:?}: {e}"))? {
        let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            count += encrypt_dir_in_place(&path)?;
        } else if path.is_file() {
            let data = fs::read(&path).map_err(|e| format!("read {path:?}: {e}"))?;
            if CryptoKey::is_encrypted(&data) {
                continue; // already encrypted
            }
            let ciphertext = crypto.encrypt(&data);
            let tmp = path.with_extension(
                path.extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
                    + ".tmp",
            );
            fs::write(&tmp, &ciphertext).map_err(|e| format!("write tmp {tmp:?}: {e}"))?;
            fs::rename(&tmp, &path).map_err(|e| format!("rename {path:?}: {e}"))?;
            count += 1;
        }
    }
    Ok(count)
}

/// Decrypt every file in a directory in place (skips non-encrypted).
pub fn decrypt_dir_in_place(dir: &Path) -> Result<usize, String> {
    let crypto = load_key()?;
    let mut count = 0usize;
    for entry in fs::read_dir(dir).map_err(|e| format!("read dir {dir:?}: {e}"))? {
        let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            count += decrypt_dir_in_place(&path)?;
        } else if path.is_file() {
            let data = fs::read(&path).map_err(|e| format!("read {path:?}: {e}"))?;
            if !CryptoKey::is_encrypted(&data) {
                continue; // not encrypted, skip
            }
            let plaintext = crypto.decrypt(&data)?;
            let tmp = path.with_extension(
                path.extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
                    + ".tmp",
            );
            fs::write(&tmp, &plaintext).map_err(|e| format!("write tmp {tmp:?}: {e}"))?;
            fs::rename(&tmp, &path).map_err(|e| format!("rename {path:?}: {e}"))?;
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = CryptoKey(Zeroizing::new([42u8; 32]));
        let plaintext = b"hello world";
        let ciphertext = key.encrypt(plaintext);
        assert_ne!(ciphertext, plaintext);
        assert!(CryptoKey::is_encrypted(&ciphertext));
        let decrypted = key.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = CryptoKey(Zeroizing::new([42u8; 32]));
        let mut ciphertext = key.encrypt(b"hello world");
        ciphertext[ciphertext.len() - 1] ^= 0xff; // flip last byte (tag)
        assert!(key.decrypt(&ciphertext).is_err());
    }

    #[test]
    fn plaintext_fails_decrypt() {
        let key = CryptoKey(Zeroizing::new([42u8; 32]));
        assert!(!CryptoKey::is_encrypted(b"just plaintext bytes here"));
        assert!(key.decrypt(b"just plaintext bytes here").is_err());
    }

    #[test]
    fn decrypt_or_plaintext_fallback() {
        let key = CryptoKey(Zeroizing::new([42u8; 32]));
        let ct = key.encrypt(b"secret");
        assert_eq!(key.decrypt_or_plaintext(&ct), b"secret");
        let pt = b"plaintext";
        assert_eq!(key.decrypt_or_plaintext(pt), pt);
    }

    #[test]
    fn bad_magic_is_not_encrypted() {
        let key = CryptoKey(Zeroizing::new([42u8; 32]));
        let mut ct = key.encrypt(b"hello");
        ct[0] ^= 0xff; // corrupt magic
        assert!(!CryptoKey::is_encrypted(&ct));
        assert!(key.decrypt(&ct).is_err());
    }

    #[test]
    fn wrong_version_is_not_encrypted() {
        let key = CryptoKey(Zeroizing::new([42u8; 32]));
        let mut ct = key.encrypt(b"hello");
        ct[8] = 99; // corrupt version byte
        assert!(!CryptoKey::is_encrypted(&ct));
        assert!(key.decrypt(&ct).is_err());
    }
}
