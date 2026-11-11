//! Test support for the sync engine: an in-process fake Joplin Server and a
//! simulated device (its own database, attachment folder, API client and keys).
//!
//! The fake implements only what `src/api.rs` calls, over blocking HTTP/1.1 on
//! `std::net::TcpListener` (one thread per connection, `Connection: close`).

#![allow(dead_code)]

use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use omanote_core::api::{JoplinServer, LOCK_CLIENT_DESKTOP};
use omanote_core::e2ee::KeyRing;
use omanote_core::item::{self, RawItem};
use omanote_core::store::Store;
use omanote_core::sync::{LockTiming, SyncReport, Synchronizer};
use serde_json::{json, Value};

/// Changes per delta page: small on purpose, so every test crosses page boundaries.
pub const DELTA_PAGE: usize = 3;

pub const MASTER_KEY_ID: &str = "0123456789abcdef0123456789abcdef";

/// `info.json` of a sync target without E2EE.
pub fn info_plain() -> String {
    json!({ "version": 3 }).to_string()
}

/// `info.json` of a sync target with E2EE on and `MASTER_KEY_ID` active. The key
/// content is never decrypted by the tests: devices get the plaintext key directly.
pub fn info_e2ee() -> String {
    json!({
        "version": 3,
        "e2ee": { "value": true, "updatedTime": 1 },
        "activeMasterKeyId": { "value": MASTER_KEY_ID, "updatedTime": 1 },
        "masterKeys": [{
            "id": MASTER_KEY_ID,
            "created_time": 1,
            "updated_time": 1,
            "source_application": "test",
            "encryption_method": 8,
            "checksum": "",
            "content": "{}",
            "hasBeenUsed": true
        }],
        "ppk": { "value": null, "updatedTime": 0 },
        "appMinVersion": "3.0.0"
    })
    .to_string()
}

/// Plaintext master key used by every E2EE test device.
pub fn master_key() -> String {
    "ab".repeat(256)
}

#[derive(Debug, Clone)]
struct Change {
    name: String,
    change_type: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    pub lock_type: i64,
    pub client_type: i64,
    pub client_id: String,
}

/// What happens to the locks after a number of item requests (`lose_lock_after`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockLoss {
    /// The server's clock says every lock is stale, from then on (sticky, until
    /// `fix_lock_clock`): locks are dropped, `GET api/locks` lists none, and a lock taken
    /// again is dropped at the next item request. A refresh can never see ours, whatever
    /// the order of its list and acquire around the item requests.
    Expired,
    /// The locks expire and another client takes an exclusive lock, which stays: a
    /// refresh then either sees it in the list or gets 409 when acquiring.
    TakenExclusive,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
}

#[derive(Default)]
struct State {
    /// Bumped by `wipe`: cursors from an earlier "server" are no longer valid.
    epoch: u64,
    items: BTreeMap<String, Vec<u8>>,
    log: Vec<Change>,
    locks: Vec<Lock>,
    sessions: HashSet<String>,
    requests: Vec<Request>,
    /// `(method, path substring)`: the next matching request is dropped without a reply.
    fail_once: Vec<(String, String)>,
    info_writes: usize,
    /// POSTs to `api/locks` by a client that already held that lock (refreshes).
    lock_refreshes: usize,
    /// Wait before answering each item content request (a slow network).
    delay: Duration,
    /// `(remaining item content requests, what then happens to the locks)`.
    lose_lock: Option<(usize, LockLoss)>,
    /// Set by `LockLoss::Expired`: every lock counts as stale.
    locks_stale: bool,
}

pub struct FakeServer {
    pub url: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    addr: std::net::SocketAddr,
}

impl FakeServer {
    pub fn start(info_json: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(State::default()));
        state.lock().unwrap().items.insert("info.json".into(), info_json.as_bytes().to_vec());
        let stop = Arc::new(AtomicBool::new(false));
        {
            let state = state.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let state = state.clone();
                    std::thread::spawn(move || handle_connection(stream, &state));
                }
            });
        }
        Self { url: format!("http://{addr}"), state, stop, addr }
    }

    fn st(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Raw bytes stored under `name` (`<id>.md`, `.resource/<id>`, `info.json`).
    pub fn item(&self, name: &str) -> Option<Vec<u8>> {
        self.st().items.get(name).cloned()
    }

    pub fn item_text(&self, name: &str) -> Option<String> {
        self.item(name).map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    pub fn has(&self, name: &str) -> bool {
        self.st().items.contains_key(name)
    }

    /// Names of every stored item except `info.json`.
    pub fn names(&self) -> Vec<String> {
        self.st().items.keys().filter(|k| *k != "info.json").cloned().collect()
    }

    /// Stores content as another client would (logged as a change).
    pub fn put_item(&self, name: &str, content: &[u8]) {
        store_item(&mut self.st(), name, content.to_vec());
    }

    /// Replaces content WITHOUT logging a change: models data the client could not
    /// read before and now could (a newer parser), without anyone editing it.
    pub fn replace_silently(&self, name: &str, content: &[u8]) {
        self.st().items.insert(name.to_string(), content.to_vec());
    }

    /// A server that lost its data: every item is gone except `info.json`, and the
    /// change log starts over (old delta cursors ask for a full resync).
    pub fn wipe(&self) {
        let mut st = self.st();
        st.items.retain(|k, _| k == "info.json");
        st.log.clear();
        st.epoch += 1;
    }

    pub fn locks(&self) -> Vec<Lock> {
        self.st().locks.clone()
    }

    /// Simulates a Joplin client holding an exclusive lock (a migration).
    pub fn add_exclusive_lock(&self) {
        self.st().locks.push(Lock { lock_type: 2, client_type: 1, client_id: "otherclient".into() });
    }

    pub fn requests(&self) -> Vec<Request> {
        self.st().requests.clone()
    }

    pub fn clear_requests(&self) {
        self.st().requests.clear();
    }

    /// Requests that changed data (`PUT` and `DELETE` on items) since the last `clear_requests`.
    pub fn writes(&self) -> Vec<Request> {
        self.requests()
            .into_iter()
            .filter(|r| (r.method == "PUT" || r.method == "DELETE") && r.path.starts_with("/api/items/"))
            .collect()
    }

    /// The next request whose method matches and whose path contains `path_part` is
    /// read and then dropped without a response (a network failure).
    pub fn fail_once(&self, method: &str, path_part: &str) {
        self.st().fail_once.push((method.to_string(), path_part.to_string()));
    }

    pub fn info_writes(&self) -> usize {
        self.st().info_writes
    }

    pub fn lock_refreshes(&self) -> usize {
        self.st().lock_refreshes
    }

    /// Every item content request (`<id>.md`, `.resource/<id>`) waits `delay` first.
    pub fn set_delay(&self, delay: Duration) {
        self.st().delay = delay;
    }

    /// After `n` more item content requests the locks are lost as `how` says.
    pub fn lose_lock_after(&self, n: usize, how: LockLoss) {
        self.st().lose_lock = Some((n, how));
    }

    /// Ends `LockLoss::Expired`: locks are kept and listed again.
    pub fn fix_lock_clock(&self) {
        self.st().locks_stale = false;
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop so its thread ends.
        let _ = TcpStream::connect(self.addr);
    }
}

fn store_item(st: &mut State, name: &str, content: Vec<u8>) {
    let change_type = if st.items.contains_key(name) { 2 } else { 1 };
    st.items.insert(name.to_string(), content);
    st.log.push(Change { name: name.to_string(), change_type });
}

struct HttpRequest {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn read_request(stream: &TcpStream) -> Option<HttpRequest> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        reader.read_line(&mut h).ok()?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        let (k, v) = h.split_once(':')?;
        headers.push((k.trim().to_string(), v.trim().to_string()));
    }
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; len];
    reader.read_exact(&mut body).ok()?;
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    let query = query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect();
    Some(HttpRequest { method, path: percent_decode(&path), query, headers, body })
}

struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

fn json_reply(status: u16, v: Value) -> Reply {
    Reply { status, content_type: "application/json", body: v.to_string().into_bytes() }
}

fn error_reply(status: u16, code: &str, message: &str) -> Reply {
    json_reply(status, json!({ "error": message, "code": code }))
}

fn handle_connection(mut stream: TcpStream, state: &Mutex<State>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let Some(req) = read_request(&stream) else { return };
    let is_item_content = req.path.ends_with(":/content") && !req.path.contains("info.json");
    if is_item_content {
        let delay = state.lock().unwrap_or_else(|e| e.into_inner()).delay;
        std::thread::sleep(delay);
    }
    let reply = {
        let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
        st.requests.push(Request { method: req.method.clone(), path: req.path.clone() });
        if is_item_content {
            if let Some((n, how)) = st.lose_lock {
                if n <= 1 {
                    st.lose_lock = None;
                    st.locks.clear();
                    match how {
                        LockLoss::Expired => st.locks_stale = true,
                        LockLoss::TakenExclusive => {
                            st.locks.push(Lock { lock_type: 2, client_type: 1, client_id: "otherclient".into() })
                        }
                    }
                } else {
                    st.lose_lock = Some((n - 1, how));
                }
            }
            if st.locks_stale {
                st.locks.clear();
            }
        }
        if std::env::var_os("OMANOTE_FAKE_SERVER_LOG").is_some() {
            eprintln!("fake server: {} {}", req.method, req.path);
        }
        if let Some(i) = st.fail_once.iter().position(|(m, p)| *m == req.method && req.path.contains(p.as_str())) {
            st.fail_once.remove(i);
            None
        } else {
            Some(route(&mut st, &req))
        }
    };
    let Some(reply) = reply else {
        // Network failure: close the connection without answering.
        let _ = stream.shutdown(std::net::Shutdown::Both);
        return;
    };
    let reason = match reply.status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        reply.content_type,
        reply.body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&reply.body);
    let _ = stream.flush();
}

/// `updated_time` of a stored item in ms, as Joplin Server's `jop_updated_time`
/// (kept in clear text on encrypted envelopes too).
fn jop_updated_time(content: &[u8]) -> Option<i64> {
    let text = std::str::from_utf8(content).ok()?;
    RawItem::parse(text).ok().map(|it| it.time("updated_time"))
}

fn route(st: &mut State, req: &HttpRequest) -> Reply {
    if req.method == "POST" && req.path == "/api/sessions" {
        let id = item::new_id();
        st.sessions.insert(id.clone());
        return json_reply(200, json!({ "id": id, "user_id": "testuser" }));
    }
    match req.header("X-API-AUTH") {
        Some(sid) if st.sessions.contains(sid) => {}
        _ => return error_reply(403, "invalidSession", "Invalid session"),
    }

    if let Some(rest) = req.path.strip_prefix("/api/locks") {
        return route_locks(st, req, rest);
    }

    let Some(rest) = req.path.strip_prefix("/api/items/root:/") else {
        return error_reply(404, "notFound", &format!("no route for {}", req.path));
    };

    if rest == ":/delta" && req.method == "GET" {
        return delta(st, req);
    }
    if let Some(name) = rest.strip_suffix(":/content") {
        return match req.method.as_str() {
            "GET" => match st.items.get(name) {
                Some(b) => Reply { status: 200, content_type: "application/octet-stream", body: b.clone() },
                None => error_reply(404, "notFound", &format!("{name} not found")),
            },
            "PUT" => {
                if name == "info.json" {
                    st.info_writes += 1;
                }
                if name.starts_with("locks/") || name.starts_with("temp/") {
                    return error_reply(400, "invalidName", "clients must not write there");
                }
                store_item(st, name, req.body.clone());
                json_reply(200, json!({ "name": name }))
            }
            _ => error_reply(405, "methodNotAllowed", "method not allowed"),
        };
    }
    if let Some(name) = rest.strip_suffix(':') {
        if req.method == "DELETE" {
            if st.items.remove(name).is_none() {
                return error_reply(404, "notFound", &format!("{name} not found"));
            }
            st.log.push(Change { name: name.to_string(), change_type: 3 });
            return json_reply(200, json!({}));
        }
    }
    error_reply(404, "notFound", &format!("no route for {} {}", req.method, req.path))
}

fn delta(st: &mut State, req: &HttpRequest) -> Reply {
    let start = match req.query.iter().find(|(k, _)| k == "cursor").map(|(_, v)| v.as_str()) {
        None | Some("") => 0,
        Some(c) => {
            let parsed = c
                .strip_prefix('e')
                .and_then(|c| c.split_once('-'))
                .and_then(|(e, i)| Some((e.parse::<u64>().ok()?, i.parse::<usize>().ok()?)));
            match parsed {
                Some((e, i)) if e == st.epoch && i <= st.log.len() => i,
                _ => return error_reply(400, "resyncRequired", "Delta cursor is invalid: a full resync is required"),
            }
        }
    };
    let end = (start + DELTA_PAGE).min(st.log.len());
    let items: Vec<Value> = st.log[start..end]
        .iter()
        .map(|c| {
            // Like Joplin Server: the item's current updated time, none once deleted.
            let jop = st.items.get(&c.name).and_then(|b| jop_updated_time(b));
            json!({ "item_name": c.name, "type": c.change_type, "jop_updated_time": jop })
        })
        .collect();
    json_reply(
        200,
        json!({ "items": items, "cursor": format!("e{}-{end}", st.epoch), "has_more": end < st.log.len() }),
    )
}

fn route_locks(st: &mut State, req: &HttpRequest, rest: &str) -> Reply {
    if req.method == "POST" && rest.is_empty() {
        let v: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
        let lock = Lock {
            lock_type: v["type"].as_i64().unwrap_or(0),
            client_type: v["clientType"].as_i64().unwrap_or(0),
            client_id: v["clientId"].as_str().unwrap_or("").to_string(),
        };
        if st.locks.iter().any(|l| l.lock_type == 2 && l.client_id != lock.client_id) {
            return error_reply(409, "hasExclusiveLock", "an exclusive lock is held by another client");
        }
        if st.locks.contains(&lock) {
            st.lock_refreshes += 1; // like Joplin Server: the same client's lock gets a new updatedTime
        } else {
            st.locks.push(lock.clone());
        }
        return json_reply(
            200,
            json!({ "type": lock.lock_type, "clientType": lock.client_type, "clientId": lock.client_id, "updatedTime": item::now_ms() }),
        );
    }
    if req.method == "GET" && rest.is_empty() {
        // Every lock was refreshed by its owner as far as this fake knows: updatedTime = now.
        // With a stale clock none is active, so none is listed.
        let listed = if st.locks_stale { &[][..] } else { &st.locks[..] };
        let items: Vec<Value> = listed
            .iter()
            .map(|l| {
                json!({
                    "id": item::new_id(),
                    "type": l.lock_type,
                    "clientType": l.client_type,
                    "clientId": l.client_id,
                    "updatedTime": item::now_ms(),
                })
            })
            .collect();
        return json_reply(200, json!({ "items": items, "has_more": false }));
    }
    if req.method == "DELETE" {
        if let Some(key) = rest.strip_prefix('/') {
            let mut parts = key.splitn(3, '_');
            let (Some(t), Some(ct), Some(cid)) = (parts.next(), parts.next(), parts.next()) else {
                return error_reply(400, "invalidLock", "bad lock name");
            };
            let before = st.locks.len();
            st.locks.retain(|l| !(l.lock_type.to_string() == t && l.client_type.to_string() == ct && l.client_id == cid));
            if st.locks.len() == before {
                return error_reply(404, "notFound", "no such lock");
            }
            return json_reply(200, json!({}));
        }
    }
    error_reply(404, "notFound", "no such lock route")
}

// -- simulated device ------------------------------------------------------

/// One simulated device: its own data directory (database + `resources/`), API
/// client, key ring and client id.
pub struct Device {
    pub dir: PathBuf,
    pub store: Mutex<Store>,
    pub api: JoplinServer,
    pub keys: KeyRing,
    pub client_id: String,
    /// Lock refresh timings; the defaults are Joplin's (refresh every minute).
    pub lock_timing: LockTiming,
}

impl Device {
    pub fn new(server: &FakeServer) -> Self {
        let dir = std::env::temp_dir().join(format!("omanote-sync-test-{}", item::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(dir.join("omanote.sqlite")).unwrap();
        Self {
            dir,
            store: Mutex::new(store),
            api: JoplinServer::new(&server.url, "user@example.com", "pw"),
            keys: KeyRing::new(),
            client_id: item::new_id(),
            lock_timing: LockTiming::default(),
        }
    }

    /// A device that already holds the plaintext master key.
    pub fn with_key(server: &FakeServer) -> Self {
        let mut d = Self::new(server);
        d.give_key();
        d
    }

    pub fn give_key(&mut self) {
        self.keys.insert(MASTER_KEY_ID, master_key());
    }

    pub fn db(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap()
    }

    fn synchronizer(&mut self) -> Synchronizer<'_> {
        Synchronizer {
            api: &mut self.api,
            store: &self.store,
            keys: &mut self.keys,
            client_id: self.client_id.clone(),
            client_type: LOCK_CLIENT_DESKTOP,
        }
    }

    pub async fn sync(&mut self) -> omanote_core::Result<SyncReport> {
        let timing = self.lock_timing;
        self.synchronizer().sync_with(false, timing).await
    }

    pub async fn reupload_all(&mut self) -> omanote_core::Result<SyncReport> {
        let timing = self.lock_timing;
        self.synchronizer().sync_with(true, timing).await
    }

    pub async fn fetch_resource(&mut self, id: &str) -> omanote_core::Result<Option<PathBuf>> {
        self.synchronizer().fetch_resource(id).await
    }

    pub fn decrypt_pending(&mut self) -> omanote_core::Result<usize> {
        self.synchronizer().decrypt_pending()
    }

    /// Waits a few milliseconds before a local change. Timestamps are in ms and
    /// conflict detection compares them, so two successive edits (on any device)
    /// must never share a millisecond.
    pub fn tick() {
        std::thread::sleep(Duration::from_millis(3));
    }

    pub fn create_folder(&self, title: &str) -> String {
        Self::tick();
        self.db().create_folder(title, "").unwrap().id
    }

    pub fn create_note(&self, folder: &str, text: &str) -> String {
        Self::tick();
        self.db().create_note(folder, text).unwrap().id
    }

    pub fn edit(&self, id: &str, text: &str) {
        Self::tick();
        self.db().update_note_text(id, text).unwrap();
    }

    /// Text of a note (title line + body), `None` if unknown.
    pub fn text(&self, id: &str) -> Option<String> {
        self.db().note(id).unwrap().map(|n| n.text)
    }

    /// Every note in the store (trashed ones included), with its raw item.
    pub fn all_notes(&self) -> Vec<RawItem> {
        let db = self.db();
        let mut ids: Vec<String> = Vec::new();
        let folders: Vec<String> = db.folders().unwrap().into_iter().map(|f| f.id).collect();
        for n in db.notes_in(&folders).unwrap() {
            ids.push(n.id);
        }
        for t in db.trashed("").unwrap() {
            if !t.is_folder {
                ids.push(t.id);
            }
        }
        ids.into_iter().filter_map(|id| db.raw(&id).unwrap().map(|r| r.0)).collect()
    }

    /// Conflict copies held by this device.
    pub fn conflicts(&self) -> Vec<RawItem> {
        self.all_notes().into_iter().filter(|n| n.int("is_conflict") == 1).collect()
    }

    pub fn resources_dir(&self) -> PathBuf {
        self.dir.join("resources")
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}
