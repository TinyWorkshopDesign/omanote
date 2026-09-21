//! End-to-end check against a real Joplin Server.
//!
//! OMANOTE_URL=http://localhost:22300 OMANOTE_EMAIL=admin@localhost OMANOTE_PASSWORD=admin \
//! OMANOTE_MASTER=segreto123 OMANOTE_DB=/tmp/omanote.db cargo run --example sync_e2e -- [write]

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
    let write = std::env::args().any(|a| a == "write");
    let store = Mutex::new(Store::open(env("OMANOTE_DB"))?);
    let mut api = JoplinServer::new(&env("OMANOTE_URL"), &env("OMANOTE_EMAIL"), &env("OMANOTE_PASSWORD"));
    let mut keys = KeyRing::new();

    let mut sync = Synchronizer {
        api: &mut api,
        store: &store,
        keys: &mut keys,
        client_id: "omanotee2etest0000000000000000".into(),
        client_type: LOCK_CLIENT_DESKTOP,
    };
    let info = sync.fetch_info().await?;
    let unlocked = unlock_keys(&info, &env("OMANOTE_MASTER"), sync.keys);
    println!("e2ee={} master keys unlocked: {unlocked}/{}", info.e2ee_enabled(), info.master_keys.len());

    let t = std::time::Instant::now();
    let report = sync.sync().await?;
    println!("sync #1 {:?}: {report:?}", t.elapsed());

    {
        let db = store.lock().unwrap();
        for f in db.folders()? {
            println!("📁 {} ({} note)", f.title, f.note_count);
            for n in db.notes_in(&[f.id.clone()])? {
                let full = db.note(&n.id)?.unwrap();
                println!("   📝 {:?}", full.text);
            }
        }
        if write {
            let folders = db.folders()?;
            let test = folders.iter().find(|f| f.title == "Test").expect("Test folder");
            let spesa = db.notes_in(&[test.id.clone()])?.into_iter().find(|n| n.title == "Spesa").unwrap();
            db.update_note_text(&spesa.id, "Spesa\n- latte\n- pane 😀\n- uova (da Omanote)")?;
            db.create_note(&test.id, "Nota da Omanote ✓\nscritta in Rust, cifrata E2EE")?;
            db.create_folder("Omanote Inbox", "")?;
        }
    }

    if write {
        let report = sync.sync().await?;
        println!("sync #2: {report:?}");
    }
    Ok(())
}
