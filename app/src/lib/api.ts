// Typed bridge to the Rust core (Tauri commands).
import { convertFileSrc, invoke as tauriInvoke, type InvokeArgs, type InvokeOptions } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

/** True in the packaged app; false when the UI is opened in a plain browser. */
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const invoke = <T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> =>
  inTauri
    ? tauriInvoke<T>(cmd, args, options)
    : import("./mock").then((m) => m.mockInvoke(cmd, args as Record<string, unknown>) as Promise<T>);

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
  platform: string;
  omarchy: boolean;
  /** No Joplin Server connected: notes stay on this device. */
  local: boolean;
  /** Showing every Joplin notebook (tree root ""), new notes still go to root_folder_id. */
  whole_joplin: boolean;
  data_dir: string;
}

export interface TimerTick {
  kind: "stopwatch" | "countdown" | "pomodoro";
  title: string;
  phase: "work" | "rest";
  running: boolean;
  ms: number;
  label: string;
}

export interface Folder {
  id: string;
  parent_id: string;
  title: string;
  icon: string;
  note_count: number;
}

export interface TrashItem {
  id: string;
  parent_id: string;
  title: string;
  is_folder: boolean;
  deleted_time: number;
  encrypted: boolean;
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
  useLocal: (notebook: string) => invoke<string>("use_local", { notebook }),
  unlock: (master_password: string) => invoke<void>("unlock", { masterPassword: master_password }),
  logout: () => invoke<void>("logout"),
  syncNow: () => invoke<SyncReport | null>("sync_now"),
  resyncAll: () => invoke<SyncReport | null>("resync_all"),
  reuploadLocal: () => invoke<SyncReport | null>("reupload_local"),
  setRootFolder: (folderId?: string, newTitle?: string) =>
    invoke<string>("set_root_folder", { folderId: folderId ?? null, newTitle: newTitle ?? null }),
  setWholeJoplin: (enabled: boolean) => invoke<void>("set_whole_joplin", { enabled }),
  setNotesHome: (folderId: string) => invoke<void>("set_notes_home", { folderId }),
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
  trash: () => invoke<TrashItem[]>("list_trash"),
  cliPath: () => invoke<string>("cli_path"),
  restore: (id: string) => invoke<void>("restore_item", { id }),
  purge: (id: string) => invoke<void>("purge_item", { id }),
  emptyTrash: () => invoke<number>("empty_trash"),
  promoteNote: (id: string) => invoke<void>("promote_note", { id }),
  setLanguage: (lang: string) => invoke<void>("set_language", { lang }),
  // timer
  timerCommand: (line: string) => invoke<boolean>("timer_command", { line }),
  timerToggle: () => invoke<void>("timer_toggle"),
  timerStop: () => invoke<void>("timer_stop"),
  timerState: () => invoke<TimerTick | null>("timer_state"),
  // OCR: the image travels as the raw request body (no JSON encoding).
  ocrImage: async (image: Blob) =>
    invoke<string>("ocr_image", new Uint8Array(await image.arrayBuffer())),
  // images as Joplin attachments (referenced in notes as ![name](:/id))
  addImage: async (image: Blob, name: string) =>
    invoke<string>("add_image", new Uint8Array(await image.arrayBuffer()), {
      headers: { "x-name": name.replace(/[^\x20-\x7e]/g, "_"), "x-mime": image.type || "image/png" },
    }),
  /** URL the webview can load for an attachment (downloaded on demand). */
  resourceSrc: async (id: string) => {
    const path = await invoke<string | null>("resource_path", { id });
    return path && inTauri ? convertFileSrc(path) : path;
  },
  captureText: () => invoke<string | null>("capture_text"),
  // window
  togglePin: () => invoke<boolean>("toggle_pin"),
  hideWindow: () => invoke<void>("hide_window"),
  /** Note asked for with `omanote --open <id>` when the app was launched. */
  takeLaunchNote: () => invoke<string | null>("take_launch_note"),
  setWindowControls: (visible: boolean) => invoke<void>("set_window_controls", { visible }),
};

export const onSyncStatus = (cb: (e: SyncEvent) => void) =>
  listen<SyncEvent>("sync-status", (e) => cb(e.payload));
export const onDataChanged = (cb: () => void) => listen("data-changed", () => cb());
export const onQuickNote = (cb: () => void) => listen("quick-note", () => cb());
export const onCaptureText = (cb: () => void) => listen("capture-text", () => cb());
/** `omanote --open <id>` while the app runs (Omarchy bar plugin). */
export const onOpenNote = (cb: (id: string) => void) => listen<string>("open-note", (e) => cb(e.payload));
export const onTimer = (cb: (t: TimerTick | null) => void) => listen<TimerTick | null>("timer", (e) => cb(e.payload));
export const onTimerFinished = (cb: (event: string) => void) => listen<string>("timer-finished", (e) => cb(e.payload));


/** Human-friendly relative time in the UI language. */
export function when(ms: number, lang = "en"): string {
  const diff = Date.now() - ms;
  const min = Math.round(diff / 60000);
  const rtf = new Intl.RelativeTimeFormat(lang, { numeric: "auto", style: "short" });
  if (min < 1) return rtf.format(0, "second");
  if (min < 60) return rtf.format(-min, "minute");
  const h = Math.round(min / 60);
  if (h < 24) return rtf.format(-h, "hour");
  const d = new Date(ms);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString(lang, { day: "numeric", month: "short", year: sameYear ? undefined : "2-digit" });
}
