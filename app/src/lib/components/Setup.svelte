<script lang="ts">
  import { api, type Folder } from "../api";
  import { errText, t } from "../i18n.svelte";

  let { onDone }: { onDone: () => void } = $props();

  let step = $state<"login" | "folder">("login");
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

  async function login() {
    busy = true;
    error = "";
    try {
      await api.setup(serverUrl, email, password, needsMaster ? master : undefined);
      folders = (await api.folders()).filter((f) => f.parent_id === "");
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

  async function pickFolder() {
    busy = true;
    error = "";
    try {
      await api.setRootFolder(chosen || undefined, chosen ? undefined : newTitle);
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
      <label>{t("setup.server")}<input bind:value={serverUrl} placeholder="https://joplin.example.com" autocomplete="off" /></label>
      <label>{t("setup.email")}<input bind:value={email} placeholder="io@example.com" autocomplete="off" /></label>
      <label>{t("setup.password")}<input type="password" bind:value={password} /></label>
      {#if needsMaster}
        <label>{t("setup.master")}<input type="password" bind:value={master} /></label>
      {/if}
      <button class="primary" onclick={login} disabled={busy || !serverUrl || !email || !password}>
        {busy ? t("setup.connecting") : t("setup.connect")}
      </button>
      <p class="hint">{t("setup.keychain")}</p>
    {:else}
      <p class="sub">{t("setup.pickNotebook")}</p>
      <div class="list">
        {#each folders as f (f.id)}
          <button class="row" class:sel={chosen === f.id} onclick={() => (chosen = f.id)}>
            <span>{f.title}</span><span class="count">{f.note_count}</span>
          </button>
        {/each}
        <button class="row" class:sel={chosen === ""} onclick={() => (chosen = "")}>
          <span>＋ {t("setup.createNotebook")}</span>
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
  .count {
    color: var(--muted);
    font-size: 0.8rem;
  }
</style>
