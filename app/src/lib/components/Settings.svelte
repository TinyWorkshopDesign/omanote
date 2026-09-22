<script lang="ts">
  import { icons } from "../icons";
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
  let resyncing = $state(false);

  async function resync() {
    resyncing = true;
    try {
      await api.resyncAll();
      onRootChanged();
    } finally {
      resyncing = false;
    }
  }
  let reuploading = $state(false);

  async function reupload() {
    if (!confirm(t("set.reuploadConfirm"))) return;
    reuploading = true;
    try {
      const r = await api.reuploadLocal();
      if (r) alert(t("set.reuploadDone", { n: String(r.uploaded) }));
      onRootChanged();
    } finally {
      reuploading = false;
    }
  }
  let copied = $state("");

  const notebooks = $derived(folders.filter((f) => f.parent_id === ""));
  const hyprBinding = "bindd = SUPER ALT, N, Omanote, exec, omanote --toggle";

  // AI agents: the app binary itself runs the MCP server (`<path> mcp`) and the
  // terminal commands, so nothing else has to be installed.
  let cli = $state("omanote");
  $effect(() => {
    if (!status.mobile) api.cliPath().then((p) => (cli = p)).catch(() => {});
  });
  const sh = (p: string) => (/^[\w@%+=:,./-]+$/.test(p) ? p : `'${p.replace(/'/g, `'\\''`)}'`);
  interface Agent {
    name: string;
    /** "run": a terminal command; otherwise the file to paste the snippet into. */
    where: "run" | string;
    text: string;
  }
  const agents = $derived.by((): Agent[] => {
    const c = sh(cli);
    const json = JSON.stringify({ mcpServers: { omanote: { command: cli, args: ["mcp"] } } }, null, 2);
    return [
      { name: "Claude Code", where: "run", text: `claude mcp add omanote -- ${c} mcp` },
      { name: "Codex", where: "run", text: `codex mcp add omanote -- ${c} mcp` },
      { name: "Gemini CLI", where: "run", text: `gemini mcp add -s user omanote ${c} mcp` },
      { name: "VS Code", where: "run", text: `code --add-mcp '${JSON.stringify({ name: "omanote", command: cli, args: ["mcp"] })}'` },
      { name: "Cursor", where: "~/.cursor/mcp.json", text: json },
      { name: "Claude Desktop", where: "claude_desktop_config.json", text: json },
      { name: "Hermes", where: "~/.hermes/config.yaml", text: `mcp_servers:\n  omanote:\n    command: ${JSON.stringify(cli)}\n    args: ["mcp"]` },
      { name: "Zed", where: "~/.config/zed/settings.json", text: JSON.stringify({ context_servers: { omanote: { command: cli, args: ["mcp"] } } }, null, 2) },
      { name: t("set.agentOther"), where: "mcp.json", text: json },
    ];
  });
  let agentIndex = $state(0);
  const agent = $derived(agents[agentIndex]);

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

  async function chooseHome(id: string) {
    await api.setNotesHome(id);
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
    <button onclick={onClose} aria-label={t("act.close")}>{@html icons.close}</button>
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
        {@const home = status.whole_joplin && status.root_folder_id === f.id}
        <div class="nb">
          <button class="row" class:sel={!status.whole_joplin && status.root_folder_id === f.id} onclick={() => chooseRoot(f.id)}>
            <span>{f.icon ? `${f.icon} ` : ""}{f.title}</span>
            <span class="muted">{#if home}<span class="badge">{t("set.newNotesHere")}</span>{/if}{f.note_count}</span>
          </button>
          {#if status.whole_joplin && !home}
            <button class="sethome" title={t("ctx.useForNewNotes")} onclick={() => chooseHome(f.id)}>{t("set.newNotesHere")}</button>
          {/if}
        </div>
      {/each}
    </div>
    {#if status.whole_joplin}
      <p class="hint">{t("set.wholeJoplinHint", { name: homeName })}</p>
    {/if}
    <div class="inline">
      <input placeholder={t("setup.notebookName")} bind:value={newNotebook} onkeydown={(e) => e.key === "Enter" && createRoot()} />
      <button class="accent" onclick={createRoot} disabled={!newNotebook.trim()}>{@html icons.plus}</button>
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
      <div class="agents" role="tablist">
        {#each agents as a, i (a.name)}
          <button role="tab" aria-selected={i === agentIndex} class:on={i === agentIndex} onclick={() => (agentIndex = i)}>{a.name}</button>
        {/each}
      </div>
      <p class="hint">{agent.where === "run" ? t("set.agentRun") : t("set.agentPaste", { file: agent.where })}</p>
      <div class="code">
        <code class:block={agent.text.includes("\n")}>{agent.text}</code>
        <button onclick={() => copy(agent.text)}>{copied === agent.text ? t("set.copied") : t("set.copy")}</button>
      </div>
      <p class="hint">{t("set.agentCli")}</p>
      <div class="code">
        <code>{sh(cli)} search budget --json</code>
        <button onclick={() => copy(`${sh(cli)} --help`)}>{copied === `${sh(cli)} --help` ? t("set.copied") : t("set.copy")}</button>
      </div>
      <p class="hint mono">list · show · search · new · append · edit · move · trash · sync · mcp</p>
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
      <div class="field stack">
        <span class="hint">{t("set.resyncHint")}</span>
        <button class="outline" disabled={resyncing} onclick={resync}>{resyncing ? t("setup.wait") : t("set.resync")}</button>
      </div>
      <div class="field stack">
        <span class="hint">{t("set.reuploadHint")}</span>
        <button class="outline" disabled={reuploading} onclick={reupload}>{reuploading ? t("setup.wait") : t("set.reupload")}</button>
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
  .nb {
    position: relative;
    display: flex;
  }
  .nb .row {
    flex: 1;
  }
  .sethome {
    position: absolute;
    right: 40px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 0.72rem;
    padding: 1px 6px;
    color: var(--muted);
    border: 1px solid var(--line);
    background: var(--bg-elev);
    opacity: 0;
  }
  .nb:hover .sethome,
  .sethome:focus-visible {
    opacity: 1;
  }
  .sethome:hover {
    color: var(--accent);
    border-color: var(--accent);
  }
  @media (hover: none) {
    .sethome {
      opacity: 1;
    }
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
  .field.stack {
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }
  .code code.block {
    white-space: pre;
  }
  .agents {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 4px 0;
  }
  .agents button {
    padding: 3px 8px;
    font-size: 0.75rem;
    border: 1px solid var(--line);
    color: var(--muted);
  }
  .agents button.on {
    border-color: var(--accent);
    color: var(--accent);
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
  .outline {
    flex: none;
    border: 1px solid var(--line);
    padding: 4px 10px;
  }
  .outline:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .danger {
    align-self: flex-start;
    color: var(--danger);
    border: 1px solid var(--danger);
    margin-top: 4px;
  }
</style>
