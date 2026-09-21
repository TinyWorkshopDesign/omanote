<script lang="ts">
  import { onMount } from "svelte";
  import type { EditorView } from "@codemirror/view";
  import {
    api,
    onDataChanged,
    onQuickNote,
    onSyncStatus,
    when,
    type Folder,
    type Note,
    type NoteSummary,
    type Status,
  } from "$lib/api";
  import { createEditor } from "$lib/editor";
  import Setup from "$lib/components/Setup.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import Palette from "$lib/components/Palette.svelte";
  import { bundledThemes, initTheme, setBundledTheme, type ThemeState } from "$lib/theme";

  let status = $state<Status | null>(null);
  let folders = $state<Folder[]>([]);
  let notes = $state<NoteSummary[]>([]);
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

  let host: HTMLElement | undefined = $state();
  let view: EditorView | undefined;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let syncTimer: ReturnType<typeof setTimeout> | undefined;
  let pendingText: string | null = null;

  const index = $derived(current ? notes.findIndex((n) => n.id === current!.id) : -1);
  const folderName = $derived(
    draftFolder !== null || !current
      ? folders.find((f) => f.id === (draftFolder ?? selectedFolder))?.title ?? "Omanote"
      : folders.find((f) => f.id === current!.parent_id)?.title ?? "Omanote",
  );

  onMount(() => {
    const offs: (() => void)[] = [];
    void (async () => {
      offs.push(await initTheme((s) => (theme = s)));
      await refreshStatus();
      await refresh();
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
          if (current) {
            const fresh = await api.note(current.id);
            // Reload only if the note changed remotely and we are not typing.
            if (fresh && fresh.updated_time !== current.updated_time && !pendingText) await open(current.id);
          }
        }),
        await onQuickNote(() => newNote()),
      );
    })();
    return () => offs.forEach((off) => off());
  });

  async function refreshStatus() {
    status = await api.status();
  }

  async function refresh() {
    if (!status?.configured || !status.root_folder_id) return;
    [folders, notes] = await Promise.all([api.folders(), api.notes(selectedFolder || undefined)]);
  }

  function mountEditor(text: string) {
    view?.destroy();
    if (!host) return;
    view = createEditor({
      parent: host,
      doc: text,
      onChange: scheduleSave,
      extraKeys: [
        { key: "Mod-n", run: () => (newNote(), true) },
        { key: "Mod-k", run: () => ((palette = "search"), true) },
        { key: "Mod-\\", run: () => ((sidebar = !sidebar), true) },
        { key: "Mod-[", run: () => (go(-1), true) },
        { key: "Mod-]", run: () => (go(1), true) },
        { key: "Mod-Shift-m", run: () => ((palette = "move"), true) },
        { key: "Mod-s", run: () => (void syncNow(), true) },
        { key: "Mod-Backspace", run: () => (void trash(), true) },
      ],
    });
    view.focus();
  }

  async function open(id: string) {
    await flush();
    const n = await api.note(id);
    if (!n) return;
    current = n;
    draftFolder = null;
    mountEditor(n.text);
  }

  function newNote() {
    void flush();
    current = null;
    draftFolder = selectedFolder || status?.root_folder_id || "";
    mountEditor("");
    sidebar = false;
    palette = null;
  }

  function scheduleSave(text: string) {
    pendingText = text;
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
      clearTimeout(syncTimer);
      syncTimer = setTimeout(() => void syncNow(), 3000);
    } catch (e) {
      toast = String(e);
      setTimeout(() => (toast = ""), 4000);
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

  function go(delta: number) {
    if (!notes.length) return;
    if (!current) {
      void open(notes[0].id);
      return;
    }
    const next = index + delta;
    if (next < 0 || next >= notes.length) return;
    void open(notes[next].id);
  }

  async function trash() {
    if (!current) return;
    const id = current.id;
    await flush();
    await api.trashNote(id);
    const pos = notes.findIndex((n) => n.id === id);
    await refresh();
    if (notes.length) await open(notes[Math.min(pos, notes.length - 1)].id);
    else newNote();
    clearTimeout(syncTimer);
    syncTimer = setTimeout(() => void syncNow(), 1500);
  }

  async function selectFolder(id: string) {
    selectedFolder = id;
    await refresh();
    sidebar = false;
    if (notes.length) await open(notes[0].id);
    else newNote();
  }

  async function moveTo(folderId: string) {
    palette = null;
    await flush();
    if (!current) {
      draftFolder = folderId;
      return;
    }
    await api.moveNote(current.id, folderId);
    current = await api.note(current.id);
    await refresh();
    clearTimeout(syncTimer);
    syncTimer = setTimeout(() => void syncNow(), 1500);
  }

  async function doUnlock() {
    try {
      await api.unlock(unlockPw);
      unlockPw = "";
      await refreshStatus();
      await refresh();
    } catch (e) {
      toast = String(e);
    }
  }

  // Swipe between notes (mobile).
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
    if (e.key === "Escape") {
      if (palette) palette = null;
      else if (sidebar) sidebar = false;
      else if (settings) settings = false;
      else view?.focus();
    }
  }
</script>

<svelte:window onkeydown={windowKey} onbeforeunload={() => void flush()} />

<div class="app">
  <header data-tauri-drag-region>
    <button class="icon" title="Cartelle (⌘\)" onclick={() => (sidebar = true)}>☰</button>
    <button class="crumb" title="Sposta nota (⌘⇧M)" onclick={() => (palette = "move")}>{folderName}</button>
    <span class="pos">{index >= 0 ? `${index + 1}/${notes.length}` : current ? "" : "nuova"}</span>
    <span class="spacer"></span>
    <button
      class="icon dot"
      class:syncing={syncState === "syncing"}
      class:error={syncState === "error"}
      title={syncState === "error" ? syncMsg : "Sincronizza (⌘S)"}
      onclick={() => void syncNow()}>●</button
    >
    <button class="icon" title="Cerca (⌘K)" onclick={() => (palette = "search")}>⌕</button>
    <button class="icon" title="Nuova nota (⌘N)" onclick={newNote}>＋</button>
  </header>

  <main bind:this={host} ontouchstart={touchStart} ontouchend={touchEnd}></main>

  <footer>
    <button class="nav" disabled={index <= 0} onclick={() => go(-1)}>‹</button>
    <span class="meta">
      {#if current?.encrypted}🔒 nota cifrata, serve la password master
      {:else if current}modificata {when(current.updated_time)}
      {:else}nuova nota in {folderName}{/if}
    </span>
    <button class="nav" disabled={index < 0 || index >= notes.length - 1} onclick={() => go(1)}>›</button>
    <button class="icon" title="Cestino (⌘⌫)" onclick={() => void trash()} disabled={!current}>🗑</button>
  </footer>
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
    onClose={() => (palette = null)}
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
      <h2>Note cifrate</h2>
      <p>Inserisci la password master E2EE di Joplin per leggere e scrivere le note.</p>
      <input type="password" bind:value={unlockPw} onkeydown={(e) => e.key === "Enter" && doUnlock()} />
      <button class="primary" onclick={doUnlock}>Sblocca</button>
    </div>
  </div>
{:else if settings}
  <div class="modal">
    <div class="box">
      <h2>Impostazioni</h2>
      <p class="row"><span>Server</span><span>{status?.server_url}</span></p>
      <p class="row"><span>Account</span><span>{status?.email}</span></p>
      <p class="row"><span>E2EE</span><span>{status?.e2ee ? "attiva" : "non attiva"}</span></p>
      <p class="row"><span>Scorciatoia</span><span>{status?.hotkey}</span></p>
      <p class="row"><span>Notebook</span><span>{folders.find((f) => f.id === status?.root_folder_id)?.title ?? "—"}</span></p>
      <p class="row">
        <span>Tema</span>
        <span>{theme.fromSystem ? `${theme.current} (da Omarchy)` : theme.current}</span>
      </p>
      {#if !theme.fromSystem}
        <div class="themes">
          {#each bundledThemes as t (t.name)}
            <button
              class="swatch"
              class:sel={theme.current === t.name}
              title={t.name}
              style="background: {t.bg}; border-color: {t.accent}"
              aria-label={t.name}
              onclick={() => {
                setBundledTheme(t.name);
                theme = { current: t.name, fromSystem: false };
              }}
            ><span style="background: {t.accent}"></span></button>
          {/each}
        </div>
      {:else}
        <p class="hint">Omanote segue il tema di Omarchy: cambialo con <code>omarchy-theme-set</code>.</p>
      {/if}
      <div class="actions">
        <button onclick={() => (settings = false)}>Chiudi</button>
        <button
          class="danger"
          onclick={async () => {
            if (confirm("Disconnettere l'account e cancellare la copia locale?")) {
              await api.logout();
              await refreshStatus();
            }
          }}>Disconnetti</button
        >
      </div>
    </div>
  </div>
{/if}

<style>
  .app {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: 100dvh;
  }
  header,
  footer {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    min-height: var(--bar-h);
    background: var(--bar);
    font-size: 0.82rem;
  }
  header {
    border-bottom: 1px solid var(--line);
    padding-top: env(safe-area-inset-top);
  }
  footer {
    border-top: 1px solid var(--line);
    padding-bottom: env(safe-area-inset-bottom);
    color: var(--muted);
  }
  main {
    overflow: hidden;
    position: relative;
  }
  .spacer {
    flex: 1;
  }
  .icon {
    font-size: 1rem;
    line-height: 1;
    padding: 5px 7px;
    color: var(--muted);
  }
  .icon:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .crumb {
    font-weight: 700;
    color: var(--accent);
  }
  .pos {
    color: var(--muted);
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
  }
  .dot {
    color: var(--result);
    font-size: 0.7rem;
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
  .nav {
    font-size: 1.2rem;
    line-height: 1;
    padding: 2px 10px;
  }
  .nav:disabled {
    opacity: 0.25;
    cursor: default;
  }
  .meta {
    flex: 1;
    text-align: center;
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
    font-size: 1.1rem;
  }
  .box p {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .box .row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }
  .box .row span:last-child {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    justify-content: space-between;
    margin-top: 6px;
  }
  .primary {
    background: var(--accent);
    color: #fff;
    padding: 9px;
    font-weight: 600;
  }
  .danger {
    color: var(--danger);
  }
  .themes {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .swatch {
    width: 26px;
    height: 20px;
    padding: 0;
    border: 1px solid;
    border-radius: var(--radius);
    display: grid;
    place-items: end;
  }
  .swatch span {
    display: block;
    width: 100%;
    height: 5px;
  }
  .swatch.sel {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
  }
  .hint {
    font-size: 0.78rem;
  }
  .toast {
    position: fixed;
    bottom: calc(52px + env(safe-area-inset-bottom));
    left: 50%;
    transform: translateX(-50%);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 8px 12px;
    font-size: 0.82rem;
    max-width: 90vw;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
  }
</style>
