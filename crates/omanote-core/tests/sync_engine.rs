//! Integration tests of the sync engine (`src/sync.rs`) against an in-process fake
//! Joplin Server. Every test starts its own server and devices, so they run in
//! parallel, offline, with no fixed ports.
//!
//! Tests marked `#[ignore]` describe the behaviour the engine SHOULD have and
//! currently fails; the reason names the problem. Run them with
//! `cargo test -p omanote-core --test sync_engine -- --ignored`.

mod common;

use common::{info_e2ee, info_plain, read, Device, FakeServer};
use omanote_core::item::{self, RawItem, TYPE_NOTE};

fn md(id: &str) -> String {
    format!("{id}.md")
}

fn blob(id: &str) -> String {
    format!(".resource/{id}")
}

/// Bytes that look like a PNG, different for every `seed`.
fn png(seed: u8, len: usize) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    v.extend((0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)));
    v
}

/// A folder with one note, synced to both devices.
async fn shared_note(a: &mut Device, b: &mut Device, text: &str) -> (String, String) {
    let folder = a.create_folder("Work");
    let note = a.create_note(&folder, text);
    a.sync().await.unwrap();
    b.sync().await.unwrap();
    assert_eq!(b.text(&note).as_deref(), Some(text));
    (folder, note)
}

// -- 1. basic propagation ----------------------------------------------------

#[tokio::test]
async fn new_folders_and_notes_reach_the_other_device() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);

    let work = a.create_folder("Work");
    let sub = {
        Device::tick();
        a.db().create_folder("Projects", &work).unwrap().id
    };
    let mut notes = Vec::new();
    for i in 0..7 {
        let parent = if i % 2 == 0 { &work } else { &sub };
        let text = format!("Note {i}\nbody line {i}\n- [ ] item {i}");
        notes.push((a.create_note(parent, &text), parent.clone(), text));
    }

    let ra = a.sync().await.unwrap();
    assert_eq!(ra.uploaded, 9);
    assert_eq!((ra.downloaded, ra.conflicts, ra.deleted_local), (0, 0, 0));
    assert!(ra.errors.is_empty(), "{:?}", ra.errors);
    assert!(a.db().dirty_items().unwrap().is_empty());

    let rb = b.sync().await.unwrap();
    assert_eq!(rb.downloaded, 9, "9 items over {} delta pages", 9usize.div_ceil(common::DELTA_PAGE));
    assert_eq!((rb.uploaded, rb.conflicts, rb.still_encrypted), (0, 0, 0));
    assert!(rb.errors.is_empty(), "{:?}", rb.errors);
    assert!(!rb.needs_password);

    let folders = b.db().folders().unwrap();
    let f_work = folders.iter().find(|f| f.id == work).expect("Work folder");
    let f_sub = folders.iter().find(|f| f.id == sub).expect("Projects folder");
    assert_eq!((f_work.title.as_str(), f_work.parent_id.as_str()), ("Work", ""));
    assert_eq!((f_sub.title.as_str(), f_sub.parent_id.as_str()), ("Projects", work.as_str()));
    for (id, parent, text) in &notes {
        let n = b.db().note(id).unwrap().expect("note on B");
        assert_eq!(&n.text, text);
        assert_eq!(&n.parent_id, parent);
        assert!(!n.encrypted);
    }
    assert!(b.db().dirty_items().unwrap().is_empty());
    assert!(b.conflicts().is_empty());
    assert_eq!(server.info_writes(), 0, "info.json is never written");
}

// -- 2. edits and idle syncs -------------------------------------------------

#[tokio::test]
async fn an_edit_travels_without_conflict_and_an_idle_sync_does_nothing() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let (_, note) = shared_note(&mut a, &mut b, "Groceries\n- milk").await;

    a.edit(&note, "Groceries\n- milk\n- bread");
    let ra = a.sync().await.unwrap();
    assert_eq!((ra.uploaded, ra.downloaded, ra.conflicts), (1, 0, 0));

    let rb = b.sync().await.unwrap();
    assert_eq!((rb.uploaded, rb.downloaded, rb.conflicts), (0, 1, 0));
    assert_eq!(b.text(&note).as_deref(), Some("Groceries\n- milk\n- bread"));
    assert!(b.conflicts().is_empty());
    assert!(a.conflicts().is_empty());

    server.clear_requests();
    for d in [&mut a, &mut b] {
        let r = d.sync().await.unwrap();
        assert_eq!((r.uploaded, r.downloaded, r.conflicts, r.deleted_local, r.deleted_remote), (0, 0, 0, 0, 0));
        assert!(r.errors.is_empty());
    }
    assert!(server.writes().is_empty(), "idle syncs wrote {:?}", server.writes());
}

// -- 3. conflicts --------------------------------------------------------------

#[tokio::test]
async fn conflict_detected_on_upload_keeps_both_versions() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let (folder, note) = shared_note(&mut a, &mut b, "Plan\noriginal").await;

    a.edit(&note, "Plan\nedited on A");
    b.edit(&note, "Plan\nedited on B");
    a.sync().await.unwrap();

    // B's upload finds a remote version newer than its last sync.
    let rb = b.sync().await.unwrap();
    assert_eq!(rb.conflicts, 1);
    assert_eq!(rb.uploaded, 1, "only the conflict copy goes up");
    assert_eq!(b.text(&note).as_deref(), Some("Plan\nedited on A"), "remote wins");
    let copies = b.conflicts();
    assert_eq!(copies.len(), 1);
    let copy = &copies[0];
    assert_ne!(copy.id(), note);
    assert_eq!(copy.get("conflict_original_id"), note);
    assert_eq!(copy.get("parent_id"), folder);
    assert_eq!(b.text(copy.id()).as_deref(), Some("Plan\nedited on B"));
    assert!(server.has(&md(copy.id())));
    assert!(b.db().dirty_items().unwrap().is_empty());

    let ra = a.sync().await.unwrap();
    assert_eq!(ra.conflicts, 0);
    assert_eq!(a.text(&note).as_deref(), Some("Plan\nedited on A"));
    let a_copies = a.conflicts();
    assert_eq!(a_copies.len(), 1);
    assert_eq!(a_copies[0].id(), copy.id());
    assert_eq!(a.text(copy.id()).as_deref(), Some("Plan\nedited on B"));

    // Nothing more happens: no second copy anywhere.
    for d in [&mut a, &mut b] {
        let r = d.sync().await.unwrap();
        assert_eq!((r.conflicts, r.uploaded, r.downloaded), (0, 0, 0));
        assert_eq!(d.conflicts().len(), 1);
    }
}

/// The download-side detection: a dirty local note and a newer remote one, with the
/// upload skipped because E2EE is on and B's master key is not loaded (the app was
/// restarted and the password has not been entered yet).
#[tokio::test]
async fn conflict_detected_on_download_while_the_key_is_missing() {
    let server = FakeServer::start(&info_e2ee());
    let mut a = Device::with_key(&server);
    let mut b = Device::with_key(&server);
    let (_, note) = shared_note(&mut a, &mut b, "Trip\noriginal").await;

    b.keys = omanote_core::e2ee::KeyRing::new();
    b.edit(&note, "Trip\nedited on B offline");
    a.edit(&note, "Trip\nedited on A");
    a.sync().await.unwrap();

    server.clear_requests();
    let rb = b.sync().await.unwrap();
    assert!(rb.needs_password);
    assert_eq!(rb.conflicts, 1);
    assert_eq!(rb.uploaded, 0);
    assert!(server.writes().is_empty(), "nothing is uploaded without the key");
    let copies = b.conflicts();
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].get("conflict_original_id"), note);
    assert_eq!(b.text(copies[0].id()).as_deref(), Some("Trip\nedited on B offline"));
    assert!(b.db().note(&note).unwrap().unwrap().encrypted, "the remote version waits for the key");
    assert_eq!(rb.still_encrypted, 1);

    b.give_key();
    let rb = b.sync().await.unwrap();
    assert!(!rb.needs_password);
    assert_eq!((rb.uploaded, rb.conflicts, rb.still_encrypted), (1, 0, 0));
    assert_eq!(b.text(&note).as_deref(), Some("Trip\nedited on A"));

    a.sync().await.unwrap();
    let a_copies = a.conflicts();
    assert_eq!(a_copies.len(), 1);
    assert_eq!(a.text(a_copies[0].id()).as_deref(), Some("Trip\nedited on B offline"));
}

/// Folders are not notes: on a conflict the remote title wins and no copy is made
/// (Joplin does the same). Pinned here so a change is deliberate.
#[tokio::test]
async fn folder_rename_conflict_keeps_the_remote_title() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let (folder, _) = shared_note(&mut a, &mut b, "x").await;

    Device::tick();
    a.db().update_folder(&folder, Some("Renamed on A"), None).unwrap();
    Device::tick();
    b.db().update_folder(&folder, Some("Renamed on B"), None).unwrap();
    a.sync().await.unwrap();
    let rb = b.sync().await.unwrap();
    assert_eq!(rb.conflicts, 1);
    let titles: Vec<String> = b.db().folders().unwrap().into_iter().map(|f| f.title).collect();
    assert_eq!(titles, vec!["Renamed on A".to_string()], "B's rename is dropped");
}

// -- 4. trash ------------------------------------------------------------------

#[tokio::test]
async fn trash_and_restore_travel_both_ways() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let (folder, note) = shared_note(&mut a, &mut b, "Old idea\ndetails").await;

    Device::tick();
    a.db().trash(&note).unwrap();
    a.sync().await.unwrap();
    let rb = b.sync().await.unwrap();
    assert_eq!((rb.downloaded, rb.deleted_local), (1, 0));
    assert!(b.db().notes_in(&[folder.clone()]).unwrap().is_empty());
    let trashed = b.db().trashed("").unwrap();
    assert_eq!(trashed.len(), 1);
    assert_eq!(trashed[0].id, note);
    assert!(trashed[0].deleted_time > 0);
    assert_eq!(b.text(&note).as_deref(), Some("Old idea\ndetails"), "trashed, not deleted");
    assert!(server.has(&md(&note)));

    Device::tick();
    b.db().restore(&note, &folder, "").unwrap();
    b.sync().await.unwrap();
    let ra = a.sync().await.unwrap();
    assert_eq!(ra.downloaded, 1);
    assert!(a.db().trashed("").unwrap().is_empty());
    let listed = a.db().notes_in(&[folder.clone()]).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, note);
}

// -- 5. permanent deletion -------------------------------------------------------

#[tokio::test]
async fn permanent_deletion_removes_the_item_everywhere() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let (home, single) = shared_note(&mut a, &mut b, "Delete me").await;
    let old = a.create_folder("Archive");
    let inside: Vec<String> = (0..2).map(|i| a.create_note(&old, &format!("Archived {i}"))).collect();
    a.sync().await.unwrap();
    b.sync().await.unwrap();
    assert_eq!(b.db().notes_in(&[old.clone()]).unwrap().len(), 2);

    {
        Device::tick();
        let db = a.db();
        db.trash(&single).unwrap();
        db.purge(&single).unwrap();
        db.trash(&old).unwrap();
        db.purge(&old).unwrap();
        assert_eq!(db.pending_deletions().unwrap().len(), 4);
    }
    let ra = a.sync().await.unwrap();
    assert_eq!(ra.deleted_remote, 4);
    assert!(a.db().pending_deletions().unwrap().is_empty());
    for id in [&single, &old, &inside[0], &inside[1]] {
        assert!(!server.has(&md(id)), "{id} still on the server");
    }

    let rb = b.sync().await.unwrap();
    assert_eq!(rb.deleted_local, 4);
    for id in [&single, &old, &inside[0], &inside[1]] {
        assert!(b.db().raw(id).unwrap().is_none(), "{id} still on B");
    }
    assert!(b.db().folders().unwrap().iter().any(|f| f.id == home));
}

/// Joplin deletes `.resource/<id>` together with `<id>.md` when an attachment is
/// deleted; here only the item goes and the file stays on the server for ever.
#[tokio::test]
#[ignore = "deleting an attachment for good leaves its .resource/<id> blob on the server (sync.rs upload deletions)"]
async fn permanently_deleted_attachment_loses_its_blob_on_the_server() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let folder = a.create_folder("Work");
    let res = a.db().add_resource(&png(1, 64), "image/png", "pic.png").unwrap();
    a.create_note(&folder, &format!("Photo\n![pic.png](:/{res})"));
    a.sync().await.unwrap();
    assert!(server.has(&blob(&res)));

    a.db().delete_permanently(&res).unwrap();
    let ra = a.sync().await.unwrap();
    assert_eq!(ra.deleted_remote, 1);
    assert!(!server.has(&md(&res)));
    assert!(!server.has(&blob(&res)), "orphan attachment blob left on the server");
}

/// When an attachment is deleted elsewhere, its downloaded file should go too.
#[tokio::test]
#[ignore = "an attachment deleted remotely keeps its downloaded file in resources/ (Store::remove only drops the row)"]
async fn attachment_deleted_remotely_drops_the_local_file() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    a.create_folder("Work");
    let res = a.db().add_resource(&png(2, 64), "image/png", "pic.png").unwrap();
    a.sync().await.unwrap();
    b.sync().await.unwrap();
    let path = b.fetch_resource(&res).await.unwrap().expect("file on B");

    a.db().delete_permanently(&res).unwrap();
    a.sync().await.unwrap();
    let rb = b.sync().await.unwrap();
    assert_eq!(rb.deleted_local, 1);
    assert!(b.db().raw(&res).unwrap().is_none());
    assert!(!path.exists(), "orphan file left in {}", path.display());
}

// -- 6. deleted remotely while edited locally ------------------------------------------

#[tokio::test]
async fn a_local_edit_survives_a_remote_deletion() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let (folder, note) = shared_note(&mut a, &mut b, "Draft\nv1").await;

    {
        Device::tick();
        let db = a.db();
        db.trash(&note).unwrap();
        db.purge(&note).unwrap();
    }
    a.sync().await.unwrap();
    assert!(!server.has(&md(&note)));

    b.edit(&note, "Draft\nv2 written on B");
    let rb = b.sync().await.unwrap();
    assert_eq!(rb.conflicts, 0);
    assert_eq!(rb.uploaded, 1, "the edited note is uploaded again");
    assert_eq!(b.text(&note).as_deref(), Some("Draft\nv2 written on B"));
    assert!(!b.db().raw(&note).unwrap().unwrap().2, "clean after the sync");
    assert!(server.item_text(&md(&note)).unwrap().contains("v2 written on B"));

    // A gets it back.
    a.sync().await.unwrap();
    assert_eq!(a.text(&note).as_deref(), Some("Draft\nv2 written on B"));
    assert_eq!(a.db().notes_in(&[folder]).unwrap().len(), 1);

    // And it stays on both devices.
    let rb = b.sync().await.unwrap();
    assert_eq!(rb.deleted_local, 0);
    assert!(b.text(&note).is_some());
}

// -- 7. / 8. E2EE ------------------------------------------------------------------------

#[tokio::test]
async fn e2ee_uploads_are_unreadable_on_the_server_and_readable_with_the_key() {
    let server = FakeServer::start(&info_e2ee());
    let mut a = Device::with_key(&server);
    let mut b = Device::with_key(&server);

    let folder = a.create_folder("Private matters");
    let note = a.create_note(&folder, "Secret plan\nthe launch code is 4815");
    let ra = a.sync().await.unwrap();
    assert!(!ra.needs_password);
    assert_eq!(ra.uploaded, 2);

    for (id, words) in [(&folder, ["Private matters"].as_slice()), (&note, ["Secret plan", "4815"].as_slice())] {
        let stored = server.item_text(&md(id)).unwrap();
        assert!(stored.contains("encryption_applied: 1"), "{stored}");
        assert!(stored.contains("encryption_cipher_text: JED01"));
        for w in words {
            assert!(!stored.contains(w), "{w:?} readable on the server");
        }
    }

    let rb = b.sync().await.unwrap();
    assert_eq!((rb.downloaded, rb.still_encrypted), (2, 0));
    assert!(!rb.needs_password);
    let n = b.db().note(&note).unwrap().unwrap();
    assert_eq!(n.text, "Secret plan\nthe launch code is 4815");
    assert_eq!(n.parent_id, folder);
    assert!(!n.encrypted);
    assert_eq!(b.db().folders().unwrap()[0].title, "Private matters");
}

#[tokio::test]
async fn e2ee_without_the_key_downloads_but_uploads_nothing_until_unlocked() {
    let server = FakeServer::start(&info_e2ee());
    let mut a = Device::with_key(&server);
    let mut b = Device::new(&server);

    let folder = a.create_folder("Shared");
    let notes: Vec<String> = (0..4).map(|i| a.create_note(&folder, &format!("Note {i}\nsecret {i}"))).collect();
    a.sync().await.unwrap();

    let local_folder = b.create_folder("Local");
    let local_note = b.create_note(&local_folder, "Written on B\nbefore unlocking");

    server.clear_requests();
    let rb = b.sync().await.unwrap();
    assert!(rb.needs_password);
    assert_eq!(rb.downloaded, 5);
    assert_eq!(rb.still_encrypted, 5);
    assert_eq!(rb.uploaded, 0);
    assert!(server.writes().is_empty(), "B wrote {:?}", server.writes());
    assert!(!server.has(&md(&local_note)));
    for id in &notes {
        let n = b.db().note(id).unwrap().unwrap();
        assert!(n.encrypted);
        assert!(!n.text.contains("secret"));
    }
    assert_eq!(b.db().dirty_items().unwrap().len(), 2, "B's own items wait for the key");

    b.give_key();
    assert_eq!(b.decrypt_pending().unwrap(), 0);
    for (i, id) in notes.iter().enumerate() {
        let n = b.db().note(id).unwrap().unwrap();
        assert!(!n.encrypted);
        assert_eq!(n.text, format!("Note {i}\nsecret {i}"));
        assert_eq!(n.parent_id, folder);
    }

    let rb = b.sync().await.unwrap();
    assert!(!rb.needs_password);
    assert_eq!((rb.uploaded, rb.still_encrypted), (2, 0));
    let stored = server.item_text(&md(&local_note)).unwrap();
    assert!(stored.contains("encryption_applied: 1") && !stored.contains("Written on B"));

    a.sync().await.unwrap();
    assert_eq!(a.text(&local_note).as_deref(), Some("Written on B\nbefore unlocking"));
}

/// Without the key the next sync (not only `decrypt_pending`) unlocks what came down.
#[tokio::test]
async fn e2ee_items_downloaded_without_the_key_are_decrypted_by_the_next_sync() {
    let server = FakeServer::start(&info_e2ee());
    let mut a = Device::with_key(&server);
    let mut b = Device::new(&server);
    let folder = a.create_folder("Shared");
    let note = a.create_note(&folder, "Hello\nworld");
    a.sync().await.unwrap();

    assert_eq!(b.sync().await.unwrap().still_encrypted, 2);
    b.give_key();
    let rb = b.sync().await.unwrap();
    assert_eq!((rb.still_encrypted, rb.uploaded), (0, 0));
    assert_eq!(b.text(&note).as_deref(), Some("Hello\nworld"));
}

// -- 9. attachments -----------------------------------------------------------------------

async fn attachment_round_trip(e2ee: bool) {
    let server = FakeServer::start(&if e2ee { info_e2ee() } else { info_plain() });
    let (mut a, mut b) = if e2ee {
        (Device::with_key(&server), Device::with_key(&server))
    } else {
        (Device::new(&server), Device::new(&server))
    };
    let bytes = png(7, 5000);
    let folder = a.create_folder("Photos");
    let res = a.db().add_resource(&bytes, "image/png", "pic.png").unwrap();
    let note = a.create_note(&folder, &format!("Holiday\n![pic.png](:/{res})"));
    let ra = a.sync().await.unwrap();
    assert_eq!(ra.uploaded, 3);

    let stored = server.item(&blob(&res)).expect("blob on the server");
    let item_text = server.item_text(&md(&res)).expect("resource item on the server");
    if e2ee {
        assert_ne!(stored, bytes);
        assert!(stored.starts_with(b"JED01"), "blob is FileV1-encrypted");
        assert!(item_text.contains("encryption_applied: 1") && !item_text.contains("pic.png"));
    } else {
        assert_eq!(stored, bytes);
        assert!(item_text.contains("encryption_blob_encrypted: 0"));
    }

    let rb = b.sync().await.unwrap();
    assert_eq!((rb.downloaded, rb.still_encrypted), (3, 0));
    assert!(b.text(&note).unwrap().contains(&format!("(:/{res})")));
    assert!(b.db().resource_file(&res).unwrap().is_none(), "files are fetched on demand");
    let (it, encrypted, ..) = b.db().raw(&res).unwrap().unwrap();
    assert!(!encrypted);
    assert_eq!(it.get("encryption_blob_encrypted"), if e2ee { "1" } else { "0" });
    let path = b.fetch_resource(&res).await.unwrap().expect("file fetched");
    assert_eq!(read(&path), bytes);
    assert!(path.starts_with(b.resources_dir()));

    // An edited attachment (a redrawn sketch) travels again; B drops its stale copy.
    let edited = png(9, 3000);
    Device::tick();
    a.db().update_resource(&res, &edited).unwrap();
    let ra = a.sync().await.unwrap();
    assert_eq!((ra.uploaded, ra.conflicts), (1, 0));
    let stored = server.item(&blob(&res)).unwrap();
    if e2ee {
        assert!(stored.starts_with(b"JED01"));
    } else {
        assert_eq!(stored, edited);
    }

    let rb = b.sync().await.unwrap();
    assert_eq!((rb.downloaded, rb.conflicts), (1, 0));
    assert!(b.db().resource_file(&res).unwrap().is_none(), "stale file dropped");
    let path = b.fetch_resource(&res).await.unwrap().expect("new file fetched");
    assert_eq!(read(&path), edited);
    assert_eq!(b.db().raw(&res).unwrap().unwrap().0.get("size"), "3008");
}

#[tokio::test]
async fn attachments_round_trip_and_edited_files_travel_again() {
    attachment_round_trip(false).await;
}

#[tokio::test]
async fn attachments_round_trip_and_edited_files_travel_again_under_e2ee() {
    attachment_round_trip(true).await;
}

/// Both devices redraw the same sketch: the remote file wins, but B's drawing must
/// not vanish (Joplin keeps the local file in a conflict note).
#[tokio::test]
#[ignore = "an attachment edited on two devices loses the local file with no copy (sync.rs conflict branch only copies notes)"]
async fn attachment_conflict_keeps_the_local_file() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    a.create_folder("Sketches");
    let res = a.db().add_resource(&png(1, 100), "image/png", "sketch.png").unwrap();
    a.sync().await.unwrap();
    b.sync().await.unwrap();
    b.fetch_resource(&res).await.unwrap().unwrap();

    let from_a = png(2, 100);
    let from_b = png(3, 100);
    Device::tick();
    a.db().update_resource(&res, &from_a).unwrap();
    Device::tick();
    b.db().update_resource(&res, &from_b).unwrap();
    a.sync().await.unwrap();
    let rb = b.sync().await.unwrap();
    assert_eq!(rb.conflicts, 1);
    assert_eq!(read(&b.fetch_resource(&res).await.unwrap().unwrap()), from_a, "remote wins");

    let survives = std::fs::read_dir(b.resources_dir())
        .unwrap()
        .filter_map(|e| std::fs::read(e.unwrap().path()).ok())
        .any(|bytes| bytes == from_b);
    assert!(survives, "B's drawing is gone");
}

// -- 10. re-upload --------------------------------------------------------------------------

async fn reupload_restores_a_wiped_server(e2ee: bool) {
    let server = FakeServer::start(&if e2ee { info_e2ee() } else { info_plain() });
    let mut a = if e2ee { Device::with_key(&server) } else { Device::new(&server) };
    let bytes = png(4, 2000);
    let folder = a.create_folder("Everything");
    let res = a.db().add_resource(&bytes, "image/png", "photo.png").unwrap();
    let mut notes = vec![a.create_note(&folder, &format!("With a photo\n![photo.png](:/{res})"))];
    for i in 0..4 {
        notes.push(a.create_note(&folder, &format!("Note {i}")));
    }
    a.sync().await.unwrap();
    let total = 2 + notes.len();
    assert_eq!(server.names().len(), total + 1, "items plus the blob");

    server.wipe();
    assert!(server.names().is_empty());
    // A plain sync does not notice anything is missing (as in Joplin).
    let r = a.sync().await.unwrap();
    assert_eq!(r.uploaded, 0);

    let r = a.reupload_all().await.unwrap();
    assert_eq!(r.uploaded, total);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(server.names().len(), total + 1);
    assert!(server.has(&blob(&res)));
    assert!(a.db().dirty_items().unwrap().is_empty());

    // Doing it again finds everything in place.
    server.clear_requests();
    let r = a.reupload_all().await.unwrap();
    assert_eq!((r.uploaded, r.conflicts), (0, 0));
    assert!(server.writes().is_empty(), "{:?}", server.writes());

    let mut b = if e2ee { Device::with_key(&server) } else { Device::new(&server) };
    let rb = b.sync().await.unwrap();
    assert_eq!((rb.downloaded, rb.still_encrypted), (total, 0));
    for id in &notes {
        assert_eq!(b.text(id), a.text(id));
    }
    assert_eq!(read(&b.fetch_resource(&res).await.unwrap().unwrap()), bytes);
}

#[tokio::test]
async fn reupload_restores_a_wiped_server_for_a_fresh_device() {
    reupload_restores_a_wiped_server(false).await;
}

#[tokio::test]
async fn reupload_restores_a_wiped_server_under_e2ee() {
    reupload_restores_a_wiped_server(true).await;
}

/// The note was edited here AFTER it changed elsewhere: the download skips the
/// older remote version, so only the kept `sync_time` reveals the conflict at upload.
#[tokio::test]
async fn reupload_keeps_a_local_edit_newer_than_a_remote_change_as_a_conflict() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let (_, note) = shared_note(&mut a, &mut b, "Shared\nv1").await;

    server.wipe();
    b.reupload_all().await.unwrap();
    b.edit(&note, "Shared\nv2 from B");
    b.sync().await.unwrap();

    a.edit(&note, "Shared\nv2 from A");
    let ra = a.reupload_all().await.unwrap();
    assert_eq!(ra.conflicts, 1);
    assert_eq!(a.text(&note).as_deref(), Some("Shared\nv2 from B"), "not overwritten");
    let copies = a.conflicts();
    assert_eq!(copies.len(), 1);
    assert_eq!(a.text(copies[0].id()).as_deref(), Some("Shared\nv2 from A"));
    assert!(server.item_text(&md(&note)).unwrap().contains("v2 from B"));

    b.sync().await.unwrap();
    assert_eq!(b.conflicts().len(), 1);
}

/// The note was edited here BEFORE it changed elsewhere: the download, which runs
/// first in a re-upload, finds the dirty note and a newer remote one.
#[tokio::test]
async fn reupload_keeps_a_local_edit_older_than_a_remote_change_as_a_conflict() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let (_, note) = shared_note(&mut a, &mut b, "Shared\nv1").await;

    a.edit(&note, "Shared\nv2 from A");
    b.edit(&note, "Shared\nv2 from B");
    b.sync().await.unwrap();

    let ra = a.reupload_all().await.unwrap();
    assert_eq!(ra.conflicts, 1);
    assert_eq!(a.text(&note).as_deref(), Some("Shared\nv2 from B"));
    let copies = a.conflicts();
    assert_eq!(copies.len(), 1, "exactly one copy");
    assert_eq!(a.text(copies[0].id()).as_deref(), Some("Shared\nv2 from A"));
    assert!(server.has(&md(copies[0].id())), "the copy is uploaded by the same re-upload");
    assert!(a.db().dirty_items().unwrap().is_empty());

    b.sync().await.unwrap();
    assert_eq!(b.conflicts().len(), 1);
    assert_eq!(b.text(&note).as_deref(), Some("Shared\nv2 from B"));
}

// -- 11. unreadable items ----------------------------------------------------------------------

/// A valid note written by "another client", for injecting into the server.
fn foreign_note(id: &str, folder: &str, title: &str) -> String {
    let mut it = RawItem::new_with_fields(TYPE_NOTE, item::NOTE_FIELDS);
    it.set("id", id);
    it.set("parent_id", folder);
    let now = item::now_ms();
    for k in ["created_time", "updated_time", "user_created_time", "user_updated_time"] {
        it.set_time(k, now);
    }
    it.title = Some(title.to_string());
    it.body = Some("from another client".into());
    it.serialize()
}

#[tokio::test]
async fn an_unreadable_item_is_reported_and_the_rest_still_arrives() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let folder = a.create_folder("Work");
    let mut notes: Vec<String> = (0..2).map(|i| a.create_note(&folder, &format!("Before {i}"))).collect();
    a.sync().await.unwrap();
    let bad = item::new_id();
    server.put_item(&md(&bad), b"this is not a Joplin item\nat all");
    notes.extend((0..2).map(|i| a.create_note(&folder, &format!("After {i}"))));
    a.sync().await.unwrap();

    let rb = b.sync().await.unwrap();
    assert_eq!(rb.errors.len(), 1, "{:?}", rb.errors);
    assert!(rb.errors[0].contains(&bad));
    assert_eq!(rb.downloaded, 5);
    for id in &notes {
        assert!(b.text(id).is_some());
    }
    assert!(b.db().raw(&bad).unwrap().is_none());

    // The delta cursor moved past it: it is not looked at again (documented in
    // AGENTS.md; "resync all" = reset_delta brings it back).
    let rb = b.sync().await.unwrap();
    assert!(rb.errors.is_empty());
    b.db().reset_delta().unwrap();
    let rb = b.sync().await.unwrap();
    assert_eq!(rb.errors.len(), 1);
    assert_eq!(rb.downloaded, 0);
}

/// An item this client cannot read today (a format it does not parse yet) should be
/// tried again later instead of being skipped for good once the cursor moves on.
#[tokio::test]
#[ignore = "an unreadable item is skipped while the delta cursor advances, so it is never retried (sync.rs download)"]
async fn an_unreadable_item_is_retried_at_the_next_sync() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let folder = a.create_folder("Work");
    a.sync().await.unwrap();
    let id = item::new_id();
    server.put_item(&md(&id), b"not readable by this version");

    let rb = b.sync().await.unwrap();
    assert_eq!(rb.errors.len(), 1);

    // Same content from the client's point of view once it can read it.
    server.replace_silently(&md(&id), foreign_note(&id, &folder, "Readable now").as_bytes());
    let rb = b.sync().await.unwrap();
    assert!(rb.errors.is_empty());
    assert_eq!(b.text(&id).as_deref(), Some("Readable now\nfrom another client"));
}

// -- 12. locks -------------------------------------------------------------------------------------

#[tokio::test]
async fn no_lock_is_left_behind_after_a_good_or_a_failed_sync() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let folder = a.create_folder("Work");
    let notes: Vec<String> = (0..5).map(|i| a.create_note(&folder, &format!("Note {i}"))).collect();

    a.sync().await.unwrap();
    assert!(server.locks().is_empty());
    assert!(server.requests().iter().any(|r| r.method == "POST" && r.path == "/api/locks"), "a sync lock was taken");

    // Network failure while uploading the third note.
    for n in &notes {
        a.edit(n, &format!("{n} edited"));
    }
    server.fail_once("PUT", &format!("{}.md:/content", notes[2]));
    let err = a.sync().await.expect_err("the upload fails midway");
    assert!(matches!(err, omanote_core::Error::Network(_)), "{err:?}");
    assert!(server.locks().is_empty(), "lock left behind: {:?}", server.locks());
    assert_eq!(a.db().dirty_items().unwrap().len(), 3, "the first two went up");

    // The next sync finishes the job.
    let r = a.sync().await.unwrap();
    assert_eq!(r.uploaded, 3);
    assert!(server.locks().is_empty());
    for n in &notes {
        assert!(server.item_text(&md(n)).unwrap().contains("edited"));
    }
}

#[tokio::test]
async fn an_exclusive_lock_held_by_another_client_stops_the_sync_before_any_write() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let folder = a.create_folder("Work");
    a.create_note(&folder, "Waiting");
    server.add_exclusive_lock();

    server.clear_requests();
    let err = a.sync().await.expect_err("locked");
    assert!(matches!(&err, omanote_core::Error::Server { status: 409, .. }), "{err:?}");
    assert!(server.writes().is_empty());
    assert_eq!(server.locks().len(), 1, "only the other client's lock");
    assert_eq!(a.db().dirty_items().unwrap().len(), 2);
}

// -- 13. network failure during a download -------------------------------------------------------

#[tokio::test]
async fn a_download_interrupted_by_the_network_completes_at_the_next_sync() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let folder = a.create_folder("Work");
    let notes: Vec<String> = (0..10).map(|i| a.create_note(&folder, &format!("Note {i}\nbody {i}"))).collect();
    a.sync().await.unwrap();

    server.fail_once("GET", &format!("{}.md:/content", notes[5]));
    let err = b.sync().await.expect_err("the download fails midway");
    assert!(matches!(err, omanote_core::Error::Network(_)), "{err:?}");
    assert!(server.locks().is_empty());
    let got_first = {
        let db = b.db();
        db.notes_in(&[folder.clone()]).unwrap().len() + db.folders().unwrap().len()
    };
    assert!(got_first < 11, "the failure happened before the end ({got_first})");

    let rb = b.sync().await.unwrap();
    assert!(rb.errors.is_empty());
    assert_eq!(rb.downloaded + got_first, 11, "every missing item, once (folder included)");
    let listed = b.db().notes_in(&[folder.clone()]).unwrap();
    assert_eq!(listed.len(), 10);
    for (i, id) in notes.iter().enumerate() {
        assert_eq!(b.text(id), Some(format!("Note {i}\nbody {i}")));
    }
    assert!(b.conflicts().is_empty());
    assert!(b.db().dirty_items().unwrap().is_empty());

    let rb = b.sync().await.unwrap();
    assert_eq!((rb.downloaded, rb.uploaded), (0, 0));
}

// -- further edge cases ------------------------------------------------------------------------------

/// The conflict copy is created locally, then its upload fails: the next sync must
/// not turn the same local edit into a second copy.
#[tokio::test]
#[ignore = "a network failure right after a conflict copy is made yields a second conflict copy at the next sync (sync.rs upload conflict branch)"]
async fn a_failed_upload_after_a_conflict_does_not_duplicate_the_copy() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let (_, note) = shared_note(&mut a, &mut b, "Plan\nv1").await;
    a.edit(&note, "Plan\nfrom A");
    b.edit(&note, "Plan\nfrom B");
    a.sync().await.unwrap();

    // B's first write is the conflict copy.
    server.fail_once("PUT", ".md:/content");
    b.sync().await.expect_err("network failure");
    let rb = b.sync().await.unwrap();
    assert_eq!(b.text(&note).as_deref(), Some("Plan\nfrom A"));
    assert_eq!(rb.errors, Vec::<String>::new());
    let copies = b.conflicts();
    assert_eq!(copies.len(), 1, "one local edit, one conflict copy");
}

/// Another client whose clock is behind uploads an edit whose `updated_time` is
/// older than the version B already has: B never takes it, although it is the
/// server's current content.
#[tokio::test]
#[ignore = "a remote edit with an older updated_time (clock behind) is never applied locally (sync.rs download compares with the local updated_time)"]
async fn a_remote_edit_from_a_device_with_a_slow_clock_is_not_lost() {
    let server = FakeServer::start(&info_plain());
    let mut a = Device::new(&server);
    let mut b = Device::new(&server);
    let (_, note) = shared_note(&mut a, &mut b, "Shared\nv1").await;

    // The other client edits the note a minute "in the past".
    let mut it = RawItem::parse(&server.item_text(&md(&note)).unwrap()).unwrap();
    let past = it.time("updated_time") - 60_000;
    it.set_time("updated_time", past);
    it.set_time("user_updated_time", past);
    it.body = Some("v2 from a device whose clock is late".into());
    server.put_item(&md(&note), it.serialize().as_bytes());

    let rb = b.sync().await.unwrap();
    assert_eq!(rb.downloaded, 1);
    assert_eq!(b.text(&note).as_deref(), Some("Shared\nv2 from a device whose clock is late"));
}
