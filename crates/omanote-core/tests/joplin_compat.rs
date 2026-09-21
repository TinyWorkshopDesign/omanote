//! Cross-checks against vectors produced by Joplin's own JS crypto (see scratch gen.mjs):
//! SJCL (CCM) and WebCrypto (PBKDF2 + AES-GCM) exactly as `EncryptionService` uses them.

use omanote_core::e2ee::{decrypt_master_key, KeyRing, MasterKey};
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("joplin_vectors.json")).unwrap()
}

fn mk(v: &Value, key: &str) -> MasterKey {
    serde_json::from_value(v[key].clone()).unwrap()
}

#[test]
fn decrypts_joplin_master_keys() {
    let v = vectors();
    let password = v["password"].as_str().unwrap();
    let expected = v["mkPlain"].as_str().unwrap();
    assert_eq!(decrypt_master_key(&mk(&v, "mkKeyV1"), password).unwrap(), expected);
    assert_eq!(decrypt_master_key(&mk(&v, "mkSjcl4"), password).unwrap(), expected);
    assert!(decrypt_master_key(&mk(&v, "mkKeyV1"), "wrong").is_err());
}

#[test]
fn decrypts_joplin_items() {
    let v = vectors();
    let mut ring = KeyRing::new();
    let m = mk(&v, "mkKeyV1");
    ring.insert(&m.id, v["mkPlain"].as_str().unwrap().to_string());
    let plain = v["plain"].as_str().unwrap();
    assert_eq!(ring.decrypt_string(v["stringV1"].as_str().unwrap()).unwrap(), plain);
    assert_eq!(ring.decrypt_string(v["sjcl1a"].as_str().unwrap()).unwrap(), plain);
    let short: String = plain.chars().take(20000).collect();
    // JS slice() counts UTF-16 units; the emoji sits well inside the first 20000 chars.
    let short_js: String = String::from_utf16(&plain.encode_utf16().take(20000).collect::<Vec<_>>()).unwrap();
    assert_eq!(ring.decrypt_string(v["sjcl1b"].as_str().unwrap()).unwrap(), short_js);
    assert_ne!(short.len(), 0);
}
