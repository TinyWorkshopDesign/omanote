//! Attachments round-trip with Joplin (E2EE on).
//!
//! `upload <png>`: add the image as a resource + a note showing it, then sync.
//! `fetch <note title>`: sync, find `![..](:/id)` in the note, download + decrypt the blob,
//! print its path and size.

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
    let store = Mutex::new(Store::open(env("OMANOTE_DB"))?);
    let mut api = JoplinServer::new(&env("OMANOTE_URL"), &env("OMANOTE_EMAIL"), &env("OMANOTE_PASSWORD"));
    let mut keys = KeyRing::new();
    let mut sync = Synchronizer {
        api: &mut api,
        store: &store,
        keys: &mut keys,
        client_id: "resourcetest00000000000000000000".into(),
        client_type: LOCK_CLIENT_DESKTOP,
    };
    let info = sync.fetch_info().await?;
    unlock_keys(&info, &env("OMANOTE_MASTER"), sync.keys);
    sync.sync().await?;

    match args.first().map(String::as_str) {
        Some("upload") => {
            let bytes = std::fs::read(&args[1]).expect("image");
            {
                let db = store.lock().unwrap();
                let folder = db.folders()?.into_iter().find(|f| f.title == "Test").expect("Test folder").id;
                let id = db.add_resource(&bytes, "image/png", "screenshot.png")?;
                db.create_note(&folder, &format!("Image from Omanote\n![screenshot.png](:/{id})"))?;
                println!("resource {id} ({} bytes)", bytes.len());
            }
            println!("sync: {:?}", sync.sync().await?);
        }
        Some("fetch") => {
            let title = &args[1];
            let id = {
                let db = store.lock().unwrap();
                let all: Vec<String> = db.folders()?.into_iter().map(|f| f.id).collect();
                let note = db.notes_in(&all)?.into_iter().find(|n| &n.title == title).expect("note");
                let text = db.note(&note.id)?.unwrap().text;
                let at = text.find("](:/").expect("resource link") + 4;
                text[at..at + 32].to_string()
            };
            let path = sync.fetch_resource(&id).await?.expect("blob");
            println!("fetched {} ({} bytes)", path.display(), std::fs::metadata(&path).unwrap().len());
        }
        _ => println!("usage: upload <png> | fetch <note title>"),
    }
    Ok(())
}
