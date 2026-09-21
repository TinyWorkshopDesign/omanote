<script lang="ts">
  import { api, type Folder, type Status } from "../api";
  import { LANGUAGES, MOD, i18n, setLang, t, type Key, type Lang } from "../i18n.svelte";
  import { bundledThemes, setBundledTheme, type ThemeState } from "../theme";

  let {
    status,
    folders,
    theme = $bindable(),
    fontSize = $bindable(),
    onRootChanged,
    onLogout,
    onConnect,
    onClose,
  }: {
    status: Status;
    folders: Folder[];
    theme: ThemeState;
    fontSize: number;
    onRootChanged: () => void;
    onLogout: () => void;
    /** Local mode: open the Joplin Server login. */
    onConnect: () => void;
    onClose: () => void;
  } = $props();

  let newNotebook = $state("");
  let copied = $state("");

  const notebooks = $derived(folders.filter((f) => f.parent_id === ""));
  const hyprBinding = "bindd = SUPER ALT, N, Omanote, exec, omanote --toggle";
  const mcpCommand = "claude mcp add omanote -- omanote-cli mcp";

  // Shortcuts (Ctrl instead of ⌘ outside Apple platforms). None of
  // them collide with Omarchy, whose own bindings all use the Super key.
  const shortcuts: [string, Key][] = [
    [`${MOD} N`, "key.new"],
    [`${MOD} [  ${MOD} ]`, "key.prevNext"],
    [`${MOD} 1`, "key.front"],
    [`${MOD} ⇧ 1`, "key.promote"],
    [`${MOD} D`, "key.delete"],
    [`${MOD} F`, "key.search"],
    [`${MOD} ⇧ K`, "key.check"],
    [`${MOD} ⇧ M`, "key.cycle"],
    [`${MOD} B  I  U`, "key.format"],
    [`${MOD} ⇧ X`, "key.strike"],
    [`${MOD} ⇧ H`, "key.heading"],
    [`${MOD} /`, "key.comment"],
    ["⌥ ↑  ⌥ ↓", "key.moveLine"],
    [`${MOD} P`, "key.pin"],
    [`${MOD} W`, "key.hide"],
    [`${MOD} +  ${MOD} −`, "key.zoom"],
    [`${MOD} \\`, "key.folders"],
    [`${MOD} E`, "key.moveNote"],
    [`${MOD} ⇧ O`, "key.capture"],
    [`${MOD} S`, "key.syncKey"],
  ];

  async function copy(text: string) {
    await navigator.clipboard?.writeText(text);
    copied = text;
    setTimeout(() => (copied = ""), 1500);
  }

  const homeName = $derived(folders.find((f) => f.id === status.root_folder_id)?.title ?? "Omanote");

  async function chooseWhole() {
    await api.setWholeJoplin(true);
    onRootChanged();
  }

  async function chooseRoot(id: string) {
    await api.setRootFolder(id);
    onRootChanged();
  }

  async function createRoot() {
    if (!newNotebook.trim()) return;
    await api.setRootFolder(undefined, newNotebook.trim());
    newNotebook = "";
    onRootChanged();
  }

  function changeLang(l: Lang) {
    setLang(l);
    void api.setLanguage(l);
  }
</script>

<div class="scrim" onclick={onClose} role="presentation"></div>
<div class="panel" role="dialog" aria-label={t("set.title")}>
  <header>
    <h2>{t("set.title")}</h2>
    <button onclick={onClose} aria-label={t("act.close")}>✕</button>
  </header>

  <section>
    <h3>{t("set.notebook")}</h3>
    <p class="hint">{t("set.notebookHint")}</p>
    <div class="list">
      {#if !status.local}
        <button class="row whole" class:sel={status.whole_joplin} onclick={chooseWhole}>
          <span>{t("set.wholeJoplin")}</span><span class="muted">{notebooks.length}</span>
        </button>
      {/if}
      {#each notebooks as f (f.id)}
        <button class="row" class:sel={!status.whole_joplin && status.root_folder_id === f.id} onclick={() => chooseRoot(f.id)}>
          <span>{f.icon ? `${f.icon} ` : ""}{f.title}</span>
          <span class="muted"
            >{#if status.whole_joplin && status.root_folder_id === f.id}<span class="badge">{t("set.newNotesHere")}</span>{/if}
            {f.note_count}</span
          >
        </button>
      {/each}
    </div>
    {#if status.whole_joplin}
      <p class="hint">{t("set.wholeJoplinHint", { name: homeName })}</p>
    {/if}
    <div class="inline">
      <input placeholder={t("setup.notebookName")} bind:value={newNotebook} onkeydown={(e) => e.key === "Enter" && createRoot()} />
      <button class="accent" onclick={createRoot} disabled={!newNotebook.trim()}>＋</button>
    </div>
  </section>

  <section>
    <h3>{t("set.appearance")}</h3>
    <div class="field">
      <span>{t("set.language")}</span>
      <select value={i18n.lang} onchange={(e) => changeLang((e.currentTarget as HTMLSelectElement).value as Lang)}>
        {#each Object.entries(LANGUAGES) as [code, name] (code)}
          <option value={code}>{name}</option>
        {/each}
      </select>
    </div>
    <div class="field">
      <span>{t("set.textSize")}</span>
      <span class="stepper">
        <button onclick={() => (fontSize = Math.max(11, fontSize - 1))}>−</button>
        <span>{fontSize}px</span>
        <button onclick={() => (fontSize = Math.min(24, fontSize + 1))}>+</button>
      </span>
    </div>
    <div class="field">
      <span>{t("set.theme")}</span>
      <span class="muted">{theme.current}</span>
    </div>
    {#if theme.fromSystem}
      <p class="hint">{t("set.themeOmarchy")}</p>
    {:else}
      <div class="themes">
        {#each bundledThemes as th (th.name)}
          <button
            class="swatch"
            class:sel={theme.current === th.name}
            title={th.name}
            aria-label={th.name}
            style="background: {th.bg}; border-color: {th.accent}"
            onclick={() => {
              setBundledTheme(th.name);
              theme = { current: th.name, fromSystem: false };
            }}><span style="background: {th.accent}"></span></button
          >
        {/each}
      </div>
    {/if}
  </section>

  <section>
    <h3>{t("set.shortcuts")}</h3>
    {#if !status.mobile}
      <div class="field">
        <span>{t("set.hotkey")}</span>
        <kbd>{status.omarchy ? "Super ⌥ N" : status.hotkey.replace("Alt", "⌥").replace("+", " ")}</kbd>
      </div>
      {#if status.omarchy || status.platform === "linux"}
        <p class="hint">{t("set.omarchyHotkey")}</p>
        <div class="code">
          <code>{hyprBinding}</code>
          <button onclick={() => copy(hyprBinding)}>{copied === hyprBinding ? t("set.copied") : t("set.copy")}</button>
        </div>
      {/if}
    {/if}
    <table>
      <tbody>
        {#each shortcuts as [keys, label] (label)}
          <tr><td><kbd>{keys}</kbd></td><td>{t(label)}</td></tr>
        {/each}
      </tbody>
    </table>
  </section>

  {#if !status.mobile}
    <section>
      <h3>{t("set.ai")}</h3>
      <p class="hint">{t("set.aiHint")}</p>
      <div class="code">
        <code>{mcpCommand}</code>
        <button onclick={() => copy(mcpCommand)}>{copied === mcpCommand ? t("set.copied") : t("set.copy")}</button>
      </div>
      <p class="hint mono">omanote-cli list · show · search · new · append · edit · move · sync</p>
    </section>
  {/if}

  <section>
    <h3>{t("set.account")}</h3>
    {#if status.local}
      <p class="hint">{t("set.localOnly")}</p>
      <button class="accent wide" onclick={onConnect}>{t("set.connect")}</button>
    {:else}
      <div class="field"><span>{t("set.server")}</span><span class="muted">{status.server_url}</span></div>
      <div class="field"><span>{t("setup.email")}</span><span class="muted">{status.email}</span></div>
      <div class="field">
        <span>{t("set.e2ee")}</span><span class="muted">{status.e2ee ? t("set.on") : t("set.off")}</span>
      </div>
      <button
        class="danger"
        onclick={async () => {
          if (confirm(t("set.logoutConfirm"))) {
            await api.logout();
            onLogout();
          }
        }}>{t("set.logout")}</button
      >
    {/if}
  </section>
</div>

<style>
  .scrim {
    position: fixed;
    z-index: 10;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
  }
  .panel {
    position: fixed;
    z-index: 11;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(30rem, 100vw);
    background: var(--bg-elev);
    border-left: 1px solid var(--accent);
    overflow-y: auto;
    padding: calc(10px + env(safe-area-inset-top)) 16px calc(16px + env(safe-area-inset-bottom));
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    margin: 0;
    font-size: 1rem;
    color: var(--accent);
  }
  h3 {
    margin: 0 0 6px;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  section {
    padding: 14px 0;
    border-bottom: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .hint {
    margin: 0;
    font-size: 0.78rem;
    color: var(--muted);
    line-height: 1.45;
  }
  .mono {
    font-size: 0.72rem;
  }
  .muted {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .field {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    font-size: 0.85rem;
  }
  .list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
  }
  .row {
    display: flex;
    justify-content: space-between;
    text-align: left;
    border-radius: 0;
    padding: 6px 9px;
  }
  .row.sel {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 700;
  }
  .row.whole {
    border-bottom: 1px solid var(--line);
    font-weight: 700;
  }
  .badge {
    color: var(--accent);
    border: 1px solid var(--accent);
    padding: 0 5px;
    margin-right: 6px;
    font-size: 0.72rem;
  }
  .inline {
    display: flex;
    gap: 6px;
  }
  .accent {
    background: var(--accent);
    color: var(--bg);
    font-weight: 700;
  }
  .accent:disabled {
    opacity: 0.4;
  }
  select {
    font: inherit;
    color: inherit;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 4px 6px;
  }
  .stepper {
    display: inline-flex;
    align-items: center;
    gap: 4px;
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
  table {
    border-collapse: collapse;
    font-size: 0.8rem;
  }
  td {
    padding: 3px 0;
    vertical-align: top;
  }
  td:first-child {
    width: 9.5rem;
  }
  kbd {
    font-family: var(--font);
    font-size: 0.78rem;
    color: var(--accent);
    white-space: nowrap;
  }
  .code {
    display: flex;
    gap: 6px;
    align-items: stretch;
  }
  .code code {
    flex: 1;
    font-size: 0.74rem;
    background: var(--bg);
    border: 1px solid var(--line);
    padding: 6px 8px;
    overflow-x: auto;
    white-space: nowrap;
  }
  .wide {
    align-self: flex-start;
    padding: 7px 12px;
  }
  .danger {
    align-self: flex-start;
    color: var(--danger);
    border: 1px solid var(--danger);
    margin-top: 4px;
  }
</style>
