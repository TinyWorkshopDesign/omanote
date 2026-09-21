<script lang="ts">
  // Quick switcher: search notes (mode "search") or pick a folder (mode "move").
  import { api, type Folder, type NoteSummary, when } from "../api";
  import { i18n, t } from "../i18n.svelte";
  import { icons } from "../icons";

  let {
    mode,
    folders,
    notes,
    onPickNote,
    onPickFolder,
    onCreate,
    onClose,
  }: {
    mode: "search" | "move";
    folders: Folder[];
    notes: NoteSummary[];
    onPickNote: (id: string) => void;
    onPickFolder: (id: string) => void;
    /** Return with no results creates a note from the query. */
    onCreate: (text: string) => void;
    onClose: () => void;
  } = $props();

  let query = $state("");
  let hits = $state<NoteSummary[]>([]);
  let index = $state(0);
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    input?.focus();
  });

  $effect(() => {
    const q = query.trim();
    if (mode !== "search") return;
    if (!q) {
      hits = notes.slice(0, 40);
      return;
    }
    let alive = true;
    api.search(q).then((r) => alive && (hits = r.slice(0, 40)));
    return () => {
      alive = false;
    };
  });

  const folderHits = $derived(
    folders.filter((f) => f.title.toLowerCase().includes(query.trim().toLowerCase())),
  );
  const count = $derived(mode === "search" ? hits.length : folderHits.length);

  function choose(i = index) {
    if (mode === "search") {
      if (hits[i]) onPickNote(hits[i].id);
      else if (query.trim()) onCreate(query.trim());
    } else if (folderHits[i]) onPickFolder(folderHits[i].id);
  }

  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown") index = Math.min(index + 1, count - 1);
    else if (e.key === "ArrowUp") index = Math.max(index - 1, 0);
    else if (e.key === "Enter") choose();
    else if (e.key === "Escape") onClose();
    else return;
    e.preventDefault();
  }
</script>

<div class="scrim" onclick={onClose} role="presentation"></div>
<div class="palette">
  <input
    bind:this={input}
    bind:value={query}
    onkeydown={key}
    placeholder={mode === "search" ? t("palette.search") : t("palette.move")}
  />
  <div class="results">
    {#if mode === "search"}
      {#each hits as n, i (n.id)}
        <button class:sel={i === index} onmouseenter={() => (index = i)} onclick={() => choose(i)}>
          <span class="t">{#if n.encrypted}{@html icons.lock} {t("note.encrypted")}{:else}{n.title || t("note.untitled")}{/if}</span>
          <span class="w">{when(n.updated_time, i18n.lang)}</span>
          <span class="p">{n.preview.replace(/\n/g, " ").slice(0, 70)}</span>
        </button>
      {:else}
        {#if query.trim()}
          <button class="sel" onclick={() => onCreate(query.trim())}><span class="t">↵ {t("palette.create")}</span></button>
        {:else}
          <p class="empty">{t("palette.empty")}</p>
        {/if}
      {/each}
    {:else}
      {#each folderHits as f, i (f.id)}
        <button class:sel={i === index} onmouseenter={() => (index = i)} onclick={() => choose(i)}>
          <span class="t">{f.icon ? `${f.icon} ` : ""}{f.title}</span>
          <span class="w">{f.note_count}</span>
        </button>
      {/each}
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    z-index: 10;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
  }
  .palette {
    position: fixed;
    z-index: 11;
    top: max(8vh, env(safe-area-inset-top));
    left: 50%;
    transform: translateX(-50%);
    width: min(32rem, 92vw);
    background: var(--bg-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.22);
    overflow: hidden;
  }
  input {
    border: 0;
    border-bottom: 1px solid var(--line);
    border-radius: 0;
    padding: 12px 14px;
  }
  input:focus {
    outline: none;
  }
  .results {
    max-height: min(24rem, 60vh);
    overflow-y: auto;
  }
  .results button {
    display: grid;
    grid-template-columns: 1fr auto;
    width: 100%;
    text-align: left;
    padding: 7px 14px;
    border-radius: 0;
  }
  .results button.sel {
    background: var(--accent-soft);
  }
  .t {
    font-weight: 550;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .p {
    grid-column: 1 / -1;
    color: var(--muted);
    font-size: 0.8rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .w {
    color: var(--muted);
    font-size: 0.78rem;
  }
  .empty {
    color: var(--muted);
    padding: 14px;
    margin: 0;
  }
</style>
