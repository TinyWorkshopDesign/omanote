<script lang="ts">
  // Navigation panel: the working notebook as a tree (notebook → folders →
  // notes). Folders expand/collapse (remembered per device), the branch of the
  // open note is expanded automatically, notes can be dragged onto folders.
  import { api, type Folder, type NoteSummary, when } from "../api";
  import { i18n, t } from "../i18n.svelte";

  let {
    folders,
    rootId,
    currentId,
    scope,
    version,
    onOpenNote,
    onSelectFolder,
    onNewNote,
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
    onOpenNote: (id: string) => void;
    onSelectFolder: (id: string) => void;
    onNewNote: (folderId: string) => void;
    onChanged: () => void;
    onClose: () => void;
    onSettings: () => void;
  } = $props();

  const EXPANDED_KEY = "omanote.expanded";

  let notes = $state<NoteSummary[]>([]);
  let expanded = $state<Set<string>>(loadExpanded());
  let adding = $state<string | null>(null); // parent id of the folder being created
  let newName = $state("");
  let renaming = $state<string | null>(null);
  let renameName = $state("");
  let dropTarget = $state<string | null>(null);

  const root = $derived(folders.find((f) => f.id === rootId));
  const childFolders = $derived((parent: string) =>
    folders.filter((f) => f.parent_id === parent).sort((a, b) => a.title.localeCompare(b.title)),
  );
  const notesIn = $derived((parent: string) => notes.filter((n) => n.parent_id === parent));

  $effect(() => {
    void version;
    api.notes().then((n) => (notes = n));
  });

  // Reveal the open note: expand every folder above it.
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
      p = folders.find((f) => f.id === p)?.parent_id ?? "";
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

  function toggle(id: string) {
    const next = new Set(expanded);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setExpanded(next);
  }

  function countIn(id: string): number {
    return notesIn(id).length + childFolders(id).reduce((s, f) => s + countIn(f.id), 0);
  }

  async function addFolder() {
    const parent = adding;
    const title = newName.trim();
    adding = null;
    newName = "";
    if (!parent || !title) return;
    await api.createFolder(title, parent);
    setExpanded(new Set([...expanded, parent]));
    onChanged();
  }

  async function commitRename() {
    const id = renaming;
    renaming = null;
    if (id && renameName.trim()) {
      await api.renameFolder(id, renameName.trim());
      onChanged();
    }
  }

  async function removeFolder(f: Folder) {
    if (!confirm(t("nav.trashFolderConfirm", { name: f.title }))) return;
    await api.trashFolder(f.id);
    if (scope === f.id) onSelectFolder("");
    onChanged();
  }

  async function drop(e: DragEvent, folderId: string) {
    e.preventDefault();
    dropTarget = null;
    const id = e.dataTransfer?.getData("text/omanote-note");
    if (!id) return;
    const note = notes.find((n) => n.id === id);
    if (!note || note.parent_id === folderId) return;
    await api.moveNote(id, folderId);
    setExpanded(new Set([...expanded, folderId]));
    onChanged();
  }

  function dragOver(e: DragEvent, folderId: string) {
    if (e.dataTransfer?.types.includes("text/omanote-note")) {
      e.preventDefault();
      dropTarget = folderId;
    }
  }
</script>

{#snippet noteRow(n: NoteSummary, depth: number)}
  <button
    class="row note"
    class:current={n.id === currentId}
    style="--depth: {depth}"
    draggable="true"
    ondragstart={(e) => e.dataTransfer?.setData("text/omanote-note", n.id)}
    onclick={() => onOpenNote(n.id)}
  >
    <span class="glyph">·</span>
    <span class="label">{n.encrypted ? `🔒 ${t("note.encrypted")}` : n.title || t("note.untitled")}{n.is_conflict ? " ⚠︎" : ""}</span>
    <span class="meta">{when(n.updated_time, i18n.lang)}</span>
  </button>
{/snippet}

{#snippet folderRow(f: Folder, depth: number)}
  {@const open = expanded.has(f.id)}
  <div
    class="row folder"
    class:scope={scope === f.id}
    class:drop={dropTarget === f.id}
    style="--depth: {depth}"
    role="treeitem"
    aria-expanded={open}
    aria-selected={scope === f.id}
    tabindex="-1"
    ondragover={(e) => dragOver(e, f.id)}
    ondragleave={() => dropTarget === f.id && (dropTarget = null)}
    ondrop={(e) => drop(e, f.id)}
  >
    <button class="twisty" aria-label={open ? "−" : "+"} onclick={() => toggle(f.id)}>{open ? "▾" : "▸"}</button>
    {#if renaming === f.id}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        autofocus
        bind:value={renameName}
        onblur={commitRename}
        onkeydown={(e) => (e.key === "Enter" ? commitRename() : e.key === "Escape" && (renaming = null))}
      />
    {:else}
      <button class="label-btn" onclick={() => (toggle(f.id), onSelectFolder(f.id))}>
        <span class="label">{f.icon ? `${f.icon} ` : ""}{f.title}</span>
        <span class="meta">{countIn(f.id)}</span>
      </button>
      <span class="actions">
        <button title={t("act.newNote")} onclick={() => onNewNote(f.id)}>＋</button>
        <button title={t("nav.newFolder")} onclick={() => ((adding = f.id), (newName = ""))}>▣</button>
        <button title={t("nav.rename")} onclick={() => ((renaming = f.id), (renameName = f.title))}>✎</button>
        <button title={t("nav.trash")} onclick={() => removeFolder(f)}>✕</button>
      </span>
    {/if}
  </div>
  {#if open}
    {@render children(f.id, depth + 1)}
  {/if}
{/snippet}

{#snippet children(parent: string, depth: number)}
  {#if adding === parent}
    <div class="row" style="--depth: {depth}">
      <!-- svelte-ignore a11y_autofocus -->
      <input
        autofocus
        placeholder={t("nav.folderName")}
        bind:value={newName}
        onblur={addFolder}
        onkeydown={(e) => (e.key === "Enter" ? addFolder() : e.key === "Escape" && (adding = null))}
      />
    </div>
  {/if}
  {#each childFolders(parent) as f (f.id)}
    {@render folderRow(f, depth)}
  {/each}
  {#each notesIn(parent) as n (n.id)}
    {@render noteRow(n, depth)}
  {/each}
{/snippet}

<div class="scrim" onclick={onClose} role="presentation"></div>
<aside>
  <div class="head">
    <strong>{t("nav.folders")}</strong>
    <button class="close" onclick={onClose} aria-label={t("act.close")}>✕</button>
  </div>

  <div class="tree" role="tree">
    <div
      class="row folder rootrow"
      class:scope={scope === ""}
      class:drop={dropTarget === rootId}
      role="treeitem"
      aria-selected={scope === ""}
      tabindex="-1"
      ondragover={(e) => dragOver(e, rootId)}
      ondragleave={() => dropTarget === rootId && (dropTarget = null)}
      ondrop={(e) => drop(e, rootId)}
    >
      <button class="label-btn" onclick={() => onSelectFolder("")}>
        <span class="label">{root?.icon ? `${root.icon} ` : ""}{root?.title ?? "Omanote"}</span>
        <span class="meta">{notes.length}</span>
      </button>
      <span class="actions always">
        <button title={t("act.newNote")} onclick={() => onNewNote(rootId)}>＋</button>
        <button title={t("nav.newFolder")} onclick={() => ((adding = rootId), (newName = ""))}>▣</button>
      </span>
    </div>
    {@render children(rootId, 1)}
  </div>

  <button class="settings" onclick={onSettings}>⚙ {t("nav.settings")}</button>
</aside>

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
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: 32px;
    padding-left: calc(var(--depth, 0) * 16px);
    border-left: 2px solid transparent;
  }
  .row input {
    margin: 2px 6px;
    padding: 4px 8px;
  }
  .twisty {
    width: 22px;
    padding: 4px 0;
    color: var(--muted);
    flex: none;
  }
  .label-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    text-align: left;
    padding: 5px 6px;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    color: var(--muted);
    font-size: 0.75rem;
    flex: none;
  }
  .folder .label {
    font-weight: 700;
  }
  .rootrow {
    border-bottom: 1px solid var(--line);
    margin-bottom: 4px;
    padding-left: 4px;
  }
  .rootrow .label {
    color: var(--accent);
  }
  .row.scope {
    border-left-color: var(--accent);
    background: var(--accent-soft);
  }
  .row.drop {
    outline: 1px dashed var(--accent);
    outline-offset: -1px;
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
  .actions.always,
  .row:focus-within .actions {
    opacity: 1;
  }
  @media (hover: none) {
    .actions {
      opacity: 1;
    }
  }
  .note {
    width: 100%;
    text-align: left;
    justify-content: space-between;
    padding-right: 6px;
    cursor: pointer;
  }
  .note .glyph {
    width: 22px;
    flex: none;
    text-align: center;
    color: var(--muted);
  }
  .note .label {
    flex: 1;
  }
  .note.current {
    background: var(--panel);
    border-left-color: var(--accent);
  }
  .note.current .label,
  .note.current .glyph {
    color: var(--accent);
  }
  .settings {
    text-align: left;
    color: var(--muted);
    border-top: 1px solid var(--line);
    padding-top: 8px;
  }
</style>
