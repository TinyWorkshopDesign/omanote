//! Re-upload of local data to a server that lost it (no E2EE).
//!
//! `seed <db>`: create a folder, a note and an attachment, then sync.
//! `reupload <db>`: re-upload everything in `<db>` (run after wiping the server).
//! `edit <db> <line>` / `edit-sync <db> <line>`: add a line to the note (and sync).
//! `show <db>`: list notes with their conflict flag and last line.
//! `purge <db>`: trash a note, delete it for good, sync.
//! `check <db> <sha256>`: sync a fresh store, print what came down and the blob hash.

use std::sync::Mutex;

use omanote_core::api::{JoplinServer, LOCK_CLIENT_DESKTOP};
use omanote_core::e2ee::KeyRing;
use omanote_core::store::Store;
use omanote_core::sync::Synchronizer;
use sha2::{Digest, Sha256};

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("missing {k}"))
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::main]
async fn main() -> omanote_core::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let store = Mutex::new(Store::open(&args[1])?);
    let mut api = JoplinServer::new(&env("OMANOTE_URL"), &env("OMANOTE_EMAIL"), &env("OMANOTE_PASSWORD"));
    let mut keys = KeyRing::new();
    let mut sync = Synchronizer {
        api: &mut api,
        store: &store,
        keys: &mut keys,
        client_id: "reuploadtest00000000000000000000".into(),
        client_type: LOCK_CLIENT_DESKTOP,
    };
    match args[0].as_str() {
        "seed" => {
            {
                let db = store.lock().unwrap();
                let f = db.create_folder("Lavoro", "")?;
                let img = std::fs::read(&args[2]).expect("image");
                let id = db.add_resource(&img, "image/png", "foto.png")?;
                db.create_note(&f.id, &format!("Nota con foto\n![foto.png](:/{id})"))?;
                println!("sha256 {}", hex(&img));
            }
            println!("{:?}", sync.sync().await?);
        }
        "reupload" => println!("{:?}", sync.reupload_all().await?),
        "edit" | "edit-sync" => {
            {
                let db = store.lock().unwrap();
                let all: Vec<String> = db.folders()?.into_iter().map(|f| f.id).collect();
                let note = db.notes_in(&all)?.into_iter().find(|n| n.title == "Nota con foto").expect("a note");
                let text = db.note(&note.id)?.unwrap().text;
                db.update_note_text(&note.id, &format!("{text}\n{}", args[2]))?;
            }
            if args[0] == "edit-sync" {
                println!("{:?}", sync.sync().await?);
            }
        }
        "show" => {
            let db = store.lock().unwrap();
            let all: Vec<String> = db.folders()?.into_iter().map(|f| f.id).collect();
            for n in db.notes_in(&all)? {
                println!("{:?} conflict={} last line={:?}", n.title, n.is_conflict, db.note(&n.id)?.unwrap().text.lines().last());
            }
        }
        "purge" => {
            {
                let db = store.lock().unwrap();
                let all: Vec<String> = db.folders()?.into_iter().map(|f| f.id).collect();
                let note = db.notes_in(&all)?.into_iter().next().expect("a note");
                db.trash(&note.id)?;
                db.purge(&note.id)?;
                println!("purged {:?}", note.title);
            }
            println!("{:?}", sync.sync().await?);
        }
        "check" => {
            println!("{:?}", sync.sync().await?);
            let (note, id) = {
                let db = store.lock().unwrap();
                let all: Vec<String> = db.folders()?.into_iter().map(|f| f.id).collect();
                println!("folders: {:?}", db.folders()?.iter().map(|f| &f.title).collect::<Vec<_>>());
                let note = db.notes_in(&all)?.into_iter().next().expect("a note");
                let text = db.note(&note.id)?.unwrap().text;
                let at = text.find("](:/").expect("resource link") + 4;
                (note.title, text[at..at + 32].to_string())
            };
            let path = sync.fetch_resource(&id).await?.expect("blob");
            let sha = hex(&std::fs::read(&path).unwrap());
            println!("note {note:?}, blob sha256 {sha}, match = {}", sha == args[2]);
        }
        _ => println!("usage: seed <db> <png> | reupload <db> | purge <db> | check <db> <sha256>"),
    }
    Ok(())
}
