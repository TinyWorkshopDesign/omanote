//! Joplin item (de)serialisation, byte-compatible with `BaseItem.serialize` /
//! `BaseItem.unserialize` in `@joplin/lib`.
//!
//! On-the-wire format (one `<id>.md` file per item on the sync target):
//!
//! ```text
//! Title
//!
//! Body (notes only, may span many lines)
//!
//! id: 0123456789abcdef0123456789abcdef
//! parent_id: ...
//! type_: 1
//! ```
//!
//! Unknown properties are preserved verbatim so that items written by newer
//! Joplin clients survive a round-trip through Omanote.

use chrono::{DateTime, TimeZone, Utc};
use indexmap::IndexMap;

use crate::error::{Error, Result};

pub const TYPE_NOTE: i64 = 1;
pub const TYPE_FOLDER: i64 = 2;
pub const TYPE_RESOURCE: i64 = 4;
pub const TYPE_TAG: i64 = 5;
pub const TYPE_NOTE_TAG: i64 = 6;
pub const TYPE_MASTER_KEY: i64 = 9;
pub const TYPE_REVISION: i64 = 13;


/// Field order used by Joplin (SQLite column order) for newly created notes.
pub const NOTE_FIELDS: &[&str] = &[
    "id",
    "parent_id",
    "created_time",
    "updated_time",
    "is_conflict",
    "latitude",
    "longitude",
    "altitude",
    "author",
    "source_url",
    "is_todo",
    "todo_due",
    "todo_completed",
    "source",
    "source_application",
    "application_data",
    "order",
    "user_created_time",
    "user_updated_time",
    "encryption_cipher_text",
    "encryption_applied",
    "markup_language",
    "is_shared",
    "share_id",
    "conflict_original_id",
    "master_key_id",
    "user_data",
    "deleted_time",
];

pub const FOLDER_FIELDS: &[&str] = &[
    "id",
    "created_time",
    "updated_time",
    "user_created_time",
    "user_updated_time",
    "encryption_cipher_text",
    "encryption_applied",
    "parent_id",
    "is_shared",
    "share_id",
    "master_key_id",
    "icon",
    "user_data",
    "deleted_time",
];

/// Properties kept in clear text when an item is encrypted
/// (`BaseItem.serializeForSync` → `keepKeys`).
pub const ENCRYPTION_KEEP_KEYS: &[&str] = &[
    "id",
    "note_id",
    "tag_id",
    "parent_id",
    "share_id",
    "updated_time",
    "deleted_time",
    "type_",
    "is_locked",
    "extracted_resource_ids",
];

/// A Joplin item in its generic form. `props` holds every `key: value` line
/// (already unescaped), in the original order, including `type_`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawItem {
    pub title: Option<String>,
    pub body: Option<String>,
    pub props: IndexMap<String, String>,
}

impl RawItem {
    pub fn get(&self, key: &str) -> &str {
        self.props.get(key).map(String::as_str).unwrap_or("")
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.props.insert(key.to_string(), value.into());
    }

    pub fn id(&self) -> &str {
        self.get("id")
    }

    pub fn item_type(&self) -> i64 {
        self.get("type_").parse().unwrap_or(0)
    }

    pub fn int(&self, key: &str) -> i64 {
        self.get(key).parse().unwrap_or(0)
    }

    /// Timestamp property as milliseconds since the Unix epoch.
    pub fn time(&self, key: &str) -> i64 {
        parse_time(self.get(key))
    }

    pub fn set_time(&mut self, key: &str, ms: i64) {
        self.set(key, format_time(ms));
    }

    pub fn is_encrypted(&self) -> bool {
        self.get("encryption_applied") == "1" && self.get("encryption_cipher_text").starts_with("JED")
    }

    /// Parses the content of a `<id>.md` sync file.
    pub fn parse(content: &str) -> Result<Self> {
        let lines: Vec<&str> = content.split('\n').collect();
        let mut props_rev: Vec<(String, String)> = Vec::new();
        let mut body_lines: Vec<&str> = Vec::new();

        for i in (0..lines.len()).rev() {
            let line = lines[i].trim();
            if line.is_empty() {
                body_lines = lines[..i].to_vec();
                break;
            }
            let p = line
                .find(':')
                .ok_or_else(|| Error::Format(format!("invalid property line: {line}")))?;
            props_rev.push((line[..p].trim().to_string(), line[p + 1..].trim().to_string()));
        }

        let mut props = IndexMap::new();
        for (k, v) in props_rev.into_iter().rev() {
            let v = if k == "body" || k.ends_with('_') { v } else { unescape_prop(&v) };
            props.insert(k, v);
        }

        let item_type: i64 = props
            .get("type_")
            .and_then(|t| t.parse().ok())
            .ok_or_else(|| Error::Format("missing type_".into()))?;

        if let Some(id) = props.get("id") {
            if !id.is_empty() && !(id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())) {
                return Err(Error::Format(format!("invalid item id: {id:?}")));
            }
        }

        let mut title = None;
        let mut body = None;
        if !body_lines.is_empty() {
            title = Some(body_lines[0].to_string());
            let rest = if body_lines.len() > 2 { &body_lines[2..] } else { &[][..] };
            if item_type == TYPE_NOTE {
                body = Some(rest.join("\n"));
            }
        } else if item_type == TYPE_NOTE && !props.contains_key("encryption_cipher_text") {
            body = Some(String::new());
        }

        Ok(RawItem { title, body, props })
    }

    /// Serialises the item exactly like `BaseItem.serialize`.
    pub fn serialize(&self) -> String {
        let mut parts: Vec<String> = Vec::with_capacity(3);
        if let Some(t) = &self.title {
            parts.push(t.clone());
        }
        if let Some(b) = &self.body {
            if !b.is_empty() {
                parts.push(b.clone());
            }
        }
        let props: Vec<String> = self
            .props
            .iter()
            .map(|(k, v)| format!("{k}: {}", escape_prop(v)))
            .collect();
        if !props.is_empty() {
            parts.push(props.join("\n"));
        }
        parts.join("\n\n")
    }

    /// Builds the reduced, encrypted envelope for an item (`serializeForSync`).
    pub fn encrypted_envelope(&self, cipher_text: String) -> RawItem {
        let mut props = IndexMap::new();
        for (k, v) in &self.props {
            if ENCRYPTION_KEEP_KEYS.contains(&k.as_str()) && k != "type_" {
                props.insert(k.clone(), v.clone());
            }
        }
        props.insert("encryption_cipher_text".into(), cipher_text);
        props.insert("encryption_applied".into(), "1".into());
        props.insert("type_".into(), self.get("type_").to_string());
        RawItem { title: None, body: None, props }
    }

    /// Creates an empty item with Joplin's default field set.
    pub fn new_with_fields(item_type: i64, fields: &[&str]) -> RawItem {
        let mut props = IndexMap::new();
        for f in fields {
            let default = match *f {
                "latitude" | "longitude" => "0.00000000",
                "altitude" => "0.0000",
                "is_conflict" | "is_todo" | "todo_due" | "todo_completed" | "order"
                | "encryption_applied" | "is_shared" | "deleted_time" => "0",
                "markup_language" => "1",
                _ => "",
            };
            props.insert(f.to_string(), default.to_string());
        }
        props.insert("type_".into(), item_type.to_string());
        RawItem { title: Some(String::new()), body: None, props }
    }
}

/// Mirrors `serialize_format`'s escaping of non-body props.
fn escape_prop(v: &str) -> String {
    v.replace("\\n", "\\\\n")
        .replace("\\r", "\\\\r")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Mirrors `unserialize_format`'s unescaping, including its replacement order.
fn unescape_prop(v: &str) -> String {
    v.replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\\n", "\\n")
        .replace("\\\r", "\\r")
}

pub fn format_time(ms: i64) -> String {
    if ms == 0 {
        return String::new();
    }
    let dt: DateTime<Utc> = Utc.timestamp_millis_opt(ms).single().unwrap_or_default();
    dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn parse_time(s: &str) -> i64 {
    if s.is_empty() {
        return 0;
    }
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "My title\n\nLine one\n\nLine three\n\nid: 0123456789abcdef0123456789abcdef\nparent_id: fedcba9876543210fedcba9876543210\ncreated_time: 2024-03-01T10:20:30.123Z\nupdated_time: 2024-03-02T10:20:30.456Z\nauthor: a\\nb\nfuture_field: keep me\ntype_: 1";

    #[test]
    fn round_trip_note() {
        let item = RawItem::parse(NOTE).unwrap();
        assert_eq!(item.title.as_deref(), Some("My title"));
        assert_eq!(item.body.as_deref(), Some("Line one\n\nLine three"));
        assert_eq!(item.get("author"), "a\nb");
        assert_eq!(item.get("future_field"), "keep me");
        assert_eq!(item.time("updated_time"), 1709374830456);
        assert_eq!(item.serialize(), NOTE);
    }

    #[test]
    fn empty_body_note() {
        let s = "Title\n\nid: 0123456789abcdef0123456789abcdef\ntype_: 1";
        let item = RawItem::parse(s).unwrap();
        assert_eq!(item.body.as_deref(), Some(""));
        assert_eq!(item.serialize(), s);
    }

    #[test]
    fn folder_has_no_body() {
        let s = "Work\n\nid: 0123456789abcdef0123456789abcdef\ntype_: 2";
        let item = RawItem::parse(s).unwrap();
        assert_eq!(item.title.as_deref(), Some("Work"));
        assert_eq!(item.body, None);
        assert_eq!(item.serialize(), s);
    }

    #[test]
    fn encrypted_item_has_only_props() {
        let s = "id: 0123456789abcdef0123456789abcdef\nparent_id: \nupdated_time: 2024-03-02T10:20:30.456Z\nencryption_cipher_text: JED01000022...\nencryption_applied: 1\ntype_: 1";
        let item = RawItem::parse(s).unwrap();
        assert_eq!(item.title, None);
        assert!(item.is_encrypted());
        assert_eq!(item.serialize(), s);
    }

    #[test]
    fn literal_backslash_n_round_trips() {
        let mut item = RawItem::new_with_fields(TYPE_FOLDER, FOLDER_FIELDS);
        item.set("user_data", "C:\\new\nline");
        let parsed = RawItem::parse(&item.serialize()).unwrap();
        assert_eq!(parsed.get("user_data"), "C:\\new\nline");
    }

    #[test]
    fn rejects_bad_id() {
        assert!(RawItem::parse("x\n\nid: ../../etc\ntype_: 1").is_err());
    }
}
