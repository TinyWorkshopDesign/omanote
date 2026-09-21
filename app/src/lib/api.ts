// Typed bridge to the Rust core (Tauri commands).
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

/** True in the packaged app; false when the UI is opened in a plain browser. */
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const invoke = <T>(cmd: string, args?: Record<string, unknown>): Promise<T> =>
  inTauri
    ? tauriInvoke<T>(cmd, args)
    : import("./mock").then((m) => m.mockInvoke(cmd, args) as Promise<T>);

const listen = <T>(event: string, cb: (e: { payload: T }) => void): Promise<UnlistenFn> =>
  inTauri ? tauriListen<T>(event, cb) : Promise.resolve(() => {});

export interface Status {
  configured: boolean;
  server_url: string;
  email: string;
  root_folder_id: string;
  e2ee: boolean;
  locked: boolean;
  hotkey: string;
  mobile: boolean;
}

export interface Folder {
  id: string;
  parent_id: string;
  title: string;
  icon: string;
  note_count: number;
}

export interface NoteSummary {
  id: string;
  parent_id: string;
  title: string;
  preview: string;
  updated_time: number;
  is_conflict: boolean;
  encrypted: boolean;
}

export interface Note {
  id: string;
  parent_id: string;
  text: string;
  updated_time: number;
  created_time: number;
  encrypted: boolean;
}

export interface SyncReport {
  uploaded: number;
  downloaded: number;
  deleted_local: number;
  deleted_remote: number;
  conflicts: number;
  still_encrypted: number;
  needs_password: boolean;
  errors: string[];
}

export interface SyncEvent {
  state: "syncing" | "idle" | "error";
  message: string;
  report: SyncReport | null;
}

export const api = {
  status: () => invoke<Status>("get_status"),
  setup: (server_url: string, email: string, password: string, master_password?: string) =>
    invoke<void>("setup", { serverUrl: server_url, email, password, masterPassword: master_password ?? null }),
  unlock: (master_password: string) => invoke<void>("unlock", { masterPassword: master_password }),
  logout: () => invoke<void>("logout"),
  syncNow: () => invoke<SyncReport | null>("sync_now"),
  setRootFolder: (folderId?: string, newTitle?: string) =>
    invoke<string>("set_root_folder", { folderId: folderId ?? null, newTitle: newTitle ?? null }),
  folders: () => invoke<Folder[]>("list_folders"),
  notes: (folderId?: string) => invoke<NoteSummary[]>("list_notes", { folderId: folderId ?? null }),
  search: (query: string) => invoke<NoteSummary[]>("search_notes", { query }),
  note: (id: string) => invoke<Note | null>("get_note", { id }),
  createNote: (folderId: string | undefined, text: string) =>
    invoke<Note>("create_note", { folderId: folderId ?? null, text }),
  updateNote: (id: string, text: string) => invoke<Note>("update_note", { id, text }),
  moveNote: (id: string, folderId: string) => invoke<void>("move_note", { id, folderId }),
  trashNote: (id: string) => invoke<void>("trash_note", { id }),
  createFolder: (title: string, parentId?: string) =>
    invoke<Folder>("create_folder", { title, parentId: parentId ?? null }),
  renameFolder: (id: string, title: string) => invoke<void>("rename_folder", { id, title }),
  moveFolder: (id: string, parentId: string) => invoke<void>("move_folder", { id, parentId }),
  trashFolder: (id: string) => invoke<void>("trash_folder", { id }),
};

export const onSyncStatus = (cb: (e: SyncEvent) => void) =>
  listen<SyncEvent>("sync-status", (e) => cb(e.payload));
export const onDataChanged = (cb: () => void) => listen("data-changed", () => cb());
export const onQuickNote = (cb: () => void) => listen("quick-note", () => cb());

/** Human-friendly relative time, in Italian. */
export function when(ms: number): string {
  const diff = Date.now() - ms;
  const min = Math.round(diff / 60000);
  if (min < 1) return "adesso";
  if (min < 60) return `${min} min`;
  const h = Math.round(min / 60);
  if (h < 24) return `${h} h`;
  const d = new Date(ms);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: sameYear ? undefined : "2-digit" });
}
