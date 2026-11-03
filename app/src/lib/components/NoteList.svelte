<script lang="ts">
  // The notes of the selected folder (or All Notes, or Recently Deleted):
  // search, sorting, date sections, a context menu (right-click / long press),
  // swipe actions on touch screens, keyboard navigation, and rows that can be
  // dragged onto a sidebar folder.
  import { type Folder, type NoteSummary, type TrashItem, when } from "../api";
  import { i18n, t, MOD } from "../i18n.svelte";
  import { icons } from "../icons";
  import { prefs, type Sort } from "../prefs.svelte";
  import { rowDate, sectionKey, sectionOf, type Section } from "../dates";
  import { DRAG_TYPE, treeClipboard } from "../treeClipboard.svelte";

  let {
    title,
    trashMode = false,
    notes,
    trash = [],
    folders,
    showFolder,
    currentId,
    query = $bindable(""),
    searchAll = $bindable(false),
    scopeName = "",
    back = null,
    sidebarButton = false,
    lights = false,
    onToggleSidebar,
    onBack,
    onOpen,
    onNew,
    onMove,
    onDuplicate,
    onDelete,
    onRestore,
    onPurge,
    onEmptyTrash,
  }: {
    title: string;
    trashMode?: boolean;
    notes: NoteSummary[];
    trash?: TrashItem[];
    folders: Folder[];
    /** Show each note's folder (All Notes, search results). */
    showFolder: boolean;
    currentId: string | null;
    query?: string;
    /** Search every note instead of only this folder. */
    searchAll?: boolean;
    /** Folder name for the search scope switch ("" = no switch). */
    scopeName?: string;
    /** Stack layout: the back button's label. */
    back?: string | null;
    sidebarButton?: boolean;
    lights?: boolean;
    onToggleSidebar: () => void;
    onBack: () => void;
    /** `focus`: move the keyboard to the note (Enter, click). */
    onOpen: (id: string, focus: boolean) => void;
    onNew: () => void;
    onMove: (id: string) => void;
    onDuplicate: (id: string) => void;
    onDelete: (id: string) => void;
    onRestore: (id: string) => void;
    onPurge: (id: string) => void;
    onEmptyTrash: () => void;
  } = $props();

  interface Item {
    id: string;
    title: string;
    preview: string;
    time: number;
    folder: string;
    encrypted: boolean;
    conflict: boolean;
    isFolder: boolean;
  }

  let list: HTMLElement | undefined = $state();
  let menu = $state<{ x: number; y: number; id: string } | null>(null);
  let sortMenu = $state(false);
  /** Touch swipe: the row showing its actions and how far it is pulled. */
  let swiped = $state<{ id: string; dx: number } | null>(null);

  const folderName = (id: string) => folders.find((f) => f.id === id)?.title ?? "";

  const items = $derived.by((): Item[] => {
    if (trashMode) {
      return trash.map((x) => ({
        id: x.id,
        title: x.title,
        preview: "",
        time: x.deleted_time,
        folder: x.is_folder ? "" : folderName(x.parent_id),
        encrypted: x.encrypted,
        conflict: false,
        isFolder: x.is_folder,
      }));
    }
    const out = notes.map((n) => ({
      id: n.id,
      title: n.title,
      preview: n.preview.replace(/\s+/g, " ").trim(),
      time: n.updated_time,
      folder: folderName(n.parent_id),
      encrypted: n.encrypted,
      conflict: n.is_conflict,
      isFolder: false,
    }));
    if (prefs.sort === "title") out.sort((a, b) => (a.title || "~").localeCompare(b.title || "~", i18n.lang));
    else out.sort((a, b) => b.time - a.time);
    return out;
  });

  /** Rows grouped by date (only when sorted by date and not searching). */
  const groups = $derived.by(() => {
    if (prefs.sort === "title" || query.trim() || trashMode) return [{ key: "", section: null as Section | null, items }];
    const out: { key: string; section: Section | null; items: Item[] }[] = [];
    for (const it of items) {
      const s = sectionOf(it.time, i18n.lang);
      const k = sectionKey(s);
      if (out.at(-1)?.key !== k) out.push({ key: k, section: s, items: [] });
      out.at(-1)!.items.push(it);
    }
    return out;
  });

  function sectionLabel(s: Section): string {
    if (s.kind === "older") return s.label;
    return t(`date.${s.kind}` as "date.today");
  }

  function itemTitle(it: Item): string {
    if (it.encrypted) return t("note.encrypted");
    return it.title || t("note.untitled");
  }

  function setSort(s: Sort) {
    prefs.sort = s;
    sortMenu = false;
  }

  // -------------------------------------------------------------- keyboard

  function key(e: KeyboardEvent) {
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    const mod = MOD === "⌘" ? e.metaKey : e.ctrlKey;
    const k = e.key.toLowerCase();
    const i = items.findIndex((x) => x.id === currentId);
    let handled = true;
    if (k === "arrowdown" || k === "arrowup") {
      const next = items[Math.max(0, Math.min(items.length - 1, i + (k === "arrowdown" ? 1 : -1)))];
      if (next) {
        onOpen(next.id, false);
        list?.querySelector(`[data-id="${next.id}"]`)?.scrollIntoView({ block: "nearest" });
      }
    } else if (k === "enter" && currentId) onOpen(currentId, true);
    else if ((k === "delete" || k === "backspace") && currentId) (trashMode ? onPurge : onDelete)(currentId);
    else if (mod && k === "x" && currentId && !trashMode) treeClipboard.clip = { mode: "cut", kind: "note", id: currentId };
    else if (mod && k === "c" && currentId && !trashMode) treeClipboard.clip = { mode: "copy", kind: "note", id: currentId };
    else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  // ------------------------------------------------------------ menus

  function openMenu(x: number, y: number, id: string) {
    swiped = null;
    menu = { x: Math.min(x, window.innerWidth - 240), y: Math.min(y, window.innerHeight - 260), id };
  }

  /** Runs a menu entry on its row, captured before the menu closes. */
  function menuAction(fn: (id: string) => void) {
    const id = menu?.id;
    menu = null;
    if (id) fn(id);
  }

  // ------------------------------------------------------------ touch

  // A finger dragging a row left reveals Move and Delete (Restore / Delete in
  // the trash); holding a finger still opens the context menu.
  const ACTIONS_W = 150;
  let touchStart: { id: string; x: number; y: number; base: number; horizontal: boolean | null } | null = null;
  let pressTimer: ReturnType<typeof setTimeout> | undefined;
  let longPressed = false;

  function pointerDown(e: PointerEvent, id: string) {
    if (e.pointerType === "mouse") return;
    longPressed = false;
    const base = swiped?.id === id ? swiped.dx : 0;
    touchStart = { id, x: e.clientX, y: e.clientY, base, horizontal: null };
    clearTimeout(pressTimer);
    pressTimer = setTimeout(() => {
      longPressed = true;
      touchStart = null;
      openMenu(e.clientX, e.clientY, id);
    }, 500);
  }

  function pointerMove(e: PointerEvent) {
    const s = touchStart;
    if (!s) return;
    const dx = e.clientX - s.x;
    const dy = e.clientY - s.y;
    if (Math.abs(dx) > 8 || Math.abs(dy) > 8) clearTimeout(pressTimer);
    if (s.horizontal === null && (Math.abs(dx) > 10 || Math.abs(dy) > 10)) s.horizontal = Math.abs(dx) > Math.abs(dy);
    if (!s.horizontal) return;
    swiped = { id: s.id, dx: Math.max(-ACTIONS_W - 40, Math.min(0, s.base + dx)) };
  }

  function pointerUp() {
    clearTimeout(pressTimer);
    const s = touchStart;
    touchStart = null;
    if (!s?.horizontal || !swiped) return;
    swiped = swiped.dx < -ACTIONS_W / 2 ? { id: swiped.id, dx: -ACTIONS_W } : null;
  }

  function rowClick(id: string) {
    if (longPressed) return;
    if (swiped) {
      swiped = null;
      return;
    }
    onOpen(id, true);
  }

  function dragStart(e: DragEvent, id: string) {
    if (!e.dataTransfer || trashMode) return;
    e.dataTransfer.setData(DRAG_TYPE, JSON.stringify({ kind: "note", id }));
    e.dataTransfer.effectAllowed = "move";
  }
</script>

<section class="notelist" class:lights>
  <header data-tauri-drag-region>
    {#if back !== null}
      <button class="back" onclick={onBack}>{@html icons.prev}<span>{back}</span></button>
    {:else if sidebarButton}
      <button class="icon" title={t("act.sidebar")} aria-label={t("act.sidebar")} onclick={onToggleSidebar}>{@html icons.sidebar}</button>
    {/if}
    <span class="spacer" data-tauri-drag-region></span>
    {#if trashMode}
      <button class="text danger" disabled={!trash.length} onclick={onEmptyTrash}>{t("trash.empty")}</button>
    {:else}
      <span class="anchor">
        <button class="icon" title={t("list.sortBy")} aria-label={t("list.sortBy")} onclick={() => (sortMenu = !sortMenu)}
          >{@html icons.sort}</button
        >
        {#if sortMenu}
          <div class="menu-scrim" onclick={() => (sortMenu = false)} role="presentation"></div>
          <div class="menu sortmenu" role="menu">
            <div class="menu-title">{t("list.sortBy")}</div>
            <button role="menuitemradio" aria-checked={prefs.sort === "edited"} onclick={() => setSort("edited")}
              ><span class="tick">{#if prefs.sort === "edited"}{@html icons.check}{/if}</span>{t("list.sortEdited")}</button
            >
            <button role="menuitemradio" aria-checked={prefs.sort === "title"} onclick={() => setSort("title")}
              ><span class="tick">{#if prefs.sort === "title"}{@html icons.check}{/if}</span>{t("list.sortTitle")}</button
            >
          </div>
        {/if}
      </span>
      <button class="icon" title={t("act.newNote")} aria-label={t("act.newNote")} onclick={onNew}>{@html icons.compose}</button>
    {/if}
  </header>

  <div class="titlebar">
    <h1>{title}</h1>
    <span class="count">{t(items.length === 1 ? "list.countOne" : "list.count", { n: items.length })}</span>
  </div>

  {#if !trashMode}
    <div class="search">
      <span class="ico-s">{@html icons.search}</span>
      <input
        type="search"
        placeholder={t("list.search")}
        bind:value={query}
        autocapitalize="off"
        autocorrect="off"
        spellcheck="false"
        data-search
        onkeydown={(e) => {
          if (e.key === "Escape") {
            query = "";
            (e.currentTarget as HTMLInputElement).blur();
          } else if (e.key === "ArrowDown" && items.length) {
            e.preventDefault();
            onOpen(items[0].id, false);
            list?.focus();
          }
        }}
      />
      {#if query}
        <button class="clear" aria-label={t("act.close")} onclick={() => (query = "")}>{@html icons.close}</button>
      {/if}
    </div>
    {#if query && scopeName}
      <div class="scope" role="radiogroup">
        <button role="radio" aria-checked={searchAll} class:on={searchAll} onclick={() => (searchAll = true)}>{t("nav.allNotes")}</button>
        <button role="radio" aria-checked={!searchAll} class:on={!searchAll} onclick={() => (searchAll = false)}>{scopeName}</button>
      </div>
    {/if}
  {/if}

  <div class="rows" role="listbox" tabindex="0" bind:this={list} onkeydown={key} aria-label={title}>
    {#if !items.length}
      <div class="empty">
        {#if query.trim()}
          <p>{t("list.noResults", { q: query.trim() })}</p>
        {:else if trashMode}
          <p>{t("trash.none")}</p>
        {:else}
          <p class="big">{t("list.empty")}</p>
          <button class="text" onclick={onNew}>{@html icons.compose} {t("act.newNote")}</button>
        {/if}
      </div>
    {/if}
    {#each groups as g (g.key)}
      {#if g.section}
        <div class="section">{sectionLabel(g.section)}</div>
      {/if}
      {#each g.items as it (it.id)}
        {@const sel = it.id === currentId}
        {@const dx = swiped?.id === it.id ? swiped.dx : 0}
        <div class="swipe">
          <div class="actions" class:open={dx !== 0} aria-hidden={dx === 0}>
            {#if trashMode}
              <button class="act restore" onclick={() => ((swiped = null), onRestore(it.id))}>{@html icons.restore}<span>{t("trash.restore")}</span></button>
              <button class="act delete" onclick={() => ((swiped = null), onPurge(it.id))}>{@html icons.trash}<span>{t("trash.purge")}</span></button>
            {:else}
              <button class="act move" onclick={() => ((swiped = null), onMove(it.id))}>{@html icons.move}<span>{t("act.moveShort")}</span></button>
              <button class="act delete" onclick={() => ((swiped = null), onDelete(it.id))}>{@html icons.trash}<span>{t("act.deleteShort")}</span></button>
            {/if}
          </div>
          <div
            class="row"
            class:sel
            class:dragging={dx !== 0}
            style="transform: translateX({dx}px)"
            data-id={it.id}
            role="option"
            aria-selected={sel}
            tabindex="-1"
            draggable={!trashMode}
            ondragstart={(e) => dragStart(e, it.id)}
            onclick={() => rowClick(it.id)}
            oncontextmenu={(e) => (e.preventDefault(), openMenu(e.clientX, e.clientY, it.id))}
            onpointerdown={(e) => pointerDown(e, it.id)}
            onpointermove={pointerMove}
            onpointerup={pointerUp}
            onpointercancel={pointerUp}
            onkeydown={() => {}}
          >
            <div class="line1">
              {#if it.isFolder}<span class="ico-s">{@html icons.folder}</span>{/if}
              {#if it.encrypted}<span class="ico-s">{@html icons.lock}</span>{/if}
              <span class="title">{itemTitle(it)}</span>
              {#if it.conflict}<span class="ico-s warn" title={t("note.conflict")}>{@html icons.warn}</span>{/if}
            </div>
            <div class="line2">
              <span class="date">{trashMode ? when(it.time, i18n.lang) : rowDate(it.time, i18n.lang)}</span>
              {#if it.preview}<span class="preview">{it.preview}</span>{/if}
            </div>
            {#if (showFolder || trashMode) && it.folder}
              <div class="line3">{@html icons.folder} {it.folder}</div>
            {/if}
          </div>
        </div>
      {/each}
    {/each}
  </div>
</section>

{#if menu}
  {@const m = menu}
  <div class="menu-scrim" onclick={() => (menu = null)} oncontextmenu={(e) => (e.preventDefault(), (menu = null))} role="presentation"></div>
  <div class="menu ctx" style="left: {m.x}px; top: {m.y}px" role="menu">
    {#if trashMode}
      <button role="menuitem" onclick={() => menuAction(onRestore)}>{@html icons.restore} {t("trash.restore")}</button>
      <hr />
      <button role="menuitem" class="danger" onclick={() => menuAction(onPurge)}>{@html icons.trash} {t("trash.purge")}</button>
    {:else}
      <button role="menuitem" onclick={() => menuAction((id) => onOpen(id, true))}>{@html icons.note} {t("ctx.open")}</button>
      <button role="menuitem" onclick={() => menuAction(onMove)}>{@html icons.move} {t("act.move")}</button>
      <button role="menuitem" onclick={() => menuAction(onDuplicate)}>{@html icons.duplicate} {t("act.duplicate")}</button>
      <hr />
      <button role="menuitem" onclick={() => menuAction((id) => (treeClipboard.clip = { mode: "cut", kind: "note", id }))}
        >{t("ctx.cut")} <kbd>{MOD}X</kbd></button
      >
      <button role="menuitem" onclick={() => menuAction((id) => (treeClipboard.clip = { mode: "copy", kind: "note", id }))}
        >{t("ctx.copy")} <kbd>{MOD}C</kbd></button
      >
      <hr />
      <button role="menuitem" class="danger" onclick={() => menuAction(onDelete)}>{@html icons.trash} {t("act.deleteShort")}</button>
    {/if}
  </div>
{/if}

<style>
  .notelist {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--bg);
    border-right: 1px solid var(--line);
    padding-top: env(safe-area-inset-top);
  }
  header {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: var(--bar-h);
    padding: 0 6px;
  }
  .lights header {
    padding-left: 80px;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .icon {
    font-size: var(--ico-l, 22px);
    line-height: 1;
    min-width: 44px;
    min-height: 44px;
    color: var(--muted);
  }
  .icon:hover {
    color: var(--accent);
  }
  .back {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: 44px;
    padding-right: 8px;
    color: var(--accent);
    max-width: 60%;
  }
  .back :global(.ico) {
    font-size: var(--ico-l, 22px);
  }
  .back span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text {
    padding: 6px 10px;
    color: var(--accent);
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .text.danger {
    color: var(--danger);
  }
  .text:disabled {
    opacity: 0.35;
  }
  .anchor {
    position: relative;
  }
  .titlebar {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 0 16px 8px;
  }
  h1 {
    margin: 0;
    font-size: 1.35rem;
    color: var(--accent);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    color: var(--muted);
    font-size: 0.8rem;
    white-space: nowrap;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 12px 8px;
    padding: 0 8px;
    border: 1px solid var(--line);
    background: var(--bg-elev);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    padding: 7px 0;
    outline: none;
  }
  .search input::-webkit-search-cancel-button {
    display: none;
  }
  .ico-s {
    color: var(--muted);
    font-size: var(--ico-s, 15px);
    flex: none;
  }
  .clear {
    color: var(--muted);
    font-size: var(--ico-s, 15px);
    padding: 4px;
  }
  .scope {
    display: flex;
    gap: 4px;
    margin: -2px 12px 8px;
  }
  .scope button {
    padding: 3px 9px;
    font-size: 0.78rem;
    border: 1px solid var(--line);
    color: var(--muted);
    max-width: 50%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .scope button.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .rows {
    flex: 1;
    overflow-y: auto;
    outline: none;
    padding-bottom: env(safe-area-inset-bottom);
  }
  .section {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 10px 16px 4px;
    background: var(--bg);
    font-size: 0.72rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .swipe {
    position: relative;
    overflow: hidden;
  }
  .actions {
    position: absolute;
    inset: 0 0 0 auto;
    width: 150px;
    display: flex;
    visibility: hidden;
  }
  .actions.open {
    visibility: visible;
  }
  .act {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    font-size: 0.72rem;
    color: var(--bg);
  }
  .act :global(.ico) {
    font-size: var(--ico-l, 22px);
  }
  .act.move,
  .act.restore {
    background: var(--accent);
  }
  .act.delete {
    background: var(--danger);
  }
  .row {
    position: relative;
    background: var(--bg);
    padding: 9px 16px 9px 14px;
    border-left: 2px solid transparent;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 60%, transparent);
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
    touch-action: pan-y;
    transition: transform 0.18s ease;
  }
  .row.dragging {
    transition: none;
  }
  .row:hover {
    background: color-mix(in srgb, var(--panel) 50%, var(--bg));
  }
  .row.sel {
    background: var(--panel);
    border-left-color: var(--accent);
  }
  .line1 {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .title {
    flex: 1;
    min-width: 0;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.sel .title {
    color: var(--accent);
  }
  .warn {
    color: var(--danger);
  }
  .line2 {
    display: flex;
    gap: 8px;
    margin-top: 3px;
    font-size: 0.8rem;
    color: var(--muted);
    min-width: 0;
  }
  .date {
    flex: none;
    color: var(--text);
    opacity: 0.75;
    font-variant-numeric: tabular-nums;
  }
  .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .line3 {
    margin-top: 3px;
    font-size: 0.74rem;
    color: var(--muted);
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .empty {
    padding: 40px 20px;
    text-align: center;
    color: var(--muted);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }
  .empty .big {
    font-size: 1.05rem;
    color: var(--text);
  }
  .menu-scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: fixed;
    z-index: 41;
    min-width: 13rem;
    display: flex;
    flex-direction: column;
    padding: 4px;
    background: var(--bg-elev);
    border: 1px solid var(--accent);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.35);
  }
  .sortmenu {
    position: absolute;
    top: 100%;
    right: 0;
  }
  .menu-title {
    padding: 6px 10px 4px;
    font-size: 0.72rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    padding: 7px 10px;
  }
  .menu button:hover {
    background: var(--panel);
  }
  .tick {
    width: 1.1em;
    color: var(--accent);
  }
  .menu kbd {
    margin-left: auto;
    font-family: var(--font);
    font-size: 0.75rem;
    color: var(--muted);
  }
  .menu .danger {
    color: var(--danger);
  }
  .menu hr {
    border: 0;
    border-top: 1px solid var(--line);
    margin: 4px 0;
  }
</style>
