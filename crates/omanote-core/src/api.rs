//! Minimal Joplin Server client (same endpoints as `JoplinServerApi` +
//! `FileApiDriverJoplinServer`). The server is used as-is: Omanote never
//! changes its schema, `info.json` or anything outside the item files.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{Error, Result};

pub const LOCK_SYNC: i64 = 1;
pub const LOCK_CLIENT_DESKTOP: i64 = 1;
pub const LOCK_CLIENT_MOBILE: i64 = 2;

#[derive(Debug, Clone, Deserialize)]
pub struct Session {
    pub id: String,
    #[serde(default)]
    pub user_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaItem {
    pub item_name: String,
    /// 1 = create, 2 = update, 3 = delete
    #[serde(rename = "type")]
    pub change_type: i64,
    #[serde(default)]
    pub jop_updated_time: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaPage {
    pub items: Vec<DeltaItem>,
    pub cursor: String,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Clone)]
pub struct JoplinServer {
    http: reqwest::Client,
    base_url: String,
    email: String,
    password: String,
    session: Option<Session>,
}

impl JoplinServer {
    pub fn new(base_url: &str, email: &str, password: &str) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("Omanote/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("http client");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            email: email.to_string(),
            password: password.to_string(),
            session: None,
        }
    }

    pub fn with_session(mut self, session_id: Option<String>) -> Self {
        self.session = session_id.map(|id| Session { id, user_id: String::new() });
        self
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.id.as_str())
    }

    pub async fn login(&mut self) -> Result<()> {
        let body = json!({
            "email": self.email,
            "password": self.password,
            "platform": std::env::consts::OS,
            "type": "omanote",
            "version": env!("CARGO_PKG_VERSION"),
        });
        let res = self
            .http
            .post(format!("{}/api/sessions", self.base_url))
            .json(&body)
            .send()
            .await?;
        let res = check(res).await?;
        self.session = Some(res.json().await?);
        Ok(())
    }

    async fn ensure_session(&mut self) -> Result<String> {
        if self.session.is_none() {
            self.login().await?;
        }
        Ok(self.session.as_ref().unwrap().id.clone())
    }

    /// Sends a request, re-authenticating once if the session expired (403).
    async fn send(
        &mut self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Body>,
    ) -> Result<reqwest::Response> {
        for attempt in 0..2 {
            let sid = self.ensure_session().await?;
            let mut req = self
                .http
                .request(method.clone(), format!("{}/{}", self.base_url, path))
                .header("X-API-AUTH", sid)
                .header("X-API-MIN-VERSION", "2.6.0");
            if !query.is_empty() {
                req = req.query(query);
            }
            match &body {
                Some(Body::Json(v)) => req = req.json(v),
                Some(Body::Bytes(b)) => {
                    req = req
                        .header("Content-Type", "application/octet-stream")
                        .body(b.clone())
                }
                None => {}
            }
            let res = req.send().await?;
            if res.status().as_u16() == 403 && attempt == 0 {
                self.session = None;
                continue;
            }
            return Ok(res);
        }
        unreachable!()
    }

    fn item_path(name: &str) -> String {
        format!("api/items/root:/{}:", name.trim_matches('/'))
    }

    /// GET the content of a file (`info.json`, `<id>.md`, …). `None` on 404.
    pub async fn get(&mut self, name: &str) -> Result<Option<String>> {
        let path = format!("{}/content", Self::item_path(name));
        let res = self.send(reqwest::Method::GET, &path, &[], None).await?;
        if res.status().as_u16() == 404 {
            return Ok(None);
        }
        Ok(Some(check(res).await?.text().await?))
    }

    pub async fn get_bytes(&mut self, name: &str) -> Result<Option<Vec<u8>>> {
        let path = format!("{}/content", Self::item_path(name));
        let res = self.send(reqwest::Method::GET, &path, &[], None).await?;
        if res.status().as_u16() == 404 {
            return Ok(None);
        }
        Ok(Some(check(res).await?.bytes().await?.to_vec()))
    }

    pub async fn put(&mut self, name: &str, content: Vec<u8>) -> Result<()> {
        let path = format!("{}/content", Self::item_path(name));
        let res = self
            .send(reqwest::Method::PUT, &path, &[], Some(Body::Bytes(content)))
            .await?;
        check(res).await?;
        Ok(())
    }

    pub async fn delete(&mut self, name: &str) -> Result<()> {
        let path = Self::item_path(name);
        let res = self.send(reqwest::Method::DELETE, &path, &[], None).await?;
        if res.status().as_u16() == 404 {
            return Ok(());
        }
        check(res).await?;
        Ok(())
    }

    /// One page of changes since `cursor`. Restarts from scratch if the
    /// server asks for a full resync.
    pub async fn delta(&mut self, cursor: Option<&str>) -> Result<DeltaPage> {
        let path = format!("{}/delta", Self::item_path(""));
        let query: Vec<(&str, &str)> = cursor.map(|c| vec![("cursor", c)]).unwrap_or_default();
        let res = self.send(reqwest::Method::GET, &path, &query, None).await?;
        match check(res).await {
            Ok(r) => Ok(r.json().await?),
            Err(Error::Server { code: Some(c), .. }) if c == "resyncRequired" && cursor.is_some() => {
                Box::pin(self.delta(None)).await
            }
            Err(e) => Err(e),
        }
    }

    pub async fn acquire_lock(&mut self, client_type: i64, client_id: &str) -> Result<()> {
        let body = json!({ "type": LOCK_SYNC, "clientType": client_type, "clientId": client_id });
        let res = self
            .send(reqwest::Method::POST, "api/locks", &[], Some(Body::Json(body)))
            .await?;
        check(res).await?;
        Ok(())
    }

    pub async fn release_lock(&mut self, client_type: i64, client_id: &str) -> Result<()> {
        let path = format!("api/locks/{LOCK_SYNC}_{client_type}_{client_id}");
        let res = self.send(reqwest::Method::DELETE, &path, &[], None).await?;
        if res.status().as_u16() != 404 {
            check(res).await?;
        }
        Ok(())
    }
}

enum Body {
    Json(Value),
    Bytes(Vec<u8>),
}

async fn check(res: reqwest::Response) -> Result<reqwest::Response> {
    if res.status().is_success() {
        return Ok(res);
    }
    let status = res.status().as_u16();
    let text = res.text().await.unwrap_or_default();
    let (code, message) = match serde_json::from_str::<Value>(&text) {
        Ok(v) => (
            v.get("code").and_then(|c| c.as_str()).map(String::from),
            v.get("error").and_then(|e| e.as_str()).unwrap_or(&text).to_string(),
        ),
        Err(_) => (None, text.chars().take(500).collect()),
    };
    Err(Error::Server { status, code, message })
}
