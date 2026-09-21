//! Joplin end-to-end encryption, compatible with `EncryptionService` in
//! `@joplin/lib/services/e2ee`.
//!
//! Master keys (stored in `info.json`) are encrypted with the user's password.
//! Once decrypted, a master key is a 512-char hex string that is itself used as
//! the *password* (UTF-8 bytes) for per-chunk key derivation of items.
//!
//! Supported methods:
//! * `KeyV1` (8), `FileV1` (9), `StringV1` (10): PBKDF2-HMAC-SHA512 + AES-256-GCM.
//! * `SJCL3` (3), `SJCL4` (4), `SJCL1a` (5), `SJCL1b` (7): SJCL JSON, PBKDF2-HMAC-SHA256 + AES-CCM.
//! * `SJCL` (1) / `SJCL2` (2) use OCB2, deprecated since 2020 and not supported.

use std::collections::HashMap;

use aes::{Aes128, Aes256};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::Aes256Gcm;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ccm::consts::{U12, U13, U8};
use ccm::Ccm;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};

use crate::error::{Error, Result};

pub const METHOD_SJCL: u32 = 1;
pub const METHOD_SJCL2: u32 = 2;
pub const METHOD_SJCL3: u32 = 3;
pub const METHOD_SJCL4: u32 = 4;
pub const METHOD_SJCL1A: u32 = 5;
pub const METHOD_CUSTOM: u32 = 6;
pub const METHOD_SJCL1B: u32 = 7;
pub const METHOD_KEY_V1: u32 = 8;
pub const METHOD_FILE_V1: u32 = 9;
pub const METHOD_STRING_V1: u32 = 10;

const STRING_V1_CHUNK: usize = 65536;
const FILE_V1_CHUNK: usize = 131072;

/// A master key entry as stored in `info.json` → `masterKeys`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterKey {
    pub id: String,
    #[serde(default)]
    pub created_time: i64,
    #[serde(default)]
    pub updated_time: i64,
    #[serde(default)]
    pub source_application: String,
    pub encryption_method: u32,
    #[serde(default)]
    pub checksum: String,
    pub content: String,
    #[serde(default)]
    pub enabled: Option<i64>,
    #[serde(rename = "hasBeenUsed", default)]
    pub has_been_used: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct GcmPayload {
    salt: String,
    iv: String,
    ct: String,
}

#[derive(Deserialize)]
struct SjclPayload {
    iv: String,
    #[serde(default = "default_iter")]
    iter: u32,
    #[serde(default = "default_ks")]
    ks: u32,
    #[serde(default = "default_ts")]
    ts: u32,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    adata: String,
    salt: String,
    ct: String,
}

fn default_iter() -> u32 {
    10000
}
fn default_ks() -> u32 {
    128
}
fn default_ts() -> u32 {
    64
}

fn b64(s: &str) -> Result<Vec<u8>> {
    B64.decode(s).map_err(|e| Error::Crypto(format!("base64: {e}")))
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    rand::rng().fill_bytes(&mut b);
    b
}

// ---------------------------------------------------------------------------
// Primitive block ciphers
// ---------------------------------------------------------------------------

fn gcm_decrypt(password: &str, cipher_json: &str, iterations: u32) -> Result<Vec<u8>> {
    let p: GcmPayload = serde_json::from_str(cipher_json)?;
    let salt = b64(&p.salt)?;
    let iv = b64(&p.iv)?;
    let ct = b64(&p.ct)?;
    let key = pbkdf2::pbkdf2_hmac_array::<Sha512, 32>(password.as_bytes(), &salt, iterations);
    let cipher = Aes256Gcm::new(&key.into());
    let nonce = iv
        .as_slice()
        .try_into()
        .map_err(|_| Error::Crypto(format!("invalid GCM IV length {}", iv.len())))?;
    cipher
        .decrypt(nonce, ct.as_slice())
        .map_err(|_| Error::Crypto("GCM authentication failed (wrong key or password?)".into()))
}

fn gcm_encrypt(password: &str, plain: &[u8], iterations: u32) -> Result<String> {
    // Joplin derives the salt as sha256(nonce); any unique 32-byte value works.
    let salt: [u8; 32] = Sha256::digest(random_bytes::<36>()).into();
    let iv = random_bytes::<12>();
    let key = pbkdf2::pbkdf2_hmac_array::<Sha512, 32>(password.as_bytes(), &salt, iterations);
    let cipher = Aes256Gcm::new(&key.into());
    let ct = cipher
        .encrypt(&iv.into(), plain)
        .map_err(|_| Error::Crypto("GCM encryption failed".into()))?;
    Ok(serde_json::to_string(&GcmPayload {
        salt: B64.encode(salt),
        iv: B64.encode(iv),
        ct: B64.encode(ct),
    })?)
}

/// Decrypts an `sjcl.json.encrypt` payload (CCM mode only).
fn sjcl_decrypt(password: &str, cipher_json: &str) -> Result<Vec<u8>> {
    let p: SjclPayload = serde_json::from_str(cipher_json)?;
    if p.mode != "ccm" {
        return Err(Error::Crypto(format!(
            "SJCL mode '{}' not supported (legacy OCB2 data, re-encrypt it from Joplin)",
            p.mode
        )));
    }
    if !p.adata.is_empty() {
        return Err(Error::Crypto("SJCL adata not supported".into()));
    }
    if p.ts != 64 {
        return Err(Error::Crypto(format!("unsupported SJCL tag size {}", p.ts)));
    }
    let salt = b64(&p.salt)?;
    let iv = b64(&p.iv)?;
    let ct = b64(&p.ct)?;
    let tag_len = (p.ts / 8) as usize;
    if ct.len() < tag_len || iv.len() < 7 {
        return Err(Error::Crypto("SJCL payload too short".into()));
    }
    let plain_len = ct.len() - tag_len;

    // sjcl.mode.ccm: L = size of the length field, nonce = iv truncated to 15 - L bytes.
    let mut l = 2usize;
    while l < 4 && (plain_len >> (8 * l)) != 0 {
        l += 1;
    }
    if l < 15usize.saturating_sub(iv.len()) {
        l = 15 - iv.len();
    }
    let nonce = &iv[..15 - l];

    let mut key = vec![0u8; (p.ks / 8) as usize];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, p.iter, &mut key);

    let fail = || Error::Crypto("CCM authentication failed (wrong key or password?)".into());
    macro_rules! ccm {
        ($aes:ty, $n:ty) => {{
            let c = Ccm::<$aes, U8, $n>::new_from_slice(&key).map_err(|_| fail())?;
            c.decrypt(nonce.try_into().map_err(|_| fail())?, ct.as_slice())
                .map_err(|_| fail())
        }};
    }
    match (p.ks, nonce.len()) {
        (128, 13) => ccm!(Aes128, U13),
        (128, 12) => ccm!(Aes128, U12),
        (256, 13) => ccm!(Aes256, U13),
        (256, 12) => ccm!(Aes256, U12),
        (ks, n) => Err(Error::Crypto(format!("unsupported SJCL key/nonce size {ks}/{n}"))),
    }
}

/// JavaScript's global `unescape()` (used by SJCL1a/SJCL1b).
fn js_unescape(s: &str) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    let hex = |u: &[u16]| -> Option<u16> {
        let s = String::from_utf16(u).ok()?;
        u16::from_str_radix(&s, 16).ok()
    };
    let mut i = 0;
    while i < units.len() {
        let c = units[i];
        if c == b'%' as u16 {
            if i + 6 <= units.len() && units[i + 1] == b'u' as u16 {
                if let Some(v) = hex(&units[i + 2..i + 6]) {
                    out.push(v);
                    i += 6;
                    continue;
                }
            }
            if i + 3 <= units.len() {
                if let Some(v) = hex(&units[i + 1..i + 3]) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    String::from_utf16_lossy(&out)
}

fn utf16le_decode_units(bytes: &[u8]) -> Vec<u16> {
    bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}

fn utf16le_encode(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|u| u.to_le_bytes()).collect()
}

// ---------------------------------------------------------------------------
// Master keys
// ---------------------------------------------------------------------------

/// Decrypts a master key with the user's master password; returns the hex
/// plaintext that is used as the password for item encryption.
pub fn decrypt_master_key(mk: &MasterKey, password: &str) -> Result<String> {
    let plain = match mk.encryption_method {
        METHOD_KEY_V1 => hex::encode(gcm_decrypt(password, &mk.content, 220_000)?),
        METHOD_SJCL3 | METHOD_SJCL4 => {
            String::from_utf8(sjcl_decrypt(password, &mk.content)?)
                .map_err(|_| Error::Crypto("invalid UTF-8 in master key".into()))?
        }
        METHOD_SJCL2 => {
            return Err(Error::Crypto(
                "master key uses legacy OCB2 encryption: open Joplin desktop, it will offer to upgrade it"
                    .into(),
            ))
        }
        m => return Err(Error::UnsupportedMethod(m)),
    };
    if !mk.checksum.is_empty() && mk.encryption_method == METHOD_SJCL2 {
        let sum = hex::encode(Sha256::digest(plain.as_bytes()));
        if sum != mk.checksum {
            return Err(Error::Crypto("master key checksum mismatch".into()));
        }
    }
    Ok(plain)
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub method: u32,
    pub master_key_id: String,
}

fn take<'a>(s: &'a str, pos: &mut usize, n: usize) -> Result<&'a str> {
    let end = pos
        .checked_add(n)
        .filter(|e| *e <= s.len())
        .ok_or_else(|| Error::Crypto("truncated cipher text".into()))?;
    let out = s
        .get(*pos..end)
        .ok_or_else(|| Error::Crypto("invalid cipher text".into()))?;
    *pos = end;
    Ok(out)
}

fn hex_num(s: &str) -> Result<usize> {
    usize::from_str_radix(s, 16).map_err(|_| Error::Crypto(format!("invalid hex number {s:?}")))
}

pub fn decode_header(cipher_text: &str) -> Result<(Header, usize)> {
    let mut pos = 0;
    let ident = take(cipher_text, &mut pos, 5)?;
    if !ident.starts_with("JED") || ident[3..].parse::<u32>().is_err() {
        return Err(Error::Crypto(format!("invalid encryption header {ident:?}")));
    }
    let md_size = hex_num(take(cipher_text, &mut pos, 6)?)?;
    if md_size < 34 {
        return Err(Error::Crypto("invalid header metadata size".into()));
    }
    let md = take(cipher_text, &mut pos, md_size)?;
    let method = hex_num(&md[..2])? as u32;
    let master_key_id = md[2..34].to_string();
    Ok((Header { method, master_key_id }, pos))
}

fn encode_header(method: u32, master_key_id: &str) -> String {
    let md = format!("{method:02x}{master_key_id}");
    format!("JED01{:06x}{md}", md.len())
}

/// Holds decrypted master keys (id → hex plaintext) and the active key id.
#[derive(Default, Clone)]
pub struct KeyRing {
    keys: HashMap<String, String>,
    pub active_id: Option<String>,
}

impl KeyRing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: &str, plain_hex: String) {
        self.keys.insert(id.to_string(), plain_hex);
    }

    pub fn has(&self, id: &str) -> bool {
        self.keys.contains_key(id)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    fn key(&self, id: &str) -> Result<&str> {
        self.keys
            .get(id)
            .map(String::as_str)
            .ok_or_else(|| Error::MasterKeyNotLoaded(id.to_string()))
    }

    /// `EncryptionService.decryptString`.
    pub fn decrypt_string(&self, cipher_text: &str) -> Result<String> {
        let (header, mut pos) = decode_header(cipher_text)?;
        let key = self.key(&header.master_key_id)?;
        let mut units: Vec<u16> = Vec::new();
        let mut text = String::new();

        while pos < cipher_text.len() {
            let len_hex = take(cipher_text, &mut pos, 6)?;
            let len = hex_num(len_hex)?;
            if len == 0 {
                continue;
            }
            let block = take(cipher_text, &mut pos, len)?;
            match header.method {
                METHOD_STRING_V1 => units.extend(utf16le_decode_units(&gcm_decrypt(key, block, 3)?)),
                METHOD_SJCL3 | METHOD_SJCL4 => text.push_str(&sjcl_utf8(key, block)?),
                METHOD_SJCL1A | METHOD_SJCL1B => text.push_str(&js_unescape(&sjcl_utf8(key, block)?)),
                m => return Err(Error::UnsupportedMethod(m)),
            }
        }
        if header.method == METHOD_STRING_V1 {
            // Chunks are split on UTF-16 code units, so surrogate pairs may span two chunks.
            text = String::from_utf16(&units).map_err(|_| Error::Crypto("invalid UTF-16".into()))?;
        }
        Ok(text)
    }

    /// `EncryptionService.encryptString` with the default method (StringV1).
    pub fn encrypt_string(&self, plain: &str) -> Result<String> {
        let id = self
            .active_id
            .clone()
            .ok_or_else(|| Error::MasterKeyNotLoaded("no active master key".into()))?;
        let key = self.key(&id)?;
        let units: Vec<u16> = plain.encode_utf16().collect();
        let mut out = encode_header(METHOD_STRING_V1, &id);
        for chunk in units.chunks(STRING_V1_CHUNK) {
            let block = gcm_encrypt(key, &utf16le_encode(chunk), 3)?;
            out.push_str(&format!("{:06x}", block.len()));
            out.push_str(&block);
        }
        Ok(out)
    }

    /// Decrypts an encrypted resource blob (`decryptFile`), returning raw bytes.
    pub fn decrypt_file(&self, cipher_text: &str) -> Result<Vec<u8>> {
        let (header, mut pos) = decode_header(cipher_text)?;
        let key = self.key(&header.master_key_id)?;
        let mut out = Vec::new();
        let mut legacy_b64 = String::new();
        while pos < cipher_text.len() {
            let len = hex_num(take(cipher_text, &mut pos, 6)?)?;
            if len == 0 {
                continue;
            }
            let block = take(cipher_text, &mut pos, len)?;
            match header.method {
                METHOD_FILE_V1 => out.extend(gcm_decrypt(key, block, 3)?),
                // Legacy methods encrypted the base64 text of the file.
                METHOD_SJCL3 | METHOD_SJCL4 => legacy_b64.push_str(&sjcl_utf8(key, block)?),
                METHOD_SJCL1A | METHOD_SJCL1B => legacy_b64.push_str(&js_unescape(&sjcl_utf8(key, block)?)),
                m => return Err(Error::UnsupportedMethod(m)),
            }
        }
        if !legacy_b64.is_empty() {
            out = b64(&legacy_b64)?;
        }
        Ok(out)
    }

    /// Encrypts a resource blob with FileV1.
    pub fn encrypt_file(&self, data: &[u8]) -> Result<String> {
        let id = self
            .active_id
            .clone()
            .ok_or_else(|| Error::MasterKeyNotLoaded("no active master key".into()))?;
        let key = self.key(&id)?;
        let mut out = encode_header(METHOD_FILE_V1, &id);
        // Joplin reads FILE_V1_CHUNK base64 chars per block, i.e. 3/4 of that in bytes.
        for chunk in data.chunks(FILE_V1_CHUNK / 4 * 3) {
            let block = gcm_encrypt(key, chunk, 3)?;
            out.push_str(&format!("{:06x}", block.len()));
            out.push_str(&block);
        }
        Ok(out)
    }
}

fn sjcl_utf8(key: &str, block: &str) -> Result<String> {
    String::from_utf8(sjcl_decrypt(key, block)?).map_err(|_| Error::Crypto("invalid UTF-8".into()))
}

pub fn is_encrypted_text(s: &str) -> bool {
    s.len() >= 5 && s.starts_with("JED") && s[3..5].bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MK_ID: &str = "0123456789abcdef0123456789abcdef";

    fn ring() -> KeyRing {
        let mut r = KeyRing::new();
        r.insert(MK_ID, "ab".repeat(256));
        r.active_id = Some(MK_ID.into());
        r
    }

    #[test]
    fn string_v1_round_trip_with_surrogates_across_chunks() {
        let r = ring();
        // Put an emoji (surrogate pair) exactly across the chunk boundary.
        let mut s = "a".repeat(STRING_V1_CHUNK - 1);
        s.push_str("😀 ciao àèì");
        let enc = r.encrypt_string(&s).unwrap();
        assert!(enc.starts_with("JED01000022"));
        assert_eq!(r.decrypt_string(&enc).unwrap(), s);
    }

    #[test]
    fn file_v1_round_trip() {
        let r = ring();
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let enc = r.encrypt_file(&data).unwrap();
        assert_eq!(r.decrypt_file(&enc).unwrap(), data);
    }

    #[test]
    fn header() {
        let h = encode_header(10, MK_ID);
        let (d, pos) = decode_header(&h).unwrap();
        assert_eq!(d, Header { method: 10, master_key_id: MK_ID.into() });
        assert_eq!(pos, h.len());
    }

    #[test]
    fn unescape_js() {
        assert_eq!(js_unescape("%E0%u00e8%20x%ZZ"), "à\u{e8} x%ZZ");
    }
}
