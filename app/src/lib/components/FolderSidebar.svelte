<script lang="ts">
  // Folders sidebar: All Notes, the folder tree, Recently Deleted (Joplin's
  // trash) and a footer with New Folder, the sync status and Settings. Notes
  // and folders can be dropped onto folders or onto Recently Deleted; folders
  // are renamed inline and cut / copied / pasted (⌘X ⌘C ⌘V or the menu).
  // Rows are divs, not buttons: WebKit does not start drags from buttons.
  import { api, type Folder, type NoteSummary, when } from "../api";
  import { i18n, t, MOD } from "../i18n.svelte";
  import { icons } from "../icons";
  import { DRAG_TYPE, treeClipboard, type TreeKind } from "../treeClipboard.svelte";

  let {
    folders,
    notes,
    rootId,
    noteHome,
    selection,
    trashCount,
    syncState,
    syncMessage,
    lastSync,
    local,
    lights = false,
    onSelect,
    onNewNote,
    onTrashNote,
    onHomeChanged,
    onChanged,
    onSync,
    onSettings,
  }: {
    folders: Folder[];
    /** Every note of the working tree (for the counts). */
    notes: NoteSummary[];
    /** Top of the tree: the working notebook, or "" for all of Joplin. */
    rootId: string;
    /** Notebook for new notes and for notes that would land at Joplin's top level. */
    noteHome: string;
    /** "all", "trash" or a folder id. */
    selection: string;
    trashCount: number;
    syncState: "idle" | "syncing" | "error";
    syncMessage: string;
    lastSync: number;
    /** No Joplin Server: no sync line. */
    local: boolean;
    /** macOS: leave room for the traffic lights. */
    lights?: boolean;
    onSelect: (selection: string) => void;
    onNewNote: (folderId: string) => void;
    onTrashNote: (id: string) => void;
    onHomeChanged: () => void;
    onChanged: () => void;
    onSync: () => void;
    onSettings: () => void;
  } = $props();

  interface Row {
    id: string;
    depth: number;
  }

  const EXPANDED_KEY = "omanote.expanded";

  let expanded = $state<Set<string>>(loadExpanded());
  let adding = $state<string | null>(null); // parent id of the folder being created
  let newName = $state("");
  let renaming = $state<string | null>(null);
  let renameName = $state("");
  let dropTarget = $state<string | null>(null);
  let menu = $state<{ x: number; y: number; id: string } | null>(null);
  let tree: HTMLElement | undefined = $state();

  const clip = $derived(treeClipboard.clip);
  const childFolders = (parent: string) =>
    folders.filter((f) => f.parent_id === parent).sort((a, b) => a.title.localeCompare(b.title));
  const folderOf = (id: string) => folders.find((f) => f.id === id);
  const directCount = $derived.by(() => {
    const m = new Map<string, number>();
    for (const n of notes) m.set(n.parent_id, (m.get(n.parent_id) ?? 0) + 1);
    return m;
  });

  /** Visible folder rows: the working notebook first (it holds notes too), then its folders. */
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const walk = (parent: string, depth: number) => {
      for (const f of childFolders(parent)) {
        out.push({ id: f.id, depth });
        if (isOpen(f.id)) walk(f.id, depth + 1);
      }
    };
    if (rootId) {
      out.push({ id: rootId, depth: 0 });
      if (isOpen(rootId) || !folderOf(rootId)) walk(rootId, 1);
    } else walk("", 0);
    return out;
  });

  // Reveal the selected folder: expand every folder above it.
  $effect(() => {
    const sel = selection;
    if (sel === "all" || sel === "trash") return;
    const next = new Set(expanded);
    let p = folderOf(sel)?.parent_id ?? "";
    let changed = false;
    while (p) {
      if (p === rootId ? next.has(`!${p}`) : !next.has(p)) {
        if (p === rootId) next.delete(`!${p}`);
        else next.add(p);
        changed = true;
      }
      p = folderOf(p)?.parent_id ?? "";
    }
    if (changed) setExpanded(next);
  });

  function loadExpanded(): Set<string> {
    try {
      return new Set(JSON.parse(localStorage.getItem(EXPANDED_KEY) ?? "[]"));
    } catch {
      return new Set();
    }
  }

  function setExpanded(next: Set<string>) {
    expanded = next;
    try {
      localStorage.setItem(EXPANDED_KEY, JSON.stringify([...next]));
    } catch {
      /* ignore */
    }
  }

  // The working notebook starts open (its folders are the point of the
  // sidebar); it is remembered as "!<id>" once collapsed.
  const isOpen = (id: string) => (id === rootId ? !expanded.has(`!${id}`) : expanded.has(id));

  function toggle(id: string, open?: boolean) {
    const next = new Set(expanded);
    const want = open ?? !isOpen(id);
    if (id === rootId) {
      if (want) next.delete(`!${id}`);
      else next.add(`!${id}`);
    } else if (want) next.add(id);
    else next.delete(id);
    setExpanded(next);
  }

  /** True if `folderId` is `ancestorId` or lies inside it. */
  function isInside(folderId: string, ancestorId: string): boolean {
    let p: string | undefined = folderId;
    while (p) {
      if (p === ancestorId) return true;
      p = folderOf(p)?.parent_id;
    }
    return false;
  }

  // ---------------------------------------------------------------- actions

  async function move(kind: TreeKind, id: string, dest: string) {
    // Joplin notes always live in a notebook: the top level sends them home.
    if (kind === "note" && dest === "") dest = noteHome;
    if (kind === "note") {
      if (notes.find((n) => n.id === id)?.parent_id === dest) return;
      await api.moveNote(id, dest);
    } else {
      if (isInside(dest, id) || folderOf(id)?.parent_id === dest) return;
      await api.moveFolder(id, dest);
    }
    if (dest) toggle(dest, true);
    onChanged();
  }

  async function copyFolder(id: string, dest: string, rename: boolean) {
    const src = folderOf(id);
    if (!src) return;
    const copy = await api.createFolder(rename ? `${src.title} ${t("ctx.copySuffix")}` : src.title, dest);
    for (const n of notes.filter((x) => x.parent_id === id)) {
      const full = await api.note(n.id);
      if (full) await api.createNote(copy.id, full.text);
    }
    for (const f of childFolders(id)) await copyFolder(f.id, copy.id, false);
  }

  async function paste(dest: string) {
    const c = treeClipboard.clip;
    if (!c) return;
    if (c.mode === "cut") {
      await move(c.kind, c.id, dest);
      treeClipboard.clip = null;
      return;
    }
    if (c.kind === "note") {
      const home = dest || noteHome;
      const full = await api.note(c.id);
      if (!full) return;
      const same = full.parent_id === home;
      const [first, ...rest] = full.text.split("\n");
      await api.createNote(home, same ? [`${first} ${t("ctx.copySuffix")}`, ...rest].join("\n") : full.text);
    } else {
      if (isInside(dest, c.id)) return;
      await copyFolder(c.id, dest, folderOf(c.id)?.parent_id === dest);
    }
    if (dest) toggle(dest, true);
    onChanged();
  }

  async function trashFolder(id: string) {
    const f = folderOf(id);
    if (!f || id === rootId) return;
    if (!confirm(t("nav.trashFolderConfirm", { name: f.title }))) return;
    await api.trashFolder(id);
    if (isInside(selection, id)) onSelect("all");
    onChanged();
  }

  async function addFolder() {
    const parent = adding;
    const name = newName.trim();
    adding = null;
    newName = "";
    // parent "" is Joplin's top level (a new notebook): only null means cancelled.
    if (parent === null || !name) return;
    const f = await api.createFolder(name, parent);
    if (parent) toggle(parent, true);
    onChanged();
    onSelect(f.id);
  }

  async function commitRename() {
    const id = renaming;
    renaming = null;
    if (id && renameName.trim()) {
      await api.renameFolder(id, renameName.trim());
      onChanged();
    }
    tree?.focus();
  }

  function startNewFolder(parent: string) {
    adding = parent;
    newName = "";
    if (parent) toggle(parent, true);
  }

  async function useForNewNotes(id: string) {
    await api.setNotesHome(id);
    onHomeChanged();
  }

  // ------------------------------------------------------------ drag & drop

  function dragStart(e: DragEvent, id: string) {
    if (!e.dataTransfer || id === rootId) return;
    e.dataTransfer.setData(DRAG_TYPE, JSON.stringify({ kind: "folder", id }));
    e.dataTransfer.effectAllowed = "move";
  }

  function dragOver(e: DragEvent, target: string) {
    if (!e.dataTransfer?.types.includes(DRAG_TYPE)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    dropTarget = target;
  }

  async function drop(e: DragEvent, target: string) {
    const data = e.dataTransfer?.getData(DRAG_TYPE);
    dropTarget = null;
    if (!data) return;
    e.preventDefault();
    e.stopPropagation();
    const item = JSON.parse(data) as { kind: TreeKind; id: string };
    if (target === "trash") {
      if (item.kind === "note") onTrashNote(item.id);
      else await trashFolder(item.id);
    } else await move(item.kind, item.id, target);
  }

  // --------------------------------------------------------------- keyboard

  const order = $derived(["all", ...rows.map((r) => r.id), "trash"]);

  function key(e: KeyboardEvent) {
    if (renaming || adding !== null) return;
    const mod = MOD === "⌘" ? e.metaKey : e.ctrlKey;
    const k = e.key.toLowerCase();
    const folder = selection !== "all" && selection !== "trash" ? selection : null;
    let handled = true;
    if (k === "arrowdown" || k === "arrowup") {
      const i = order.indexOf(selection);
      const next = order[Math.max(0, Math.min(order.length - 1, i + (k === "arrowdown" ? 1 : -1)))];
      onSelect(next);
      tree?.querySelector(`[data-id="${next}"]`)?.scrollIntoView({ block: "nearest" });
    } else if ((k === "arrowright" || k === "arrowleft") && folder) toggle(folder, k === "arrowright");
    else if (mod && k === "x" && folder && folder !== rootId) treeClipboard.clip = { mode: "cut", kind: "folder", id: folder };
    else if (mod && k === "c" && folder && folder !== rootId) treeClipboard.clip = { mode: "copy", kind: "folder", id: folder };
    else if (mod && k === "v" && folder) void paste(folder);
    else if ((k === "delete" || k === "backspace") && folder) void trashFolder(folder);
    else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  function openMenu(e: MouseEvent, id: string) {
    e.preventDefault();
    menu = { x: Math.min(e.clientX, window.innerWidth - 240), y: Math.min(e.clientY, window.innerHeight - 300), id };
  }

  /** Runs a menu entry on its folder, captured before the menu closes. */
  function menuAction(fn: (id: string) => void) {
    const id = menu?.id;
    menu = null;
    if (id) fn(id);
  }

  const syncLine = $derived(
    syncState === "syncing"
      ? t("sync.syncing")
      : syncState === "error"
        ? t("sync.error")
        : lastSync
          ? t("sync.synced", { when: when(lastSync, i18n.lang) })
          : t("sync.never"),
  );
</script>

<aside class="sidebar" class:lights>
  <div class="head" data-tauri-drag-region>
    <strong>{t("nav.folders")}</strong>
  </div>

  <div class="tree" role="tree" tabindex="0" bind:this={tree} onkeydown={key}>
    <div
      class="row special"
      class:sel={selection === "all"}
      data-id="all"
      role="treeitem"
      aria-selected={selection === "all"}
      tabindex="-1"
      onclick={() => onSelect("all")}
      onkeydown={() => {}}
    >
      <span class="glyph">{@html icons.allNotes}</span>
      <span class="label">{t("nav.allNotes")}</span>
      <span class="count">{notes.length}</span>
    </div>

    <div class="section">{t("nav.folders")}</div>

    {#each rows as r (r.id)}
      {@const f = folderOf(r.id)}
      {@const hasKids = childFolders(r.id).length > 0}
      {@const isSel = selection === r.id}
      <div
        class="row"
        class:sel={isSel}
        class:cut={clip?.mode === "cut" && clip.id === r.id}
        class:drop={dropTarget === r.id}
        style="--depth: {r.depth}"
        data-id={r.id}
        role="treeitem"
        aria-selected={isSel}
        aria-expanded={hasKids ? isOpen(r.id) : undefined}
        tabindex="-1"
        draggable={r.id !== rootId && renaming !== r.id}
        ondragstart={(e) => dragStart(e, r.id)}
        ondragover={(e) => dragOver(e, r.id)}
        ondragleave={() => (dropTarget = null)}
        ondrop={(e) => drop(e, r.id)}
        onclick={() => onSelect(r.id)}
        oncontextmenu={(e) => openMenu(e, r.id)}
        onkeydown={() => {}}
      >
        {#if hasKids}
          <button
            class="twisty"
            aria-label={isOpen(r.id) ? "−" : "+"}
            onclick={(e) => (e.stopPropagation(), toggle(r.id))}>{@html isOpen(r.id) ? icons.collapse : icons.expand}</button
          >
        {:else}
          <span class="twisty"></span>
        {/if}
        {#if renaming === r.id}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            autofocus
            bind:value={renameName}
            onclick={(e) => e.stopPropagation()}
            onblur={commitRename}
            onkeydown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") void commitRename();
              else if (e.key === "Escape") renaming = null;
            }}
          />
        {:else}
          <span class="glyph">{#if f?.icon}{f.icon}{:else}{@html icons.folder}{/if}</span>
          <span class="label"
            >{f?.title ?? "Omanote"}{#if !rootId && r.id === noteHome}<span class="home">{t("set.newNotesHere")}</span>{/if}</span
          >
          <span class="count">{directCount.get(r.id) ?? 0}</span>
          <button class="more" title="⋯" onclick={(e) => (e.stopPropagation(), openMenu(e, r.id))}>{@html icons.more}</button>
        {/if}
      </div>
      {#if adding === r.id}
        <div class="row" style="--depth: {r.depth + 1}">
          <span class="twisty"></span>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            autofocus
            placeholder={t("nav.folderName")}
            bind:value={newName}
            onblur={addFolder}
            onkeydown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") void addFolder();
              else if (e.key === "Escape") adding = null;
            }}
          />
        </div>
      {/if}
    {/each}
    {#if adding === ""}
      <div class="row" style="--depth: 0">
        <span class="twisty"></span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          autofocus
          placeholder={t("nav.folderName")}
          bind:value={newName}
          onblur={addFolder}
          onkeydown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") void addFolder();
            else if (e.key === "Escape") adding = null;
          }}
        />
      </div>
    {/if}

    <div
      class="row special trash"
      class:sel={selection === "trash"}
      class:drop={dropTarget === "trash"}
      data-id="trash"
      role="treeitem"
      aria-selected={selection === "trash"}
      tabindex="-1"
      ondragover={(e) => dragOver(e, "trash")}
      ondragleave={() => (dropTarget = null)}
      ondrop={(e) => drop(e, "trash")}
      onclick={() => onSelect("trash")}
      onkeydown={() => {}}
    >
      <span class="glyph">{@html icons.trash}</span>
      <span class="label">{t("nav.recentlyDeleted")}</span>
      <span class="count">{trashCount}</span>
    </div>
  </div>

  <footer>
    <button class="newfolder" onclick={() => startNewFolder(rootId)}>{@html icons.folderAdd} {t("nav.newFolder")}</button>
    <div class="status">
      {#if !local}
        <button
          class="sync"
          class:syncing={syncState === "syncing"}
          class:error={syncState === "error"}
          title={syncState === "error" ? syncMessage : t("act.sync")}
          onclick={onSync}
          >{@html syncState === "error" ? icons.cloudOff : syncState === "syncing" ? icons.sync : icons.cloudOk}
          <span>{syncLine}</span></button
        >
      {:else}
        <span class="sync local">{t("sync.local")}</span>
      {/if}
      <button class="gear" title={t("nav.settings")} aria-label={t("nav.settings")} onclick={onSettings}>{@html icons.settings}</button>
    </div>
  </footer>
</aside>

{#if menu}
  {@const m = menu}
  {@const isRoot = m.id === rootId}
  <div class="menu-scrim" onclick={() => (menu = null)} oncontextmenu={(e) => (e.preventDefault(), (menu = null))} role="presentation"></div>
  <div class="menu" style="left: {m.x}px; top: {m.y}px" role="menu">
    <button role="menuitem" onclick={() => menuAction((id) => onNewNote(id))}>{@html icons.compose} {t("ctx.newNote")}</button>
    <button role="menuitem" onclick={() => menuAction((id) => startNewFolder(id))}>{@html icons.folderAdd} {t("ctx.newFolder")}</button>
    {#if !rootId && m.id !== noteHome}
      <button role="menuitem" onclick={() => menuAction((id) => void useForNewNotes(id))}>{@html icons.note} {t("ctx.useForNewNotes")}</button>
    {/if}
    <hr />
    {#if !isRoot}
      <button role="menuitem" onclick={() => menuAction((id) => ((renaming = id), (renameName = folderOf(id)?.title ?? "")))}
        >{t("ctx.rename")}</button
      >
      <button role="menuitem" onclick={() => menuAction((id) => (treeClipboard.clip = { mode: "cut", kind: "folder", id }))}
        >{t("ctx.cut")} <kbd>{MOD}X</kbd></button
      >
      <button role="menuitem" onclick={() => menuAction((id) => (treeClipboard.clip = { mode: "copy", kind: "folder", id }))}
        >{t("ctx.copy")} <kbd>{MOD}C</kbd></button
      >
    {/if}
    <button role="menuitem" disabled={!clip} onclick={() => menuAction((id) => void paste(id))}>{t("ctx.paste")} <kbd>{MOD}V</kbd></button>
    {#if !isRoot}
      <hr />
      <button role="menuitem" class="danger" onclick={() => menuAction((id) => void trashFolder(id))}>{@html icons.trash} {t("act.deleteShort")}</button>
    {/if}
  </div>
{/if}

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--bg-elev);
    border-right: 1px solid var(--line);
    padding-top: env(safe-area-inset-top);
  }
  .head {
    display: flex;
    align-items: center;
    min-height: var(--bar-h);
    padding: 0 14px;
    color: var(--accent);
  }
  /* macOS: the traffic lights sit at the top left (no title bar). */
  .lights .head {
    padding-left: 84px;
  }
  .tree {
    flex: 1;
    overflow-y: auto;
    outline: none;
    padding: 4px 6px;
  }
  .section {
    margin: 12px 8px 4px;
    font-size: 0.72rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 34px;
    padding: 0 6px 0 calc(var(--depth, 0) * 14px + 4px);
    border-left: 2px solid transparent;
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
  }
  .row:hover {
    background: color-mix(in srgb, var(--panel) 60%, transparent);
  }
  .row.sel {
    background: var(--panel);
    border-left-color: var(--accent);
  }
  .row.sel .label,
  .row.sel .glyph {
    color: var(--accent);
  }
  .row.cut {
    opacity: 0.45;
  }
  .row.drop {
    background: var(--accent-soft);
    outline: 1px dashed var(--accent);
    outline-offset: -1px;
  }
  .row.special {
    padding-left: 8px;
  }
  .trash {
    margin-top: 12px;
  }
  .row input {
    flex: 1;
    margin: 2px 4px;
    padding: 4px 8px;
  }
  .twisty {
    width: 18px;
    flex: none;
    padding: 0;
    color: var(--muted);
    font-size: var(--ico-s, 15px);
    text-align: center;
  }
  .glyph {
    width: 20px;
    flex: none;
    text-align: center;
    color: var(--muted);
    font-size: var(--ico-s, 15px);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .home {
    margin-left: 8px;
    padding: 0 5px;
    font-size: 0.68rem;
    color: var(--accent);
    border: 1px solid var(--accent);
    vertical-align: 1px;
  }
  .count {
    color: var(--muted);
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
  }
  .more {
    display: none;
    padding: 2px 4px;
    color: var(--muted);
    font-size: var(--ico-s, 15px);
  }
  .row:hover .more,
  .row.sel .more {
    display: block;
  }
  .row:hover .count,
  .row.sel .count {
    display: none;
  }
  @media (hover: none) {
    .more {
      display: block;
    }
    .row .count {
      display: none;
    }
    .row.special .count {
      display: inline;
    }
  }
  footer {
    border-top: 1px solid var(--line);
    padding: 6px 8px calc(8px + env(safe-area-inset-bottom));
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .newfolder {
    text-align: left;
    color: var(--muted);
    padding: 6px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .newfolder:hover {
    color: var(--accent);
  }
  .status {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .sync {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    text-align: left;
    color: var(--muted);
    font-size: 0.78rem;
  }
  .sync span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sync.error {
    color: var(--danger);
  }
  .sync.syncing :global(.ico) {
    animation: spin 1.2s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .gear {
    color: var(--muted);
    font-size: var(--ico-l, 22px);
    padding: 4px 8px;
  }
  .gear:hover {
    color: var(--accent);
  }
  .menu-scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: fixed;
    z-index: 41;
    min-width: 14rem;
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
