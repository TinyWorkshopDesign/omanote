<script lang="ts">
  import { api, type Folder, type NoteSummary, when } from "../api";
  import { i18n, t } from "../i18n.svelte";

  let {
    folders,
    notes,
    selected,
    currentId,
    rootId,
    onSelectFolder,
    onSelectNote,
    onChanged,
    onClose,
    onSettings,
  }: {
    folders: Folder[];
    notes: NoteSummary[];
    selected: string;
    currentId: string | null;
    rootId: string;
    onSelectFolder: (id: string) => void;
    onSelectNote: (id: string) => void;
    onChanged: () => void;
    onClose: () => void;
    onSettings: () => void;
  } = $props();

  let adding = $state(false);
  let newName = $state("");
  let renaming = $state<string | null>(null);
  let renameName = $state("");

  // Folders under the Omanote root, nested by depth.
  const tree = $derived(build(folders, rootId, 0));

  function build(all: Folder[], parent: string, depth: number): { f: Folder; depth: number }[] {
    return all
      .filter((f) => f.parent_id === parent)
      .flatMap((f) => [{ f, depth }, ...build(all, f.id, depth + 1)]);
  }

  async function addFolder() {
    const title = newName.trim();
    adding = false;
    newName = "";
    if (!title) return;
    const parent = selected || rootId;
    await api.createFolder(title, parent);
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
    if (selected === f.id) onSelectFolder("");
    onChanged();
  }
</script>

<div class="scrim" onclick={onClose} role="presentation"></div>
<aside>
  <div class="head">
    <strong>{t("nav.folders")}</strong>
    <button title={t("nav.newFolder")} onclick={() => ((adding = true), (newName = ""))}>＋</button>
  </div>

  <button class="folder" class:sel={selected === ""} onclick={() => onSelectFolder("")}>
    <span>{t("nav.allNotes")}</span>
    <span class="count">{notes.length}</span>
  </button>

  {#each tree as { f, depth } (f.id)}
    <div class="folder-row" style="padding-left: {depth * 12}px">
      {#if renaming === f.id}
        <input bind:value={renameName} onblur={commitRename} onkeydown={(e) => e.key === "Enter" && commitRename()} />
      {:else}
        <button class="folder" class:sel={selected === f.id} onclick={() => onSelectFolder(f.id)}>
          <span>{f.icon ? `${f.icon} ` : ""}{f.title}</span>
          <span class="count">{f.note_count}</span>
        </button>
        <button class="mini" title={t("nav.rename")} onclick={() => ((renaming = f.id), (renameName = f.title))}>✎</button>
        <button class="mini" title={t("nav.trash")} onclick={() => removeFolder(f)}>🗑</button>
      {/if}
    </div>
  {/each}

  {#if adding}
    <input placeholder={t("nav.folderName")} bind:value={newName} onblur={addFolder} onkeydown={(e) => e.key === "Enter" && addFolder()} />
  {/if}

  <div class="notes">
    {#each notes as n (n.id)}
      <button class="note" class:sel={n.id === currentId} onclick={() => onSelectNote(n.id)}>
        <span class="t">{n.encrypted ? `🔒 ${t("note.encrypted")}` : n.title || t("note.untitled")}{n.is_conflict ? " ⚠︎" : ""}</span>
        <span class="p">{n.preview.replace(/\n/g, " ").slice(0, 60)}</span>
        <span class="w">{when(n.updated_time, i18n.lang)}</span>
      </button>
    {/each}
  </div>

  <button class="settings" onclick={onSettings}>⚙ {t("nav.settings")}</button>
</aside>

<style>
  .scrim {
    position: fixed;
    z-index: 10;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
  }
  aside {
    position: fixed;
    z-index: 11;
    top: 0;
    bottom: 0;
    left: 0;
    width: min(19rem, 84vw);
    background: var(--bg-elev);
    border-right: 1px solid var(--line);
    padding: 10px 8px calc(10px + env(safe-area-inset-bottom));
    display: flex;
    flex-direction: column;
    gap: 3px;
    overflow-y: auto;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 6px 6px;
    padding-top: calc(4px + env(safe-area-inset-top));
  }
  .folder-row {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .folder-row .folder {
    flex: 1;
  }
  .folder {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    text-align: left;
  }
  .folder.sel {
    background: var(--accent-soft);
    font-weight: 600;
  }
  .count {
    color: var(--muted);
    font-size: 0.78rem;
  }
  .mini {
    opacity: 0;
    font-size: 0.8rem;
    padding: 2px 4px;
  }
  .folder-row:hover .mini {
    opacity: 0.7;
  }
  .notes {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin-top: 10px;
    border-top: 1px solid var(--line);
    padding-top: 8px;
  }
  .note {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0 8px;
    text-align: left;
    padding: 6px 8px;
  }
  .note .t {
    font-weight: 550;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note .p {
    grid-column: 1;
    color: var(--muted);
    font-size: 0.8rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note .w {
    grid-row: 1;
    grid-column: 2;
    color: var(--muted);
    font-size: 0.75rem;
  }
  .note.sel {
    background: var(--panel);
  }
  .settings {
    margin-top: auto;
    text-align: left;
    color: var(--muted);
  }
</style>
