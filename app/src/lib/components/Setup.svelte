<script lang="ts">
  import { icons } from "../icons";
  import { api, type Folder } from "../api";
  import { errText, t } from "../i18n.svelte";

  let {
    onDone,
    onCancel,
    localRoot = "",
  }: {
    onDone: () => void;
    /** Present when connecting a server from local mode (Settings). */
    onCancel?: () => void;
    /** The local-mode notebook: offered first so its notes move to the server. */
    localRoot?: string;
  } = $props();

  let step = $state<"login" | "folder" | "local">("login");
  let localName = $state("Omanote");
  let serverUrl = $state("");
  let email = $state("");
  let password = $state("");
  let master = $state("");
  let needsMaster = $state(false);
  let busy = $state(false);
  let error = $state("");
  let folders = $state<Folder[]>([]);
  let chosen = $state("");
  let newTitle = $state("Omanote");
  /** "All of Joplin": every notebook shows as a folder; `chosen` is where new notes go. */
  let whole = $state(false);
  const homeName = $derived(folders.find((f) => f.id === chosen)?.title ?? newTitle);

  async function login() {
    busy = true;
    error = "";
    try {
      await api.setup(serverUrl, email, password, needsMaster ? master : undefined);
      folders = (await api.folders()).filter((f) => f.parent_id === "");
      if (localRoot && folders.some((f) => f.id === localRoot)) chosen = localRoot;
      step = "folder";
    } catch (e) {
      if (String(e).startsWith("E2EE_REQUIRED")) {
        needsMaster = true;
        error = t("setup.e2ee");
      } else {
        error = errText(e);
      }
    } finally {
      busy = false;
    }
  }

  async function startLocal() {
    busy = true;
    error = "";
    try {
      await api.useLocal(localName);
      onDone();
    } catch (e) {
      error = errText(e);
    } finally {
      busy = false;
    }
  }

  function toggleWhole() {
    whole = !whole;
    // New notes need a notebook: prefer an existing one to creating another.
    if (whole && !chosen && folders.length) chosen = folders[0].id;
  }

  async function pickFolder() {
    busy = true;
    error = "";
    try {
      await api.setRootFolder(chosen || undefined, chosen ? undefined : newTitle);
      if (whole) await api.setWholeJoplin(true);
      onDone();
    } catch (e) {
      error = errText(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay">
  <div class="card">
    <h1>Omanote</h1>
    {#if step === "login"}
      <p class="sub">{t("app.tagline")}</p>
      <label>{t("setup.server")}<input type="url" inputmode="url" bind:value={serverUrl} placeholder="https://joplin.example.com" autocomplete="off" autocapitalize="off" autocorrect="off" spellcheck="false" /></label>
      <label>{t("setup.email")}<input type="email" inputmode="email" bind:value={email} placeholder="you@example.com" autocomplete="username" autocapitalize="off" autocorrect="off" spellcheck="false" /></label>
      <label>{t("setup.password")}<input type="password" bind:value={password} autocomplete="current-password" /></label>
      {#if needsMaster}
        <label>{t("setup.master")}<input type="password" bind:value={master} /></label>
      {/if}
      <button class="primary" onclick={login} disabled={busy || !serverUrl || !email || !password}>
        {busy ? t("setup.connecting") : t("setup.connect")}
      </button>
      <p class="hint">{t("setup.keychain")}</p>
      {#if onCancel}
        <button class="secondary" onclick={onCancel}>{t("setup.cancel")}</button>
      {:else}
        <div class="or"><span></span></div>
        <button class="secondary" onclick={() => ((step = "local"), (error = ""))}>{t("setup.local")}</button>
      {/if}
    {:else if step === "local"}
      <p class="sub">{t("setup.localHint")}</p>
      <label>{t("setup.localName")}<input bind:value={localName} onkeydown={(e) => e.key === "Enter" && startLocal()} /></label>
      <button class="primary" onclick={startLocal} disabled={busy}>{busy ? t("setup.wait") : t("setup.start")}</button>
      <button class="secondary" onclick={() => (step = "login")}>{t("setup.back")}</button>
    {:else}
      <p class="sub">{t("setup.pickNotebook")}</p>
      <div class="list">
        <button class="row whole" class:sel={whole} onclick={toggleWhole}>
          <span>{t("set.wholeJoplin")}</span><span class="count">{folders.length}</span>
        </button>
      </div>
      {#if whole}
        <p class="hint">{t("set.wholeJoplinHint", { name: homeName })}</p>
        <p class="sub">{t("setup.newNotesIn")}</p>
      {/if}
      <div class="list">
        {#each folders as f (f.id)}
          <button class="row" class:sel={chosen === f.id} onclick={() => (chosen = f.id)}>
            <span>{f.icon ? `${f.icon} ` : ""}{f.title}{f.id === localRoot ? ` · ${t("setup.keepLocal")}` : ""}</span><span class="count">{f.note_count}</span>
          </button>
        {/each}
        <button class="row" class:sel={chosen === ""} onclick={() => (chosen = "")}>
          <span>{@html icons.plus} {t("setup.createNotebook")}</span>
        </button>
      </div>
      {#if chosen === ""}
        <label>{t("setup.notebookName")}<input bind:value={newTitle} /></label>
      {/if}
      <button class="primary" onclick={pickFolder} disabled={busy}>{busy ? t("setup.wait") : t("setup.start")}</button>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    z-index: 20;
    inset: 0;
    background: var(--bg);
    display: grid;
    place-items: center;
    padding: 20px;
    overflow: auto;
  }
  .card {
    width: min(26rem, 100%);
    display: flex;
    flex-direction: column;
    gap: 11px;
  }
  h1 {
    margin: 0;
    font-size: 1.5rem;
    letter-spacing: -0.01em;
  }
  .sub {
    margin: 0 0 6px;
    color: var(--muted);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 0.83rem;
    color: var(--muted);
  }
  .primary {
    background: var(--accent);
    color: var(--bg);
    padding: 10px;
    font-weight: 600;
    margin-top: 6px;
  }
  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .secondary {
    border: 1px solid var(--line);
    padding: 9px;
    color: var(--text);
  }
  .or {
    height: 1px;
    background: var(--line);
    margin: 4px 0;
  }
  .hint,
  .error {
    font-size: 0.8rem;
    color: var(--muted);
    margin: 0;
  }
  .error {
    color: var(--danger);
  }
  .list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .row {
    display: flex;
    justify-content: space-between;
    padding: 9px 11px;
    border-radius: 0;
    text-align: left;
  }
  .row.sel {
    background: var(--accent-soft);
  }
  .row.whole {
    font-weight: 700;
  }
  .count {
    color: var(--muted);
    font-size: 0.8rem;
  }
</style>
