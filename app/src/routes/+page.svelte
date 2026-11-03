<script lang="ts">
  // The app shell: folders sidebar · note list · note, laid out by the window
  // width (3 columns, 2 columns with a sliding sidebar, or a navigation stack
  // on phones and narrow windows), plus the note editor and its dialogs.
  import { onMount } from "svelte";
  import type { EditorView } from "@codemirror/view";
  import {
    api,
    onCaptureText,
    onOpenNote,
    onDataChanged,
    onQuickNote,
    onSyncStatus,
    onTimer,
    onTimerFinished,
    type Folder,
    type Note,
    type NoteSummary,
    type Status,
    type TimerTick,
    type TrashItem,
  } from "$lib/api";
  import {
    createEditor,
    insertBlock,
    insertText,
    ratesChanged,
    refreshImage,
    setFxRates,
    SKETCH_EDIT,
    type SketchEdit,
  } from "$lib/editor";
  import { buildRates } from "$lib/calc";
  import { errText, i18n, t } from "$lib/i18n.svelte";
  import { initTheme, type ThemeState } from "$lib/theme";
  import { fitIconsToScreen, icons } from "$lib/icons";
  import { FONT_DEFAULT, FONT_MAX, FONT_MIN, prefs, savePrefs } from "$lib/prefs.svelte";
  import { fullDate } from "$lib/dates";
  import Setup from "$lib/components/Setup.svelte";
  import FolderSidebar from "$lib/components/FolderSidebar.svelte";
  import NoteList from "$lib/components/NoteList.svelte";
  import Palette from "$lib/components/Palette.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import Sketch from "$lib/components/Sketch.svelte";
  import { readSketchData, type SketchData } from "$lib/sketchData";

  type Level = "folders" | "list" | "note";

  let status = $state<Status | null>(null);
  let folders = $state<Folder[]>([]);
  let allNotes = $state<NoteSummary[]>([]); // the whole working tree, newest first
  let trashItems = $state<TrashItem[]>([]);
  /** "all", "trash" or a folder id. */
  let selection = $state(prefs.lastFolder || "all");
  let query = $state("");
  let searchAll = $state(true);
  let searchHits = $state<NoteSummary[]>([]);
  let current = $state<Note | null>(null);
  let draftFolder = $state<string | null>(null); // a new, not yet saved note
  /** A note of Recently Deleted, shown read-only. */
  let trashed = $state<Note | null>(null);
  let width = $state(typeof window === "undefined" ? 1200 : window.innerWidth);
  let level = $state<Level>("note"); // stack layout only
  let sidebarOverlay = $state(false); // 2-column layout
  let palette = $state<"search" | "move" | null>(null);
  let moveId = $state<string | null>(null); // note being moved (null = the open one)
  let settings = $state(false);
  let moreMenu = $state(false);
  let sketch = $state(false); // handwriting / drawing canvas
  /** The sketch being edited (its tag in the note) and its saved strokes. */
  let sketchEditing: SketchEdit | null = null;
  let sketchInitial = $state<SketchData | null>(null);
  let connecting = $state(false); // local mode → Joplin Server login
  let syncState = $state<"idle" | "syncing" | "error">("idle");
  let syncMsg = $state("");
  let lastSync = $state(0);
  let toast = $state("");
  let unlockPw = $state("");
  let theme = $state<ThemeState>({ current: "tokyo-night", fromSystem: false });
  let timer = $state<TimerTick | null>(null);
  let timerDone = $state(false);
  let pinned = $state(false);
  let nearTop = $state(false);
  const touch = typeof matchMedia !== "undefined" && matchMedia("(hover: none)").matches;

  let host: HTMLElement | undefined = $state();
  let view: EditorView | undefined;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let syncTimer: ReturnType<typeof setTimeout> | undefined;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let pendingText: string | null = null;
  /** Pending "image or text?" question for a pasted/dropped image. */
  let imageAsk = $state<((choice: "image" | "ocr" | null) => void) | null>(null);
  let currentText = "";

  // ------------------------------------------------------------------ layout

  const layout = $derived(width >= 1000 ? "three" : width >= 700 ? "two" : "stack");
  const showSidebar = $derived(layout === "three" ? !prefs.focus && !prefs.sidebarHidden : layout === "stack" && level === "folders");
  const showList = $derived(layout === "stack" ? level === "list" : !prefs.focus);
  const showEditor = $derived(layout !== "stack" || level === "note");
  const mac = $derived(status?.platform === "macos");
  // The PencilKit canvas is for the iPad and its Apple Pencil; an iPhone (also
  // a foldable one, open) draws with the finger on the web canvas. The app asks
  // UIKit which device it runs on: screen sizes overlap.
  const ipad = $derived(status?.platform === "ios" && !!status.tablet);
  /** Leftmost column, which leaves room for the macOS traffic lights. */
  const leftmost = $derived(showSidebar ? "sidebar" : showList ? "list" : "editor");
  const overlayOpen = $derived(sidebarOverlay || !!palette || settings || moreMenu);
  const barVisible = $derived(
    !prefs.autoHideToolbar || touch || nearTop || overlayOpen || syncState === "error" || layout === "stack",
  );

  // macOS: the traffic lights stay with the sidebar or list; over the bare note
  // they follow the toolbar.
  $effect(() => {
    if (mac) void api.setWindowControls(leftmost !== "editor" || barVisible);
  });

  $effect(() => savePrefs(prefs));

  const treeRoot = $derived(status?.whole_joplin ? "" : (status?.root_folder_id ?? ""));
  const isFolder = $derived(selection !== "all" && selection !== "trash");
  const folderTitle = (id: string) => folders.find((f) => f.id === id)?.title ?? "Omanote";
  const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
  const listTitle = $derived(
    selection === "all" ? t("nav.allNotes") : selection === "trash" ? t("nav.recentlyDeleted") : folderTitle(selection),
  );
  const listNotes = $derived.by(() => {
    if (query.trim()) return !searchAll && isFolder ? searchHits.filter((n) => n.parent_id === selection) : searchHits;
    if (selection === "all") return allNotes;
    if (selection === "trash") return [];
    return allNotes.filter((n) => n.parent_id === selection);
  });
  const trashNotes = $derived(trashItems.filter((x) => !x.is_folder).length);
  const openFolder = $derived(current?.parent_id ?? draftFolder ?? "");

  // Search: the whole tree, narrowed to the folder by `searchAll`.
  $effect(() => {
    const q = query.trim();
    if (!q) {
      searchHits = [];
      return;
    }
    let alive = true;
    const timerId = setTimeout(() => api.search(q).then((r) => alive && (searchHits = r)), 120);
    return () => {
      alive = false;
      clearTimeout(timerId);
    };
  });

  onMount(() => {
    fitIconsToScreen();
    const offs: (() => void)[] = [];
    const onResize = () => (width = window.innerWidth);
    window.addEventListener("resize", onResize);
    offs.push(() => window.removeEventListener("resize", onResize));
    host?.addEventListener(SKETCH_EDIT, editSketch);
    offs.push(() => host?.removeEventListener(SKETCH_EDIT, editSketch));
    void (async () => {
      offs.push(await initTheme((s) => (theme = s)));
      document.documentElement.lang = i18n.lang;
      void api.setLanguage(i18n.lang);
      await refreshStatus();
      // Exchange rates for `100 € in $ =`: loaded in the background, cached on
      // disk by the app (one request a day). Offline, the last table is used.
      void (async () => {
        try {
          const table = await api.fxRates();
          if (!Object.keys(table.rates ?? {}).length) return;
          setFxRates(buildRates(table.rates, table.base || "EUR"));
          view?.dispatch({ effects: ratesChanged.of(null) });
        } catch {
          /* no rates: conversions stay unavailable until a rate line is added */
        }
      })();
      await refresh();
      if (selection !== "all" && selection !== "trash" && !folders.some((f) => f.id === selection)) selection = "all";
      if (selection === "trash") selection = "all";
      timer = await api.timerState();
      await startup();

      offs.push(
        await onSyncStatus((e) => {
          syncState = e.state;
          syncMsg = e.message;
          if (e.state === "idle" && e.report) lastSync = Date.now();
          if (e.report?.needs_password) void refreshStatus();
        }),
        await onDataChanged(async () => {
          await refresh();
          if (current && !pendingText) {
            const fresh = await api.note(current.id);
            if (!fresh) newNote(undefined, false);
            else if (fresh.updated_time !== current.updated_time && fresh.text !== currentText) await open(current.id, false, false);
          }
        }),
        await onQuickNote(() => newNote()),
        await onCaptureText(() => void captureText()),
        await onOpenNote(async (id) => {
          if (await api.note(id)) await open(id);
        }),
        await onTimer((tk) => {
          timer = tk;
          if (!tk) timerDone = false;
        }),
        await onTimerFinished(() => {
          timerDone = true;
          setTimeout(() => (timerDone = false), 8000);
        }),
      );
    })();
    return () => offs.forEach((off) => off());
  });

  /** What the app shows when it opens: the launch note, the last note or a new one. */
  async function startup() {
    const launch = await api.takeLaunchNote();
    if (launch && (await api.note(launch))) return open(launch, true, true);
    if (prefs.launch === "last") {
      const last = allNotes.find((n) => n.id === prefs.lastNote) ?? listNotes[0];
      if (last) {
        await open(last.id, true, !touch);
        if (layout === "stack" && last.id !== prefs.lastNote) level = "list";
        return;
      }
    }
    newNote(undefined, !touch);
  }

  function say(msg: string, ms = 3500) {
    toast = msg;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), ms);
  }

  async function refreshStatus() {
    status = await api.status();
  }

  async function refresh() {
    if (!status?.configured || !status.root_folder_id) return;
    [folders, allNotes, trashItems] = await Promise.all([api.folders(), api.notes(), api.trash()]);
  }

  function scheduleSync(ms = 3000) {
    clearTimeout(syncTimer);
    syncTimer = setTimeout(() => void syncNow(), ms);
  }

  // ------------------------------------------------------------------ editor

  const MOD_KEYS: { key: string; run: () => void }[] = [
    { key: "Mod-n", run: () => newNote() },
    { key: "Mod-d", run: () => current && void deleteNote(current.id) },
    { key: "Mod-f", run: () => focusSearch() },
    { key: "Mod-k", run: () => (palette = "search") },
    { key: "Mod-e", run: () => openMove(null) },
    { key: "Mod-\\", run: () => toggleSidebar() },
    { key: "Mod-Shift-\\", run: () => toggleFocus() },
    { key: "Mod-|", run: () => toggleFocus() },
    { key: "Mod-,", run: () => (settings = true) },
    { key: "Mod-s", run: () => void syncNow() },
    { key: "Mod-p", run: () => void togglePin() },
    { key: "Mod-w", run: () => void api.hideWindow() },
    { key: "Mod-Shift-o", run: () => void captureText() },
    { key: "Mod-Shift-d", run: () => (sketch = true) },
    { key: "Mod-=", run: () => zoom(1) },
    { key: "Mod-+", run: () => zoom(1) },
    { key: "Mod--", run: () => zoom(-1) },
    { key: "Mod-0", run: () => (prefs.fontSize = FONT_DEFAULT) },
  ];

  function zoom(step: number) {
    prefs.fontSize = Math.max(FONT_MIN, Math.min(FONT_MAX, prefs.fontSize + step));
  }

  function mountEditor(text: string, focus = true, readOnly = false) {
    view?.destroy();
    currentText = text;
    if (!host) return;
    view = createEditor({
      parent: host,
      doc: text,
      readOnly,
      onChange: scheduleSave,
      onTimer: (line) => api.timerCommand(line),
      onImages: (files) => void handleImages(files),
      resolveImage: async (id, sketch) =>
        sketch ? URL.createObjectURL(await api.resourceBlob(id)) : api.resourceSrc(id),
      extraKeys: MOD_KEYS.map((k) => ({ key: k.key, run: () => (k.run(), true) })),
    });
    if (focus && !readOnly) view.focus();
  }

  /** Leaving a note that was emptied deletes it. */
  async function leaveCurrent() {
    // Capture before awaiting: callers switch note right after calling this.
    const leaving = current;
    const text = currentText;
    await flush();
    if (leaving && text.trim() === "") {
      await api.trashNote(leaving.id);
      scheduleSync();
    }
  }

  async function open(id: string, leave = true, focus = true) {
    if (leave && current?.id !== id) await leaveCurrent();
    const n = await api.note(id);
    if (!n) return;
    current = n;
    trashed = null;
    draftFolder = null;
    prefs.lastNote = id;
    mountEditor(n.text, focus && !touch);
    if (layout === "stack") level = "note";
    await refresh();
  }

  /** Opens a note picked in the list; Recently Deleted notes open read-only. */
  async function pick(id: string, focus: boolean) {
    sidebarOverlay = false;
    if (selection !== "trash") return open(id, true, focus);
    const item = trashItems.find((x) => x.id === id);
    if (!item || item.is_folder) return;
    await leaveCurrent();
    current = null;
    trashed = await api.note(id);
    mountEditor(trashed?.text ?? "", false, true);
    if (layout === "stack") level = "note";
  }

  /** A new, empty note; it is written to the database with its first character. */
  function newNote(folder?: string, focus = true) {
    void leaveCurrent().then(refresh);
    if (selection === "trash") selection = "all";
    current = null;
    trashed = null;
    draftFolder = folder ?? (isFolder ? selection : (status?.root_folder_id ?? ""));
    mountEditor("", focus);
    palette = null;
    sidebarOverlay = false;
    if (layout === "stack") level = "note";
  }

  function scheduleSave(text: string) {
    pendingText = text;
    currentText = text;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void flush(), 500);
  }

  /** Writes pending edits to the local database, then schedules a sync. */
  async function flush() {
    clearTimeout(saveTimer);
    const text = pendingText;
    pendingText = null;
    if (text === null) return;
    try {
      if (current) {
        current = await api.updateNote(current.id, text);
      } else if (text.trim() !== "" && !trashed) {
        current = await api.createNote(draftFolder ?? undefined, text);
        prefs.lastNote = current.id;
        draftFolder = null;
      } else {
        return;
      }
      await refresh();
      scheduleSync();
    } catch (e) {
      say(errText(e));
    }
  }

  async function syncNow() {
    try {
      await api.syncNow();
      await refreshStatus();
    } catch (e) {
      syncState = "error";
      syncMsg = String(e);
    }
  }

  // -------------------------------------------------------------- navigation

  /** A sidebar entry: "all", "trash" or a folder. */
  async function select(sel: string) {
    await flush();
    selection = sel;
    prefs.lastFolder = sel === "trash" ? prefs.lastFolder : sel;
    query = "";
    sidebarOverlay = false;
    if (layout === "stack") {
      level = "list";
      return;
    }
    if (sel === "trash") {
      const first = trashItems.find((x) => !x.is_folder);
      if (first) await pick(first.id, false);
      else {
        await leaveCurrent();
        current = null;
        trashed = null;
        mountEditor("", false, true);
      }
      return;
    }
    if (current && listNotes.some((n) => n.id === current!.id)) return;
    if (listNotes.length) await open(listNotes[0].id, true, false);
    else newNote(undefined, false);
  }

  function back() {
    if (level === "note") {
      void leaveCurrent().then(refresh);
      level = "list";
    } else level = "folders";
  }

  function toggleSidebar() {
    if (layout === "three") {
      if (prefs.focus) prefs.focus = false;
      else prefs.sidebarHidden = !prefs.sidebarHidden;
    } else if (layout === "two") sidebarOverlay = !sidebarOverlay;
    else if (level !== "folders") level = "folders";
  }

  function toggleFocus() {
    prefs.focus = !prefs.focus;
    moreMenu = false;
    view?.focus();
  }

  function focusSearch() {
    if (prefs.focus) prefs.focus = false;
    if (layout === "stack" && level === "note") back();
    setTimeout(() => document.querySelector<HTMLInputElement>("[data-search]")?.focus(), 30);
  }

  async function deleteNote(id: string) {
    const wasCurrent = current?.id === id;
    if (wasCurrent) await flush();
    const order = listNotes.map((n) => n.id);
    const pos = order.indexOf(id);
    await api.trashNote(id);
    if (wasCurrent) current = null;
    await refresh();
    say(t("note.movedToTrash"));
    scheduleSync(1500);
    if (!wasCurrent) return;
    if (layout === "stack") {
      level = "list";
      return;
    }
    const next = listNotes[Math.min(Math.max(pos, 0), listNotes.length - 1)];
    if (next) await open(next.id, false, false);
    else newNote(undefined, false);
  }

  async function duplicateNote(id: string) {
    await flush();
    const n = await api.note(id);
    if (!n) return;
    const [first, ...rest] = n.text.split("\n");
    const copy = await api.createNote(n.parent_id, [`${first} ${t("ctx.copySuffix")}`, ...rest].join("\n"));
    await refresh();
    await open(copy.id, true, false);
    scheduleSync();
  }

  function openMove(id: string | null) {
    moreMenu = false;
    moveId = id;
    palette = "move";
  }

  async function moveTo(folderId: string) {
    palette = null;
    const id = moveId ?? current?.id ?? null;
    moveId = null;
    await flush();
    if (!id) {
      draftFolder = folderId;
      view?.focus();
      return;
    }
    await api.moveNote(id, folderId);
    if (current?.id === id) current = await api.note(id);
    await refresh();
    say(t("note.movedTo", { folder: folderTitle(folderId) }));
    scheduleSync(1500);
  }

  async function createFromSearch(text: string) {
    palette = null;
    await leaveCurrent();
    current = await api.createNote(isFolder ? selection : undefined, text);
    draftFolder = null;
    mountEditor(current.text);
    await refresh();
    scheduleSync();
  }

  /** Tree changed (moves, copies, renames): reload lists and the open note's folder. */
  async function treeChanged() {
    await flush();
    await refresh();
    if (current) current = (await api.note(current.id)) ?? current;
    if (isFolder && !folders.some((f) => f.id === selection)) selection = "all";
    scheduleSync(1500);
  }

  async function restoreItem(id: string) {
    await api.restore(id);
    if (trashed?.id === id) trashed = null;
    await refresh();
    say(t("trash.restored"));
    scheduleSync(1500);
    const next = trashItems.find((x) => !x.is_folder);
    if (next) await pick(next.id, false);
    else mountEditor("", false, true);
  }

  async function purgeItem(id: string) {
    const item = trashItems.find((x) => x.id === id);
    if (!confirm(t("trash.purgeConfirm", { name: item?.title || t("note.untitled") }))) return;
    await api.purge(id);
    if (trashed?.id === id) trashed = null;
    await refresh();
    scheduleSync(1500);
    const next = trashItems.find((x) => !x.is_folder);
    if (next) await pick(next.id, false);
    else mountEditor("", false, true);
  }

  async function emptyTrash() {
    if (!trashItems.length || !confirm(t("trash.emptyConfirm"))) return;
    await api.emptyTrash();
    trashed = null;
    mountEditor("", false, true);
    await refresh();
    scheduleSync(1500);
  }

  async function togglePin() {
    pinned = await api.togglePin();
  }

  async function doUnlock() {
    try {
      await api.unlock(unlockPw);
      unlockPw = "";
      await refreshStatus();
      await refresh();
    } catch (e) {
      say(errText(e));
    }
  }

  // --------------------------------------------------------------------- OCR

  async function ocrBlob(image: Blob): Promise<string | null> {
    say(t("ocr.running"), 30000);
    try {
      const text = (await api.ocrImage(image)).trim();
      if (!text) say(t("ocr.empty"));
      else toast = "";
      return text || null;
    } catch (e) {
      say(errText(e));
      return null;
    }
  }

  /** Asks whether a pasted/dropped image goes in as a picture or as text (OCR). */
  function askImage(): Promise<"image" | "ocr" | null> {
    return new Promise((resolve) => {
      imageAsk = (choice) => {
        imageAsk = null;
        resolve(choice);
        view?.focus();
      };
    });
  }

  /** Pasted or dropped images: one question, then all of them as pictures or as text. */
  async function handleImages(files: File[]) {
    const choice = await askImage();
    if (!choice || !view) return;
    if (choice === "ocr") {
      const texts: string[] = [];
      for (const f of files) {
        const text = await ocrBlob(f);
        if (text) texts.push(text);
      }
      if (texts.length) insertText(view, texts.join("\n\n"));
      return;
    }
    try {
      for (const f of files) {
        const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
        const name = f.name && f.name !== "image.png" ? f.name : `image-${stamp}.png`;
        const id = await api.addImage(f, name);
        insertBlock(view, `![${name}](:/${id})`, name.replace(/\.[^.]+$/, ""));
      }
      scheduleSync();
    } catch (e) {
      say(errText(e));
    }
  }

  // ------------------------------------------------------------ handwriting

  /** A sketch tapped in the note reopens on the canvas with its strokes. */
  async function editSketch(e: Event) {
    const edit = (e as CustomEvent<SketchEdit>).detail;
    try {
      const data = await readSketchData(await api.resourceBlob(edit.id));
      if (!data) return say(t("sketch.notEditable"));
      sketchEditing = edit;
      sketchInitial = data;
      sketch = true;
    } catch (err) {
      say(errText(err));
    }
  }

  function closeSketch() {
    sketch = false;
    sketchEditing = null;
    sketchInitial = null;
  }

  /** An edited sketch keeps its attachment id: only the file and the tag's width change. */
  async function saveEditedSketch(view: EditorView, edit: SketchEdit, png: Blob, width: number) {
    await api.updateImage(edit.id, png);
    const doc = view.state.doc;
    let { from, to } = edit;
    // The note may have changed meanwhile (a sync): find the tag again by its id.
    if (!doc.sliceString(from, to).toLowerCase().includes(edit.id)) {
      const m = new RegExp(`<img\\b[^>\\n]*:/${edit.id}[^>\\n]*>`, "i").exec(doc.toString());
      if (!m) return refreshImage(view, edit.id);
      from = m.index;
      to = from + m[0].length;
    }
    const tag = doc.sliceString(from, to);
    const next = /\bwidth=/.test(tag)
      ? tag.replace(/\bwidth=["']?\d+["']?/, `width="${width}"`)
      : tag.replace(/\s*\/?>$/, ` width="${width}">`);
    if (next !== tag) view.dispatch({ changes: { from, to, insert: next } });
    refreshImage(view, edit.id);
  }

  /** A finished drawing becomes an attachment shown at the size it was drawn. */
  async function insertSketch(png: Blob, width: number) {
    const edit = sketchEditing;
    closeSketch();
    if (!view) return;
    if (edit) {
      try {
        await saveEditedSketch(view, edit, png, width);
        scheduleSync();
      } catch (e) {
        say(errText(e));
      }
      return;
    }
    try {
      const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
      const name = `sketch-${stamp}.png`;
      const id = await api.addImage(new File([png], name, { type: "image/png" }), name);
      insertBlock(view, `<img src=":/${id}" alt="${name}" width="${width}">`, t("sketch.title"));
      scheduleSync();
    } catch (e) {
      say(errText(e));
    }
  }

  /** Handwriting read back as text (Apple Vision / tesseract). */
  async function sketchToText(png: Blob) {
    closeSketch();
    view?.focus();
    const text = await ocrBlob(png);
    if (text && view) insertText(view, text);
  }

  async function captureText() {
    try {
      const text = await api.captureText();
      if (text?.trim() && view) insertText(view, text.trim());
      else if (text !== null) say(t("ocr.empty"));
    } catch (e) {
      say(errText(e));
    }
  }

  // ---------------------------------------------------------- input gestures

  /** `Mod-x` style shortcut string for a key event. */
  function shortcutOf(e: KeyboardEvent): string {
    const mod = status?.platform === "macos" || status?.platform === "ios" ? e.metaKey : e.ctrlKey;
    if (!mod) return "";
    return `Mod-${e.shiftKey && e.key.length === 1 && /[a-z\\]/i.test(e.key) ? "Shift-" : ""}${e.key.length === 1 ? e.key.toLowerCase() : e.key}`;
  }

  function windowKey(e: KeyboardEvent) {
    if (sketch) return; // the canvas handles its own keys
    if (imageAsk) {
      const k = e.key.toLowerCase();
      if (k === "i" || k === "enter") imageAsk("image");
      else if (k === "t" || k === "o") imageAsk("ocr");
      else if (k === "escape") imageAsk(null);
      else return;
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    // Shortcuts work outside the editor too (list, sidebar); the editor's own
    // keymap handles them while it has the focus.
    const inEditor = (e.target as HTMLElement | null)?.closest?.(".cm-editor");
    if (!inEditor && !settings && !palette) {
      const sc = shortcutOf(e);
      const hit = sc && MOD_KEYS.find((k) => k.key.toLowerCase() === sc.toLowerCase());
      if (hit) {
        e.preventDefault();
        hit.run();
        return;
      }
    }
    if (e.key !== "Escape") return;
    if (moreMenu) moreMenu = false;
    else if (palette) palette = null;
    else if (sidebarOverlay) sidebarOverlay = false;
    else if (settings) settings = false;
    else if (prefs.focus) prefs.focus = false;
    else if (timer) void api.timerStop();
    else return;
    view?.focus();
  }

  // A file dropped outside the editor text (margins, below the last line) must
  // never reach the webview's default action, which would open the image in
  // place of the app. The editor handles drops on the text itself first.
  function windowDragOver(e: DragEvent) {
    if (e.dataTransfer?.types.includes("Files")) e.preventDefault();
  }

  function windowDrop(e: DragEvent) {
    if (!e.dataTransfer?.types.includes("Files")) return;
    const handled = e.defaultPrevented;
    e.preventDefault();
    if (handled || trashed) return;
    const files = [...e.dataTransfer.files].filter((f) => f.type.startsWith("image/"));
    if (!files.length) return;
    // Insert at the text position nearest to the drop point (below the last line → end).
    if (view) {
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY }, false);
      view.dispatch({ selection: { anchor: pos } });
    }
    void handleImages(files);
  }

  function pointer(e: MouseEvent) {
    nearTop = e.clientY < 72;
  }
</script>

<svelte:window
  onkeydown={windowKey}
  onbeforeunload={() => void flush()}
  onmousemove={pointer}
  ondragenter={windowDragOver}
  ondragover={windowDragOver}
  ondrop={windowDrop}
/>

<div class="shell {layout}" class:focus={prefs.focus} style="--editor-size: {prefs.fontSize}px">
  {#if status?.configured && showSidebar}
    <div class="col sidebar-col">
      {@render sidebarView(mac && leftmost === "sidebar")}
    </div>
  {/if}

  {#if status?.configured && showList}
    <div class="col list-col">
      <NoteList
        title={listTitle}
        trashMode={selection === "trash"}
        notes={listNotes}
        trash={trashItems}
        {folders}
        showFolder={selection === "all" || !!query.trim()}
        currentId={selection === "trash" ? (trashed?.id ?? null) : (current?.id ?? null)}
        bind:query
        bind:searchAll
        scopeName={isFolder ? folderTitle(selection) : ""}
        back={layout === "stack" ? t("nav.folders") : null}
        sidebarButton={layout === "two" || (layout === "three" && prefs.sidebarHidden)}
        lights={mac && leftmost === "list"}
        onToggleSidebar={toggleSidebar}
        onBack={() => (level = "folders")}
        onOpen={(id, focus) => void pick(id, focus)}
        onNew={() => newNote()}
        onMove={(id) => openMove(id)}
        onDuplicate={(id) => void duplicateNote(id)}
        onDelete={(id) => void deleteNote(id)}
        onRestore={(id) => void restoreItem(id)}
        onPurge={(id) => void purgeItem(id)}
        onEmptyTrash={() => void emptyTrash()}
      />
    </div>
  {/if}

  <div class="col editor-col" class:hidden={!showEditor}>
    <header class="toolbar" class:shown={barVisible} class:auto={prefs.autoHideToolbar && layout !== "stack"} class:lights={mac && leftmost === "editor"} data-tauri-drag-region>
      {#if layout === "stack"}
        <button class="back" onclick={back}>{@html icons.prev}<span>{selection === "trash" ? t("nav.recentlyDeleted") : listTitle}</span></button>
      {:else if prefs.focus}
        <button class="icon" title={t("act.unfocus")} aria-label={t("act.unfocus")} onclick={toggleFocus}>{@html icons.unfocus}</button>
      {/if}
      <span class="spacer" data-tauri-drag-region></span>
      {#if !trashed}
        {#if status && !status.mobile}
          <button class="icon" class:on={pinned} title={t("act.pin")} aria-label={t("act.pin")} onclick={togglePin}>{@html icons.pin}</button>
          <button class="icon" title={t("act.capture")} aria-label={t("act.capture")} onclick={() => void captureText()}>{@html icons.capture}</button>
        {/if}
        <button class="icon" title={t("act.draw")} aria-label={t("act.draw")} onclick={() => (sketch = true)}>{@html icons.draw}</button>
        <span class="anchor">
          <button class="icon" title={t("act.more")} aria-label={t("act.more")} onclick={() => (moreMenu = !moreMenu)}>{@html icons.more}</button>
          {#if moreMenu}
            <div class="menu-scrim" onclick={() => (moreMenu = false)} role="presentation"></div>
            <div class="menu more" role="menu">
              <button role="menuitem" disabled={!current && !draftFolder} onclick={() => openMove(null)}>{@html icons.move} {t("act.move")}</button>
              <button role="menuitem" disabled={!current} onclick={() => ((moreMenu = false), current && void duplicateNote(current.id))}
                >{@html icons.duplicate} {t("act.duplicate")}</button
              >
              <button role="menuitem" class="danger" disabled={!current} onclick={() => ((moreMenu = false), current && void deleteNote(current.id))}
                >{@html icons.trash} {t("act.deleteShort")}</button
              >
              <hr />
              {#if layout !== "stack"}
                <button role="menuitem" onclick={toggleFocus}>{@html prefs.focus ? icons.unfocus : icons.focus} {t(prefs.focus ? "act.unfocus" : "act.focus")}</button>
              {/if}
              <div class="menu-row">
                <span>{t("set.textSize")}</span>
                <button class="step" onclick={() => zoom(-1)} aria-label="−">−</button>
                <span class="num">{prefs.fontSize}</span>
                <button class="step" onclick={() => zoom(1)} aria-label="+">+</button>
              </div>
              {#if !status?.local}
                <button role="menuitem" onclick={() => ((moreMenu = false), void syncNow())}>{@html icons.sync} {t("act.sync")}</button>
              {/if}
              <button role="menuitem" onclick={() => ((moreMenu = false), (settings = true))}>{@html icons.settings} {t("nav.settings")}</button>
            </div>
          {/if}
        </span>
        <button class="icon" title={t("act.newNote")} aria-label={t("act.newNote")} onclick={() => newNote()}>{@html icons.compose}</button>
      {/if}
    </header>

    {#if trashed}
      <div class="banner">
        <span>{t("note.inTrash")}</span>
        <button onclick={() => trashed && void restoreItem(trashed.id)}>{@html icons.restore} {t("trash.restore")}</button>
        <button class="danger" onclick={() => trashed && void purgeItem(trashed.id)}>{@html icons.trash} {t("trash.purge")}</button>
      </div>
    {/if}
    <div class="meta">
      {#if current}
        {t("note.editedOn", { when: fullDate(current.updated_time, i18n.lang) })}{#if selection === "all" || layout !== "three"} · {folderTitle(current.parent_id)}{/if}
      {:else if draftFolder !== null && !trashed && selection !== "trash"}
        {capitalize(t("note.newIn", { folder: folderTitle(openFolder) }))}
      {/if}
    </div>
    <main bind:this={host} class:readonly={!!trashed}></main>

    {#if current?.encrypted}
      <div class="notice">{@html icons.lock} {t("note.encryptedHint")}</div>
    {/if}
  </div>

  {#if timer}
    <button
      class="timer"
      class:paused={!timer.running && !timerDone}
      class:done={timerDone}
      title={t("timer.hint")}
      onclick={() => void api.timerToggle()}
      ondblclick={() => void api.timerStop()}
    >
      <span class="t-time">{timer.label}</span>
      {#if timer.kind === "pomodoro"}<span class="t-phase">{t(timer.phase === "work" ? "timer.work" : "timer.rest")}</span>{/if}
      {#if timer.title}<span class="t-title">{timer.title}</span>{/if}
    </button>
  {/if}
</div>

{#snippet sidebarView(lights: boolean)}
  <FolderSidebar
    {folders}
    notes={allNotes}
    rootId={treeRoot}
    noteHome={status?.root_folder_id ?? ""}
    {selection}
    trashCount={trashNotes}
    {syncState}
    syncMessage={syncMsg}
    {lastSync}
    local={!!status?.local}
    {lights}
    onSelect={(s) => void select(s)}
    onNewNote={(f) => newNote(f)}
    onTrashNote={(id) => void deleteNote(id)}
    onHomeChanged={() => void refreshStatus()}
    onChanged={() => void treeChanged()}
    onSync={() => void syncNow()}
    onSettings={() => ((sidebarOverlay = false), (settings = true))}
  />
{/snippet}

{#if status?.configured && layout === "two" && sidebarOverlay}
  <div class="scrim" onclick={() => (sidebarOverlay = false)} role="presentation"></div>
  <div class="overlay-sidebar">{@render sidebarView(mac)}</div>
{/if}

{#if imageAsk}
  <div class="modal" role="dialog" aria-label={t("img.ask")}>
    <div class="box choice">
      <h2>{t("img.ask")}</h2>
      <div class="tiles">
        <button class="tile default" onclick={() => imageAsk?.("image")}>
          <span class="big">{@html icons.image}</span>
          <span>{t("img.image")}</span>
          <kbd>I</kbd>
        </button>
        <button class="tile" onclick={() => imageAsk?.("ocr")}>
          <span class="big">{@html icons.ocr}</span>
          <span>{t("img.text")}</span>
          <kbd>T</kbd>
        </button>
      </div>
      <button class="link" onclick={() => imageAsk?.(null)}>{t("setup.cancel")} <kbd>Esc</kbd></button>
    </div>
  </div>
{/if}

{#if sketch}
  <Sketch
    native={ipad}
    initial={sketchInitial}
    onImage={insertSketch}
    onText={sketchToText}
    onClose={() => (closeSketch(), view?.focus())}
  />
{/if}

{#if toast}<div class="toast">{toast}</div>{/if}

{#if palette}
  <Palette
    mode={palette}
    {folders}
    notes={allNotes}
    onPickNote={(id) => ((palette = null), void open(id))}
    onPickFolder={moveTo}
    onCreate={createFromSearch}
    onClose={() => ((palette = null), (moveId = null), view?.focus())}
  />
{/if}

{#if status && (!status.configured || connecting)}
  <Setup
    localRoot={connecting ? status.root_folder_id : ""}
    onCancel={connecting ? () => (connecting = false) : undefined}
    onDone={async () => {
      connecting = false;
      await refreshStatus();
      await refresh();
      if (allNotes.length) await open(allNotes[0].id, false);
      else newNote();
    }}
  />
{:else if status?.locked}
  <div class="modal">
    <div class="box">
      <h2>{t("unlock.title")}</h2>
      <p>{t("unlock.text")}</p>
      <input type="password" bind:value={unlockPw} onkeydown={(e) => e.key === "Enter" && doUnlock()} />
      <button class="primary" onclick={doUnlock}>{t("unlock.button")}</button>
    </div>
  </div>
{:else if settings && status}
  <Settings
    {status}
    {folders}
    bind:theme
    {syncState}
    {lastSync}
    narrow={width < 700}
    onSync={() => void syncNow()}
    onRootChanged={async () => {
      await refreshStatus();
      selection = "all";
      current = null;
      await refresh();
      if (allNotes.length) await open(allNotes[0].id, false, false);
      else newNote(undefined, false);
    }}
    onLogout={refreshStatus}
    onConnect={() => ((settings = false), (connecting = true))}
    onClose={() => ((settings = false), view?.focus())}
  />
{/if}

<style>
  .shell {
    position: relative;
    height: 100dvh;
    display: flex;
    overflow: hidden;
  }
  .col {
    height: 100%;
    min-width: 0;
  }
  .sidebar-col {
    flex: 0 0 16.5rem;
  }
  .list-col {
    flex: 0 0 21rem;
  }
  .editor-col {
    flex: 1;
    position: relative;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .editor-col.hidden {
    display: none;
  }
  /* Narrow layouts: one column at a time, full width. */
  .stack .sidebar-col,
  .stack .list-col {
    flex: 1;
  }
  .two .list-col {
    flex-basis: 19rem;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: env(safe-area-inset-top) 8px 0;
    min-height: calc(var(--bar-h) + env(safe-area-inset-top));
    background: var(--bg);
    flex: none;
  }
  .toolbar.lights {
    padding-left: 84px;
  }
  /* Auto-hide: the bar floats over the note and slides away. */
  .toolbar.auto {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 5;
    background: var(--bar);
    border-bottom: 1px solid var(--line);
    transform: translateY(-100%);
    opacity: 0;
    transition:
      transform 0.14s ease,
      opacity 0.14s ease;
  }
  .toolbar.auto.shown {
    transform: none;
    opacity: 1;
  }
  .toolbar.auto ~ .meta {
    margin-top: calc(var(--bar-h) + env(safe-area-inset-top));
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .icon {
    line-height: 1;
    font-size: var(--ico-l, 22px);
    min-width: 44px;
    min-height: 44px;
    padding: 4px 8px;
    color: var(--muted);
  }
  .icon:hover {
    color: var(--text);
  }
  .icon.on {
    color: var(--accent);
  }
  .back {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: 44px;
    padding-right: 8px;
    color: var(--accent);
    max-width: 55%;
  }
  .back :global(.ico) {
    font-size: var(--ico-l, 22px);
  }
  .back span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .anchor {
    position: relative;
  }
  .meta {
    flex: none;
    min-height: 1.4em;
    padding: 2px 20px 0;
    text-align: center;
    font-size: 0.74rem;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  main {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  main.readonly {
    opacity: 0.8;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin: 4px 16px;
    padding: 6px 10px;
    border: 1px solid var(--line);
    font-size: 0.82rem;
    color: var(--muted);
  }
  .banner span {
    flex: 1;
  }
  .banner button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--accent);
    padding: 4px 6px;
  }
  .banner .danger {
    color: var(--danger);
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 10;
    background: rgba(0, 0, 0, 0.3);
  }
  .overlay-sidebar {
    position: fixed;
    z-index: 11;
    top: 0;
    bottom: 0;
    left: 0;
    width: min(18rem, 86vw);
    border-right: 1px solid var(--accent);
  }
  .menu-scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 41;
    min-width: 15rem;
    display: flex;
    flex-direction: column;
    padding: 4px;
    background: var(--bg-elev);
    border: 1px solid var(--accent);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.35);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    padding: 7px 10px;
  }
  .menu button:hover:not(:disabled) {
    background: var(--panel);
  }
  .menu button:disabled {
    opacity: 0.35;
  }
  .menu .danger {
    color: var(--danger);
  }
  .menu hr {
    border: 0;
    border-top: 1px solid var(--line);
    margin: 4px 0;
  }
  .menu-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
  }
  .menu-row span:first-child {
    flex: 1;
  }
  .menu-row .step {
    padding: 2px 9px;
    border: 1px solid var(--line);
  }
  .menu-row .num {
    min-width: 2ch;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .notice {
    position: absolute;
    left: 50%;
    bottom: calc(24px + env(safe-area-inset-bottom));
    transform: translateX(-50%);
    color: var(--muted);
    font-size: 0.8rem;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .timer {
    position: absolute;
    right: 12px;
    bottom: calc(12px + env(safe-area-inset-bottom));
    z-index: 6;
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 10px;
    background: var(--bar);
    border: 1px solid var(--accent);
    color: var(--accent);
    font-variant-numeric: tabular-nums;
    max-width: calc(100vw - 24px);
  }
  .timer.paused {
    opacity: 0.6;
  }
  .timer.done {
    background: var(--accent);
    color: var(--bg);
    animation: pulse 0.8s infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.25;
    }
  }
  .t-time {
    font-weight: 700;
  }
  .t-phase,
  .t-title {
    font-size: 0.78rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .modal {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: grid;
    place-items: center;
    padding: 18px;
    z-index: 20;
  }
  .box {
    background: var(--bg-elev);
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    padding: 18px;
    width: min(24rem, 100%);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .box h2 {
    margin: 0;
    font-size: 1rem;
    color: var(--accent);
  }
  .box p {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .primary {
    background: var(--accent);
    color: var(--bg);
    padding: 8px;
    font-weight: 700;
  }
  .tiles {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 16px 10px 10px;
    border: 1px solid var(--line);
    color: var(--text);
  }
  .tile .big {
    font-size: 2.6rem;
    line-height: 1;
    color: var(--muted);
  }
  .tile.default,
  .tile:hover {
    border-color: var(--accent);
  }
  .tile.default .big,
  .tile:hover .big {
    color: var(--accent);
  }
  .choice kbd {
    font-family: var(--font);
    font-size: 0.75rem;
    color: var(--muted);
  }
  .choice .link {
    align-self: center;
    color: var(--muted);
  }
  .toast {
    position: fixed;
    bottom: calc(18px + env(safe-area-inset-bottom));
    left: 50%;
    transform: translateX(-50%);
    background: var(--bg-elev);
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    padding: 7px 12px;
    font-size: 0.82rem;
    max-width: 90vw;
    z-index: 30;
  }
</style>
