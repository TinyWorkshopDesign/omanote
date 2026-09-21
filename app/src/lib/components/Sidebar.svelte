<script lang="ts">
  // Navigation panel: the working notebook as a tree (notebook → folders →
  // notes). Notes and folders can be dragged onto folders, cut / copied /
  // pasted (⌘X ⌘C ⌘V or right-click), renamed and trashed; arrows move the
  // selection. Rows are divs, not buttons: WebKit does not drag buttons.
  import { api, type Folder, type NoteSummary, when } from "../api";
  import { i18n, t, MOD } from "../i18n.svelte";
  import { icons } from "../icons";
  import { treeClipboard, type TreeKind } from "../treeClipboard.svelte";

  let {
    folders,
    rootId,
    currentId,
    scope,
    version,
    mac = false,
    onOpenNote,
    onSelectFolder,
    onNewNote,
    onDeleteNote,
    onChanged,
    onClose,
    onSettings,
  }: {
    folders: Folder[];
    rootId: string;
    currentId: string | null;
    /** Folder the note stack is limited to ("" = whole notebook). */
    scope: string;
    /** Bumped by the page whenever notes change, to reload the tree. */
    version: number;
    /** macOS: leave room for the traffic lights above the title. */
    mac?: boolean;
    onOpenNote: (id: string) => void;
    onSelectFolder: (id: string) => void;
    onNewNote: (folderId: string) => void;
    onDeleteNote: (id: string) => void;
    onChanged: () => void;
    onClose: () => void;
    onSettings: () => void;
  } = $props();

  type RowKind = TreeKind | "root";
  interface Row {
    kind: RowKind;
    id: string;
    depth: number;
  }
  interface Sel {
    kind: RowKind;
    id: string;
  }

  const EXPANDED_KEY = "omanote.expanded";
  const DRAG_TYPE = "text/omanote-item";

  let notes = $state<NoteSummary[]>([]);
  let expanded = $state<Set<string>>(loadExpanded());
  let selected = $state<Sel | null>(null);
  let adding = $state<string | null>(null); // parent id of the folder being created
  let newName = $state("");
  let renaming = $state<string | null>(null);
  let renameName = $state("");
  let dropTarget = $state<string | null>(null);
  let menu = $state<{ x: number; y: number; target: Sel } | null>(null);
  let tree: HTMLElement | undefined = $state();

  const root = $derived(folders.find((f) => f.id === rootId));
  const clip = $derived(treeClipboard.clip);

  const childFolders = (parent: string) =>
    folders.filter((f) => f.parent_id === parent).sort((a, b) => a.title.localeCompare(b.title));
  const notesIn = (parent: string) => notes.filter((n) => n.parent_id === parent);
  const folderOf = (id: string) => folders.find((f) => f.id === id);
  const noteOf = (id: string) => notes.find((n) => n.id === id);

  /** Visible rows, top to bottom (also drives keyboard navigation). */
  const rows = $derived.by(() => {
    const out: Row[] = [{ kind: "root", id: rootId, depth: 0 }];
    const walk = (parent: string, depth: number) => {
      for (const f of childFolders(parent)) {
        out.push({ kind: "folder", id: f.id, depth });
        if (expanded.has(f.id)) walk(f.id, depth + 1);
      }
      for (const n of notesIn(parent)) out.push({ kind: "note", id: n.id, depth });
    };
    walk(rootId, 1);
    return out;
  });

  $effect(() => {
    void version;
    api.notes().then((n) => (notes = n));
  });

  // Reveal the open note: expand every folder above it, select it.
  $effect(() => {
    const note = notes.find((n) => n.id === currentId);
    if (!note) return;
    const next = new Set(expanded);
    let p = note.parent_id;
    let changed = false;
    while (p && p !== rootId) {
      if (!next.has(p)) {
        next.add(p);
        changed = true;
      }
      p = folderOf(p)?.parent_id ?? "";
    }
    if (changed) setExpanded(next);
    if (!selected) selected = { kind: "note", id: note.id };
  });

  $effect(() => {
    tree?.focus();
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

  function toggle(id: string, open?: boolean) {
    const next = new Set(expanded);
    const shouldOpen = open ?? !next.has(id);
    if (shouldOpen) next.add(id);
    else next.delete(id);
    setExpanded(next);
  }

  function countIn(id: string): number {
    return notesIn(id).length + childFolders(id).reduce((s, f) => s + countIn(f.id), 0);
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

  function parentOf(sel: Sel): string {
    if (sel.kind === "note") return noteOf(sel.id)?.parent_id ?? rootId;
    if (sel.kind === "folder") return folderOf(sel.id)?.parent_id ?? rootId;
    return "";
  }

  /** Folder a paste or a drop onto `sel` goes into. */
  function targetFolder(sel: Sel | null): string {
    if (!sel || sel.kind === "root") return rootId;
    return sel.kind === "folder" ? sel.id : parentOf(sel);
  }

  function title(sel: Sel): string {
    if (sel.kind === "note") return noteOf(sel.id)?.title || t("note.untitled");
    return (sel.kind === "root" ? root : folderOf(sel.id))?.title ?? "";
  }

  // ---------------------------------------------------------------- actions

  async function move(kind: TreeKind, id: string, dest: string) {
    if (kind === "note") {
      if (noteOf(id)?.parent_id === dest) return;
      await api.moveNote(id, dest);
    } else {
      if (isInside(dest, id) || folderOf(id)?.parent_id === dest) return;
      await api.moveFolder(id, dest);
    }
    if (dest !== rootId) toggle(dest, true);
    onChanged();
  }

  async function copyFolder(id: string, dest: string, rename: boolean) {
    const src = folderOf(id);
    if (!src) return;
    const name = rename ? `${src.title} ${t("ctx.copySuffix")}` : src.title;
    const copy = await api.createFolder(name, dest);
    for (const n of notesIn(id)) {
      const full = await api.note(n.id);
      if (full) await api.createNote(copy.id, full.text);
    }
    for (const f of childFolders(id)) await copyFolder(f.id, copy.id, false);
  }

  async function paste(sel: Sel | null) {
    const c = treeClipboard.clip;
    if (!c) return;
    const dest = targetFolder(sel);
    if (c.mode === "cut") {
      await move(c.kind, c.id, dest);
      treeClipboard.clip = null;
      return;
    }
    if (c.kind === "note") {
      const full = await api.note(c.id);
      if (!full) return;
      const sameFolder = noteOf(c.id)?.parent_id === dest;
      const [first, ...rest] = full.text.split("\n");
      const text = sameFolder ? [`${first} ${t("ctx.copySuffix")}`, ...rest].join("\n") : full.text;
      await api.createNote(dest, text);
    } else {
      if (isInside(dest, c.id)) return;
      await copyFolder(c.id, dest, folderOf(c.id)?.parent_id === dest);
    }
    if (dest !== rootId) toggle(dest, true);
    onChanged();
  }

  async function remove(sel: Sel) {
    if (sel.kind === "note") {
      if (!confirm(t("ctx.deleteNoteConfirm", { name: title(sel) }))) return;
      onDeleteNote(sel.id);
    } else if (sel.kind === "folder") {
      if (!confirm(t("nav.trashFolderConfirm", { name: title(sel) }))) return;
      await api.trashFolder(sel.id);
      if (scope === sel.id) onSelectFolder("");
      onChanged();
    }
    selected = null;
  }

  async function addFolder() {
    const parent = adding;
    const name = newName.trim();
    adding = null;
    newName = "";
    if (!parent || !name) return;
    await api.createFolder(name, parent);
    if (parent !== rootId) toggle(parent, true);
    onChanged();
    tree?.focus();
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
    if (parent !== rootId) toggle(parent, true);
  }

  function activate(r: Row) {
    selected = { kind: r.kind, id: r.id };
    if (r.kind === "note") onOpenNote(r.id);
    else if (r.kind === "root") onSelectFolder("");
    else {
      toggle(r.id, true);
      onSelectFolder(r.id);
    }
  }

  // ------------------------------------------------------------ drag & drop

  function dragStart(e: DragEvent, r: Row) {
    if (r.kind === "root" || !e.dataTransfer) return;
    e.dataTransfer.setData(DRAG_TYPE, JSON.stringify({ kind: r.kind, id: r.id }));
    e.dataTransfer.effectAllowed = "move";
    selected = { kind: r.kind, id: r.id };
  }

  function dropFolderFor(r: Row): string {
    return r.kind === "note" ? parentOf(r) : r.id;
  }

  function dragOver(e: DragEvent, r: Row) {
    if (!e.dataTransfer?.types.includes(DRAG_TYPE)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    dropTarget = dropFolderFor(r);
  }

  async function drop(e: DragEvent, r: Row) {
    const data = e.dataTransfer?.getData(DRAG_TYPE);
    dropTarget = null;
    if (!data) return;
    e.preventDefault();
    e.stopPropagation();
    const item = JSON.parse(data) as { kind: TreeKind; id: string };
    await move(item.kind, item.id, dropFolderFor(r));
  }

  // --------------------------------------------------------------- keyboard

  function key(e: KeyboardEvent) {
    if (renaming || adding) return;
    const mod = MOD === "⌘" ? e.metaKey : e.ctrlKey;
    const sel = selected;
    const k = e.key.toLowerCase();
    let handled = true;
    if (mod && k === "x" && sel && sel.kind !== "root") treeClipboard.clip = { mode: "cut", kind: sel.kind, id: sel.id };
    else if (mod && k === "c" && sel && sel.kind !== "root") treeClipboard.clip = { mode: "copy", kind: sel.kind, id: sel.id };
    else if (mod && k === "v") void paste(sel);
    else if ((k === "delete" || k === "backspace") && sel) void remove(sel);
    else if (k === "enter" && sel) {
      const r = rows.find((x) => x.id === sel.id);
      if (r) activate(r);
    } else if (k === "arrowdown" || k === "arrowup") {
      const i = rows.findIndex((x) => x.id === sel?.id);
      const j = Math.max(0, Math.min(rows.length - 1, i + (k === "arrowdown" ? 1 : -1)));
      selected = { kind: rows[j].kind, id: rows[j].id };
      tree?.querySelector(`[data-id="${rows[j].id}"]`)?.scrollIntoView({ block: "nearest" });
    } else if ((k === "arrowright" || k === "arrowleft") && sel?.kind === "folder") toggle(sel.id, k === "arrowright");
    else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  function openMenu(e: MouseEvent, target: Sel) {
    e.preventDefault();
    selected = target;
    menu = { x: Math.min(e.clientX, window.innerWidth - 230), y: Math.min(e.clientY, window.innerHeight - 290), target };
  }

  function menuAction(fn: () => void) {
    menu = null;
    fn();
    tree?.focus();
  }
</script>

<div class="scrim" onclick={onClose} role="presentation"></div>
<aside class:mac>
  <div class="head">
    <strong>{t("nav.folders")}</strong>
    <button class="close" onclick={onClose} aria-label={t("act.close")}>✕</button>
  </div>

  <div class="tree" role="tree" tabindex="0" bind:this={tree} onkeydown={key}>
    {#each rows as r (r.kind + r.id)}
      {@const isSel = selected?.id === r.id}
      {@const isCut = clip?.mode === "cut" && clip.id === r.id}
      <div
        class="row {r.kind}"
        class:sel={isSel}
        class:current={r.kind === "note" && r.id === currentId}
        class:scope={(r.kind === "root" && scope === "") || (r.kind === "folder" && scope === r.id)}
        class:cut={isCut}
        class:drop={(r.kind === "root" || r.kind === "folder") && dropTarget === r.id}
        style="--depth: {r.depth}"
        data-id={r.id}
        role="treeitem"
        aria-selected={isSel}
        aria-expanded={r.kind === "folder" ? expanded.has(r.id) : undefined}
        tabindex="-1"
        draggable={r.kind !== "root" && renaming !== r.id}
        ondragstart={(e) => dragStart(e, r)}
        ondragover={(e) => dragOver(e, r)}
        ondragleave={() => (dropTarget = null)}
        ondrop={(e) => drop(e, r)}
        onclick={() => activate(r)}
        oncontextmenu={(e) => openMenu(e, { kind: r.kind, id: r.id })}
        onkeydown={() => {}}
      >
        {#if r.kind === "folder"}
          <button
            class="twisty"
            aria-label={expanded.has(r.id) ? "−" : "+"}
            onclick={(e) => (e.stopPropagation(), toggle(r.id))}>{expanded.has(r.id) ? "▾" : "▸"}</button
          >
        {:else if r.kind === "note"}
          <span class="glyph">·</span>
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
        {:else if r.kind === "note"}
          {@const n = noteOf(r.id)}
          <span class="label">
            {#if n?.encrypted}{@html icons.lock} {t("note.encrypted")}{:else}{n?.title || t("note.untitled")}{/if}{n?.is_conflict ? " ⚠︎" : ""}
          </span>
          <span class="meta">{n ? when(n.updated_time, i18n.lang) : ""}</span>
        {:else}
          {@const f = r.kind === "root" ? root : folderOf(r.id)}
          <span class="label">{f?.icon ? `${f.icon} ` : ""}{f?.title ?? "Omanote"}</span>
          <span class="meta">{r.kind === "root" ? notes.length : countIn(r.id)}</span>
        {/if}

        <span class="actions" class:always={r.kind === "root"}>
          {#if r.kind !== "note"}
            <button title={t("ctx.newNote")} onclick={(e) => (e.stopPropagation(), onNewNote(r.id))}>＋</button>
            <button title={t("ctx.newFolder")} onclick={(e) => (e.stopPropagation(), startNewFolder(r.id))}>▣</button>
          {/if}
          <button title="⋯" onclick={(e) => (e.stopPropagation(), openMenu(e, { kind: r.kind, id: r.id }))}>⋯</button>
        </span>
      </div>

      {#if adding === r.id && r.kind !== "note"}
        <div class="row" style="--depth: {r.depth + 1}">
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
  </div>

  <button class="settings" onclick={onSettings}>⚙ {t("nav.settings")}</button>
</aside>

{#if menu}
  {@const m = menu}
  <div class="menu-scrim" onclick={() => (menu = null)} oncontextmenu={(e) => (e.preventDefault(), (menu = null))} role="presentation"></div>
  <div class="menu" style="left: {m.x}px; top: {m.y}px" role="menu">
    {#if m.target.kind === "note"}
      <button role="menuitem" onclick={() => menuAction(() => onOpenNote(m.target.id))}>{t("ctx.open")}</button>
    {:else}
      <button role="menuitem" onclick={() => menuAction(() => onNewNote(m.target.id))}>{t("ctx.newNote")}</button>
      <button role="menuitem" onclick={() => menuAction(() => startNewFolder(m.target.id))}>{t("ctx.newFolder")}</button>
    {/if}
    <hr />
    {#if m.target.kind !== "root"}
      <button role="menuitem" onclick={() => menuAction(() => (treeClipboard.clip = { mode: "cut", kind: m.target.kind as TreeKind, id: m.target.id }))}
        >{t("ctx.cut")} <kbd>{MOD}X</kbd></button
      >
      <button role="menuitem" onclick={() => menuAction(() => (treeClipboard.clip = { mode: "copy", kind: m.target.kind as TreeKind, id: m.target.id }))}
        >{t("ctx.copy")} <kbd>{MOD}C</kbd></button
      >
    {/if}
    <button role="menuitem" disabled={!clip} onclick={() => menuAction(() => void paste(m.target))}>{t("ctx.paste")} <kbd>{MOD}V</kbd></button>
    {#if m.target.kind !== "root"}
      <hr />
      {#if m.target.kind === "folder"}
        <button role="menuitem" onclick={() => menuAction(() => ((renaming = m.target.id), (renameName = title(m.target))))}>{t("ctx.rename")}</button>
      {/if}
      <button role="menuitem" class="danger" onclick={() => menuAction(() => void remove(m.target))}>{t("ctx.delete")} <kbd>⌫</kbd></button>
    {/if}
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    z-index: 10;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
  }
  aside {
    position: fixed;
    z-index: 11;
    top: 0;
    bottom: 0;
    left: 0;
    width: min(22rem, 88vw);
    background: var(--bg-elev);
    border-right: 1px solid var(--accent);
    padding: calc(10px + env(safe-area-inset-top)) 6px calc(10px + env(safe-area-inset-bottom));
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  /* macOS: the traffic lights sit in the top-left corner (no title bar). */
  aside.mac {
    padding-top: 44px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 2px 8px 6px;
    color: var(--accent);
  }
  .close {
    color: var(--muted);
  }
  .tree {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    outline: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 32px;
    padding: 0 4px 0 calc(var(--depth, 0) * 16px + 4px);
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
  }
  .tree:focus-within .row.sel {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .row.scope {
    border-left-color: var(--accent);
  }
  .row.cut {
    opacity: 0.45;
  }
  .row.drop {
    background: var(--accent-soft);
    outline: 1px dashed var(--accent);
    outline-offset: -1px;
  }
  .row input {
    flex: 1;
    margin: 2px 6px;
    padding: 4px 8px;
  }
  .twisty {
    width: 22px;
    padding: 4px 0;
    color: var(--muted);
    flex: none;
  }
  .glyph {
    width: 22px;
    flex: none;
    text-align: center;
    color: var(--muted);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 5px 2px;
  }
  .meta {
    color: var(--muted);
    font-size: 0.75rem;
    flex: none;
  }
  .folder .label,
  .root .label {
    font-weight: 700;
  }
  .root {
    border-bottom: 1px solid var(--line);
    margin-bottom: 4px;
  }
  .root .label {
    color: var(--accent);
  }
  .note.current .label,
  .note.current .glyph {
    color: var(--accent);
  }
  .actions {
    display: flex;
    opacity: 0;
    flex: none;
  }
  .actions button {
    padding: 3px 5px;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .actions button:hover {
    color: var(--accent);
  }
  .row:hover .actions,
  .row.sel .actions,
  .actions.always {
    opacity: 1;
  }
  @media (hover: none) {
    .actions {
      opacity: 1;
    }
  }
  .settings {
    text-align: left;
    color: var(--muted);
    border-top: 1px solid var(--line);
    padding-top: 8px;
  }
  .menu-scrim {
    position: fixed;
    inset: 0;
    z-index: 12;
  }
  .menu {
    position: fixed;
    z-index: 13;
    min-width: 13rem;
    display: flex;
    flex-direction: column;
    padding: 4px;
    background: var(--bg-elev);
    border: 1px solid var(--accent);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.35);
  }
  .menu button {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    text-align: left;
    padding: 6px 10px;
  }
  .menu button:disabled {
    opacity: 0.35;
  }
  .menu kbd {
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
