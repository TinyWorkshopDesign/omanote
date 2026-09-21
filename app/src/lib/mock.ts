// In-memory stand-in for the Rust backend, used only when the UI runs in a
// plain browser (`npm run dev`) instead of inside Tauri. Never loaded in the
// packaged app: api.ts imports it dynamically after checking for Tauri.

import type { Folder, Note, NoteSummary, Status, SyncReport } from "./api";

const folders: Folder[] = [
  { id: "r".repeat(32), parent_id: "", title: "Omanote", icon: "", note_count: 0 },
  { id: "a".repeat(32), parent_id: "r".repeat(32), title: "Spesa", icon: "🛒", note_count: 1 },
  { id: "b".repeat(32), parent_id: "r".repeat(32), title: "Lavoro", icon: "💼", note_count: 1 },
  { id: "c".repeat(32), parent_id: "b".repeat(32), title: "Progetti", icon: "", note_count: 1 },
];

const notes: (Note & { title: string })[] = [
  {
    id: "1".repeat(32),
    parent_id: "a".repeat(32),
    title: "Spesa di sabato",
    text: "Spesa di sabato\n- [ ] latte 1,20 €\n- [x] pane 2,50 €\n- [ ] caffè 4 €\ntotale\n\nsconto = 10%\ntotale - sconto",
    updated_time: Date.now() - 600_000,
    created_time: Date.now() - 86_400_000,
    encrypted: false,
  },
  {
    id: "2".repeat(32),
    parent_id: "b".repeat(32),
    title: "Riunione lunedì",
    text: "Riunione lunedì\n# Punti\n- budget trimestre: 12500 €\n- team: 7 persone\n12500 / 7\n\n- [ ] preparare slide\n- [ ] mandare invito",
    updated_time: Date.now() - 3_600_000,
    created_time: Date.now() - 172_800_000,
    encrypted: false,
  },
];

notes.push(
  {
    id: "3".repeat(32),
    parent_id: "c".repeat(32),
    title: "Omanote",
    text: "Omanote\n- [x] albero delle cartelle\n- [ ] barra inferiore",
    updated_time: Date.now() - 7_200_000,
    created_time: Date.now() - 7_200_000,
    encrypted: false,
  },
  {
    id: "4".repeat(32),
    parent_id: "r".repeat(32),
    title: "Idea veloce",
    text: "Idea veloce\n2+2",
    updated_time: Date.now() - 60_000,
    created_time: Date.now() - 60_000,
    encrypted: false,
  },
);

const images = new Map<string, string>();

const id32 = () => Array.from({ length: 32 }, () => "0123456789abcdef"[Math.floor(Math.random() * 16)]).join("");
const summary = (n: (typeof notes)[number]): NoteSummary => ({
  id: n.id,
  parent_id: n.parent_id,
  title: n.text.split("\n")[0],
  preview: n.text.split("\n").slice(1).join(" "),
  updated_time: n.updated_time,
  is_conflict: false,
  encrypted: false,
});
const subtree = (root: string): string[] => [root, ...folders.filter((f) => f.parent_id === root).flatMap((f) => subtree(f.id))];

const status: Status = {
  configured: true,
  server_url: "http://localhost:22300",
  email: "demo@omanote",
  root_folder_id: "r".repeat(32),
  e2ee: true,
  locked: false,
  hotkey: "Alt+A",
  mobile: false,
  platform: "macos",
  omarchy: false,
  local: false,
  whole_joplin: false,
  data_dir: "~/Library/Application Support/app.omanote",
};

const empty: SyncReport = {
  uploaded: 0, downloaded: 0, deleted_local: 0, deleted_remote: 0,
  conflicts: 0, still_encrypted: 0, needs_password: false, errors: [],
};

function counts() {
  for (const f of folders) f.note_count = notes.filter((n) => n.parent_id === f.id).length;
}

export async function mockInvoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const a = args as Record<string, string | undefined>;
  counts();
  switch (cmd) {
    case "get_status":
      return { ...status }; // a fresh object, like the real backend
    case "list_folders":
      return folders;
    case "set_notes_home":
      status.root_folder_id = a.folderId ?? status.root_folder_id;
      return null;
    case "set_whole_joplin":
      status.whole_joplin = Boolean((args as { enabled?: boolean }).enabled);
      return null;
    case "list_notes": {
      const ids = a.folderId ? [a.folderId] : subtree(status.whole_joplin ? "" : status.root_folder_id);
      return notes.filter((n) => ids.includes(n.parent_id)).map(summary).sort((x, y) => y.updated_time - x.updated_time);
    }
    case "search_notes":
      return notes.filter((n) => n.text.toLowerCase().includes((a.query ?? "").toLowerCase())).map(summary);
    case "get_note":
      return notes.find((n) => n.id === a.id) ?? null;
    case "create_note": {
      const n = {
        id: id32(),
        parent_id: a.folderId || status.root_folder_id,
        title: (a.text ?? "").split("\n")[0],
        text: a.text ?? "",
        updated_time: Date.now(),
        created_time: Date.now(),
        encrypted: false,
      };
      notes.unshift(n);
      return n;
    }
    case "update_note": {
      const n = notes.find((x) => x.id === a.id)!;
      n.text = a.text ?? "";
      n.updated_time = Date.now();
      return n;
    }
    case "move_note": {
      const n = notes.find((x) => x.id === a.id)!;
      n.parent_id = a.folderId!;
      return null;
    }
    case "trash_note": {
      notes.splice(notes.findIndex((x) => x.id === a.id), 1);
      return null;
    }
    case "create_folder": {
      const f = { id: id32(), parent_id: a.parentId ?? status.root_folder_id, title: a.title ?? "", icon: "", note_count: 0 };
      folders.push(f);
      return f;
    }
    case "move_folder": {
      folders.find((f) => f.id === a.id)!.parent_id = a.parentId!;
      return null;
    }
    case "trash_folder": {
      folders.splice(folders.findIndex((f) => f.id === a.id), 1);
      return null;
    }
    case "rename_folder": {
      folders.find((f) => f.id === a.id)!.title = a.title ?? "";
      return null;
    }
    case "sync_now":
      return empty;
    case "promote_note": {
      const n = notes.find((x) => x.id === a.id);
      if (n) n.updated_time = Date.now();
      return null;
    }
    case "timer_command":
      return /^\s*timer\b/i.test(a.line ?? "");
    case "ocr_image":
    case "ocr_file":
      return "Testo riconosciuto (demo)";
    case "toggle_pin":
      return true;
    case "add_image": {
      const id = id32();
      images.set(id, URL.createObjectURL(new Blob([args as unknown as Uint8Array<ArrayBuffer>], { type: "image/png" })));
      return id;
    }
    case "add_image_file":
      return id32();
    case "resource_path":
      return images.get(a.id ?? "") ?? null;
    default:
      return null;
  }
}
