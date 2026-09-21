<script lang="ts">
  import { onMount } from "svelte";
  import type { EditorView } from "@codemirror/view";
  import {
    api,
    onCaptureText,
    onDataChanged,
    onFileDrop,
    onQuickNote,
    onSyncStatus,
    onTimer,
    onTimerFinished,
    type Folder,
    type Note,
    type NoteSummary,
    type Status,
    type TimerTick,
  } from "$lib/api";
  import { createEditor, insertText } from "$lib/editor";
  import { errText, i18n, t } from "$lib/i18n.svelte";
  import { initTheme, type ThemeState } from "$lib/theme";
  import Setup from "$lib/components/Setup.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import Palette from "$lib/components/Palette.svelte";
  import Settings from "$lib/components/Settings.svelte";

  const FONT_KEY = "omanote.fontSize";
  const IMAGE_EXT = /\.(png|jpe?g|gif|webp|heic|bmp|tiff?)$/i;

  let status = $state<Status | null>(null);
  let folders = $state<Folder[]>([]);
  let notes = $state<NoteSummary[]>([]); // newest first
  let selectedFolder = $state("");
  let current = $state<Note | null>(null);
  let draftFolder = $state<string | null>(null); // a new, not yet saved note
  let sidebar = $state(false);
  let palette = $state<"search" | "move" | null>(null);
  let settings = $state(false);
  let syncState = $state<"idle" | "syncing" | "error">("idle");
  let syncMsg = $state("");
  let toast = $state("");
  let unlockPw = $state("");
  let theme = $state<ThemeState>({ current: "tokyo-night", fromSystem: false });
  let timer = $state<TimerTick | null>(null);
  let timerDone = $state(false);
  let pinned = $state(false);
  let nearTop = $state(false);
  let fontSize = $state(readFontSize());
  const touch = typeof matchMedia !== "undefined" && matchMedia("(hover: none)").matches;

  let host: HTMLElement | undefined = $state();
  let view: EditorView | undefined;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let syncTimer: ReturnType<typeof setTimeout> | undefined;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let pendingText: string | null = null;
  let currentText = "";

  const index = $derived(current ? notes.findIndex((n) => n.id === current!.id) : -1);
  const position = $derived(index >= 0 ? `${notes.length - index}/${notes.length}` : `+`);
  const folderName = $derived(
    folders.find((f) => f.id === (current?.parent_id ?? draftFolder ?? selectedFolder))?.title ?? "Omanote",
  );
  const barVisible = $derived(touch || nearTop || sidebar || !!palette || settings || syncState === "error");

  // macOS: the traffic lights appear and disappear with the hover menu.
  $effect(() => {
    if (status?.platform === "macos") void api.setWindowControls(barVisible);
  });

  function readFontSize(): number {
    try {
      return Number(localStorage.getItem(FONT_KEY)) || 14;
    } catch {
      return 14;
    }
  }

  $effect(() => {
    try {
      localStorage.setItem(FONT_KEY, String(fontSize));
    } catch {
      /* ignore */
    }
  });

  onMount(() => {
    const offs: (() => void)[] = [];
    void (async () => {
      offs.push(await initTheme((s) => (theme = s)));
      document.documentElement.lang = i18n.lang;
      void api.setLanguage(i18n.lang);
      await refreshStatus();
      await refresh();
      timer = await api.timerState();
      if (notes.length) await open(notes[0].id);
      else newNote();

      offs.push(
        await onSyncStatus((e) => {
          syncState = e.state;
          syncMsg = e.message;
          if (e.report?.needs_password) void refreshStatus();
        }),
        await onDataChanged(async () => {
          await refresh();
          if (current && !pendingText) {
            const fresh = await api.note(current.id);
            if (!fresh) newNote();
            else if (fresh.updated_time !== current.updated_time && fresh.text !== currentText) await open(current.id, false);
          }
        }),
        await onQuickNote(() => newNote()),
        await onCaptureText(() => void captureText()),
        await onTimer((tk) => {
          timer = tk;
          if (!tk) timerDone = false;
        }),
        await onTimerFinished(() => {
          timerDone = true;
          setTimeout(() => (timerDone = false), 8000);
        }),
        await onFileDrop((paths) => void ocrPaths(paths)),
      );
    })();
    return () => offs.forEach((off) => off());
  });

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
    [folders, notes] = await Promise.all([api.folders(), api.notes(selectedFolder || undefined)]);
  }

  function scheduleSync(ms = 3000) {
    clearTimeout(syncTimer);
    syncTimer = setTimeout(() => void syncNow(), ms);
  }

  // ------------------------------------------------------------------ editor

  const noteKeys = () => [
    { key: "Mod-n", run: () => (newNote(), true) },
    { key: "Mod-[", run: () => (go(-1), true) },
    { key: "Mod-]", run: () => (go(1), true) },
    { key: "Mod-1", run: () => (jumpToFront(), true) },
    { key: "Mod-Shift-1", run: () => (void promote(), true) },
    { key: "Mod-d", run: () => (void trash(true), true) },
    { key: "Mod-f", run: () => ((palette = "search"), true) },
    { key: "Mod-k", run: () => ((palette = "search"), true) },
    { key: "Mod-e", run: () => ((palette = "move"), true) },
    { key: "Mod-\\", run: () => ((sidebar = !sidebar), true) },
    { key: "Mod-,", run: () => ((settings = true), true) },
    { key: "Mod-s", run: () => (void syncNow(), true) },
    { key: "Mod-p", run: () => (void togglePin(), true) },
    { key: "Mod-w", run: () => (void api.hideWindow(), true) },
    { key: "Mod-o", run: () => (void api.hideWindow(), true) },
    { key: "Mod-Shift-o", run: () => (void captureText(), true) },
    { key: "Mod-=", run: () => ((fontSize = Math.min(24, fontSize + 1)), true) },
    { key: "Mod-+", run: () => ((fontSize = Math.min(24, fontSize + 1)), true) },
    { key: "Mod--", run: () => ((fontSize = Math.max(11, fontSize - 1)), true) },
    { key: "Mod-0", run: () => ((fontSize = 14), true) },
  ];

  function mountEditor(text: string, focus = true) {
    view?.destroy();
    currentText = text;
    if (!host) return;
    view = createEditor({
      parent: host,
      doc: text,
      onChange: scheduleSave,
      onTimer: (line) => api.timerCommand(line),
      onImage: ocrBlob,
      extraKeys: noteKeys(),
    });
    if (focus) view.focus();
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

  async function open(id: string, leave = true) {
    if (leave) await leaveCurrent();
    const n = await api.note(id);
    if (!n) return;
    current = n;
    draftFolder = null;
    mountEditor(n.text);
    await refresh();
  }

  function newNote() {
    void leaveCurrent().then(refresh);
    current = null;
    draftFolder = selectedFolder || status?.root_folder_id || "";
    mountEditor("");
    sidebar = false;
    palette = null;
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
      } else if (text.trim() !== "") {
        current = await api.createNote(draftFolder ?? undefined, text);
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

  /** -1 = older, +1 = newer; going past the newest note starts a new one. */
  function go(dir: -1 | 1) {
    if (!current) {
      if (dir === -1 && notes.length) void open(notes[0].id);
      return;
    }
    const next = index - dir;
    if (next < 0) newNote();
    else if (next < notes.length) void open(notes[next].id);
  }

  function jumpToFront() {
    if (notes.length) void open(notes[0].id);
  }

  async function promote() {
    if (!current) return;
    await flush();
    await api.promoteNote(current.id);
    await refresh();
    scheduleSync();
  }

  async function trash(ask: boolean) {
    if (!current) return;
    if (ask && currentText.trim() !== "" && !confirm(t("note.deleteConfirm"))) return;
    const id = current.id;
    await flush();
    await api.trashNote(id);
    const pos = notes.findIndex((n) => n.id === id);
    current = null;
    await refresh();
    if (notes.length) await open(notes[Math.min(pos, notes.length - 1)].id, false);
    else newNote();
    scheduleSync(1500);
  }

  async function selectFolder(id: string) {
    selectedFolder = id;
    sidebar = false;
    await leaveCurrent();
    current = null;
    await refresh();
    if (notes.length) await open(notes[0].id, false);
    else newNote();
  }

  async function moveTo(folderId: string) {
    palette = null;
    await flush();
    if (!current) {
      draftFolder = folderId;
      view?.focus();
      return;
    }
    await api.moveNote(current.id, folderId);
    current = await api.note(current.id);
    await refresh();
    scheduleSync(1500);
    view?.focus();
  }

  async function createFromSearch(text: string) {
    palette = null;
    await leaveCurrent();
    current = await api.createNote(selectedFolder || undefined, text);
    draftFolder = null;
    mountEditor(current.text);
    await refresh();
    scheduleSync();
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

  async function ocrPaths(paths: string[]) {
    const images = paths.filter((p) => IMAGE_EXT.test(p));
    if (!images.length || !view) return;
    say(t("ocr.running"), 30000);
    try {
      const texts = await Promise.all(images.map((p) => api.ocrFile(p)));
      const text = texts.map((x) => x.trim()).filter(Boolean).join("\n\n");
      if (text) {
        insertText(view, text);
        toast = "";
      } else say(t("ocr.empty"));
    } catch (e) {
      say(errText(e));
    }
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

  // Two-finger horizontal swipe (trackpad) and touch swipe move between notes.
  let wheelX = 0;
  let wheelLock = 0;
  function wheel(e: WheelEvent) {
    if (Math.abs(e.deltaX) <= Math.abs(e.deltaY) || Date.now() < wheelLock) return;
    wheelX += e.deltaX;
    if (Math.abs(wheelX) > 140) {
      go(wheelX > 0 ? 1 : -1);
      wheelX = 0;
      wheelLock = Date.now() + 700;
    }
  }

  let touchX = 0;
  let touchY = 0;
  function touchStart(e: TouchEvent) {
    touchX = e.changedTouches[0].clientX;
    touchY = e.changedTouches[0].clientY;
  }
  function touchEnd(e: TouchEvent) {
    const dx = e.changedTouches[0].clientX - touchX;
    const dy = e.changedTouches[0].clientY - touchY;
    if (Math.abs(dx) > 80 && Math.abs(dy) < 45) go(dx < 0 ? 1 : -1);
  }

  function windowKey(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (palette) palette = null;
    else if (sidebar) sidebar = false;
    else if (settings) settings = false;
    else if (timer) void api.timerStop();
    view?.focus();
  }

  function pointer(e: MouseEvent) {
    nearTop = e.clientY < 56;
  }
</script>

<svelte:window onkeydown={windowKey} onbeforeunload={() => void flush()} onmousemove={pointer} />

<div class="app" style="--editor-size: {fontSize}px">
  <header class:shown={barVisible} class:mac={status?.platform === "macos"} data-tauri-drag-region>
    <button class="icon" title={t("nav.folders")} onclick={() => (sidebar = true)}>☰</button>
    <button class="crumb" title={t("act.move")} onclick={() => (palette = "move")}>{folderName}</button>
    <span class="pos">{position}</span>
    <span class="spacer" data-tauri-drag-region></span>
    {#if status && !status.mobile}
      <button class="icon" class:on={pinned} title={t("act.pin")} onclick={togglePin}>⌃</button>
      <button class="icon" title={t("act.capture")} onclick={() => void captureText()}>⌖</button>
    {/if}
    <button
      class="icon dot"
      class:syncing={syncState === "syncing"}
      class:error={syncState === "error"}
      title={syncState === "error" ? `${t("sync.error")}: ${syncMsg}` : t("act.sync")}
      onclick={() => void syncNow()}>●</button
    >
    <button class="icon" title={t("act.search")} onclick={() => (palette = "search")}>⌕</button>
    <button class="icon" title={t("act.newNote")} onclick={newNote}>＋</button>
    <button class="icon" title={t("nav.settings")} onclick={() => (settings = true)}>⚙</button>
  </header>

  <main bind:this={host} onwheel={wheel} ontouchstart={touchStart} ontouchend={touchEnd}></main>

  {#if current?.encrypted}
    <div class="notice">🔒 {t("note.encryptedHint")}</div>
  {/if}

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

{#if toast}<div class="toast">{toast}</div>{/if}

{#if sidebar}
  <Sidebar
    {folders}
    {notes}
    selected={selectedFolder}
    currentId={current?.id ?? null}
    rootId={status?.root_folder_id ?? ""}
    onSelectFolder={selectFolder}
    onSelectNote={(id) => ((sidebar = false), void open(id))}
    onChanged={refresh}
    onClose={() => (sidebar = false)}
    onSettings={() => ((sidebar = false), (settings = true))}
  />
{/if}

{#if palette}
  <Palette
    mode={palette}
    {folders}
    {notes}
    onPickNote={(id) => ((palette = null), void open(id))}
    onPickFolder={moveTo}
    onCreate={createFromSearch}
    onClose={() => ((palette = null), view?.focus())}
  />
{/if}

{#if status && !status.configured}
  <Setup
    onDone={async () => {
      await refreshStatus();
      await refresh();
      newNote();
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
    bind:fontSize
    onRootChanged={async () => {
      await refreshStatus();
      selectedFolder = "";
      current = null;
      await refresh();
      if (notes.length) await open(notes[0].id, false);
      else newNote();
    }}
    onLogout={refreshStatus}
    onClose={() => ((settings = false), view?.focus())}
  />
{/if}

<style>
  .app {
    position: relative;
    height: 100dvh;
  }
  header {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: env(safe-area-inset-top) 10px 0;
    min-height: calc(var(--bar-h) + env(safe-area-inset-top));
    background: var(--bar);
    border-bottom: 1px solid var(--line);
    font-size: 0.95rem;
    transform: translateY(-100%);
    opacity: 0;
    transition:
      transform 0.14s ease,
      opacity 0.14s ease;
  }
  /* Room for the macOS traffic lights (no title bar). */
  header.mac {
    padding-left: 84px;
  }
  header.shown {
    transform: none;
    opacity: 1;
  }
  main {
    position: absolute;
    inset: 0;
    padding-top: calc(var(--bar-h) + 6px + env(safe-area-inset-top));
    overflow: hidden;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .icon {
    line-height: 1;
    font-size: 1.25rem;
    min-width: 34px;
    min-height: 32px;
    padding: 6px 8px;
    color: var(--muted);
  }
  .icon:hover {
    color: var(--text);
  }
  .icon.on {
    color: var(--accent);
  }
  .crumb {
    font-weight: 700;
    color: var(--accent);
    padding: 6px 8px;
  }
  .pos {
    color: var(--muted);
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
  }
  .dot {
    color: var(--result);
    font-size: 0.8rem;
  }
  .dot.syncing {
    color: var(--accent);
    animation: pulse 1s infinite;
  }
  .dot.error {
    color: var(--danger);
  }
  @keyframes pulse {
    50% {
      opacity: 0.25;
    }
  }
  .notice {
    position: absolute;
    left: 50%;
    bottom: calc(14px + env(safe-area-inset-bottom));
    transform: translateX(-50%);
    color: var(--muted);
    font-size: 0.8rem;
  }
  .timer {
    position: absolute;
    right: 12px;
    bottom: calc(12px + env(safe-area-inset-bottom));
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
  .toast {
    position: fixed;
    top: calc(var(--bar-h) + 12px + env(safe-area-inset-top));
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
