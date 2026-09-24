//! End-to-end check against a real Joplin Server.
//!
//! OMANOTE_URL=… OMANOTE_EMAIL=… OMANOTE_PASSWORD=… OMANOTE_MASTER=… OMANOTE_DB=…
//! cargo run --example sync_e2e -- [write] [edit <title> <text>] [list]
//!
//! "edit" changes a note locally WITHOUT syncing, which is how the conflict
//! path is exercised: edit the same note in Joplin, sync Joplin, then sync here.

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let write = args.iter().any(|a| a == "write");
    let edit = args.iter().position(|a| a == "edit").map(|i| (args[i + 1].clone(), args[i + 2].clone()));
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

    if let Some((title, text)) = edit {
        let db = store.lock().unwrap();
        let all: Vec<String> = db.folders()?.into_iter().map(|f| f.id).collect();
        let n = db
            .notes_in(&all)?
            .into_iter()
            .find(|n| n.title == title)
            .unwrap_or_else(|| panic!("note {title:?} not found"));
        db.update_note_text(&n.id, &text)?;
        println!("edited locally (not synced): {title}");
        return Ok(());
    }

    let t = std::time::Instant::now();
    let report = sync.sync().await?;
    println!("sync #1 {:?}: {report:?}", t.elapsed());

    {
        let db = store.lock().unwrap();
        for f in db.folders()? {
            println!("📁 {} ({} notes)", f.title, f.note_count);
            for n in db.notes_in(&[f.id.clone()])? {
                let full = db.note(&n.id)?.unwrap();
                println!("   📝 {:?}", full.text);
            }
        }
        if write {
            let folders = db.folders()?;
            let test = folders.iter().find(|f| f.title == "Test").expect("Test folder");
            let groceries = db.notes_in(&[test.id.clone()])?.into_iter().find(|n| n.title == "Groceries").unwrap();
            db.update_note_text(&groceries.id, "Groceries\n- milk\n- bread 😀\n- eggs (from Omanote)")?;
            db.create_note(&test.id, "Note from Omanote ✓\nwritten in Rust, E2EE-encrypted")?;
            db.create_folder("Omanote Inbox", "")?;
        }
    }

    if write {
        let report = sync.sync().await?;
        println!("sync #2: {report:?}");
    }
    Ok(())
}
