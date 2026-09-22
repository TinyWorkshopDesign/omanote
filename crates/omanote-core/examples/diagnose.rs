//! Read-only look at what the configured Joplin Server holds: every item in the
//! delta from scratch, with its type, title, parent and trash state. Only GET
//! requests; bodies are never printed.
//!
//! cargo run -p omanote-core --example diagnose [id-prefix…]

use std::collections::HashMap;

use omanote_core::api::JoplinServer;
use omanote_core::config::{self, Config};
use omanote_core::item::RawItem;
use omanote_core::secrets;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let dir = config::default_data_dir();
    let cfg = Config::load(&dir);
    let password = secrets::get(&dir, secrets::SERVER_PASSWORD).expect("no saved password");
    let mut api = JoplinServer::new(&cfg.server_url, &cfg.email, &password);
    api.login().await.expect("login");

    let mut history: HashMap<String, Vec<i64>> = HashMap::new();
    let mut order = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = api.delta(cursor.as_deref()).await.expect("delta");
        for ch in &page.items {
            let Some(id) = ch.item_name.strip_suffix(".md").filter(|s| s.len() == 32) else { continue };
            if !history.contains_key(id) {
                order.push(id.to_string());
            }
            history.entry(id.to_string()).or_default().push(ch.change_type);
        }
        cursor = Some(page.cursor);
        if !page.has_more {
            break;
        }
    }
    println!("{} items in the delta", order.len());

    let extra: Vec<String> = std::env::args().skip(1).collect();
    for id in &order {
        let changes = &history[id];
        let Some(content) = api.get(&format!("{id}.md")).await.expect("get") else {
            println!("{} gone  changes={changes:?}", &id[..8]);
            continue;
        };
        match RawItem::parse(&content) {
            Ok(it) => println!(
                "{} type={} enc={} title={:?} parent={} deleted={} updated={} changes={changes:?}",
                &id[..8],
                it.item_type(),
                it.get("encryption_applied"),
                it.get("title"),
                it.get("parent_id").get(..8).unwrap_or(""),
                it.get("deleted_time"),
                it.get("updated_time"),
            ),
            Err(e) => println!("{} PARSE ERROR {e}", &id[..8]),
        }
    }
    for key in extra {
        let found: Vec<_> = order.iter().filter(|id| id.starts_with(&key)).collect();
        println!("{key}: in delta = {}", !found.is_empty());
    }
}
