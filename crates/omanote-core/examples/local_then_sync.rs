//! Local mode → connect a server: notes written with no server must be
//! uploaded (not dropped) on the first sync.
//!
//! OMANOTE_URL=… OMANOTE_EMAIL=… OMANOTE_PASSWORD=… OMANOTE_MASTER=… OMANOTE_DB=/tmp/new.db \
//! cargo run --example local_then_sync

use std::sync::Mutex;

use omanote_core::api::{JoplinServer, LOCK_CLIENT_DESKTOP};
use omanote_core::e2ee::KeyRing;
use omanote_core::store::Store;
use omanote_core::sync::{unlock_keys, Synchronizer};

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("missing {k}"))
}

#[tokio::main]
async fn main() -> omanote_core::Result<()> {
    let _ = std::fs::remove_file(env("OMANOTE_DB"));
    let store = Mutex::new(Store::open(env("OMANOTE_DB"))?);

    // 1. Local mode: no server at all.
    let (root, note) = {
        let db = store.lock().unwrap();
        let root = db.create_folder("Omanote Local", "")?;
        let sub = db.create_folder("Ideas", &root.id)?;
        db.create_note(&root.id, "Offline note\nwritten without a server\nx = 21\nx * 2")?;
        let note = db.create_note(&sub.id, "list: To do\n- [ ] connect Joplin")?;
        println!("local: {} items to upload", db.dirty_items()?.len());
        (root, note)
    };

    // 2. The user connects a Joplin Server later.
    let mut api = JoplinServer::new(&env("OMANOTE_URL"), &env("OMANOTE_EMAIL"), &env("OMANOTE_PASSWORD"));
    let mut keys = KeyRing::new();
    let mut sync = Synchronizer {
        api: &mut api,
        store: &store,
        keys: &mut keys,
        client_id: "localmodetest00000000000000000000".into(),
        client_type: LOCK_CLIENT_DESKTOP,
    };
    let info = sync.fetch_info().await?;
    unlock_keys(&info, &env("OMANOTE_MASTER"), sync.keys);
    let report = sync.sync().await?;
    println!("sync: {report:?}");

    let db = store.lock().unwrap();
    assert_eq!(db.dirty_items()?.len(), 0, "everything uploaded");
    assert!(db.note(&note.id)?.is_some(), "local note kept");
    assert!(report.uploaded >= 4, "folders and notes uploaded");
    println!("ok: root {} and local notes uploaded to the server", root.id);
    Ok(())
}
