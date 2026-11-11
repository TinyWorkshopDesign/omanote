<script lang="ts">
  // Settings, organised by category: a list on the left and the pane on the
  // right, or (narrow screens) the list first and the pane pushed over it.
  import { icons } from "../icons";
  import { api, type Folder, type Status, when } from "../api";
  import { LANGUAGES, MOD, i18n, setLang, t, type Key, type Lang } from "../i18n.svelte";
  import { bundledThemes, setBundledTheme, type ThemeState } from "../theme";
  import { FONT_MAX, FONT_MIN, prefs } from "../prefs.svelte";
  import { isCleartextRemote } from "../serverUrl";

  let {
    status,
    folders,
    theme = $bindable(),
    syncState,
    lastSync,
    narrow,
    onSync,
    onRootChanged,
    onLogout,
    onConnect,
    onClose,
  }: {
    status: Status;
    folders: Folder[];
    theme: ThemeState;
    syncState: "idle" | "syncing" | "error";
    lastSync: number;
    /** Phone-width window: one level at a time. */
    narrow: boolean;
    onSync: () => void;
    onRootChanged: () => void;
    onLogout: () => void;
    /** Local mode: open the Joplin Server login. */
    onConnect: () => void;
    onClose: () => void;
  } = $props();

  type Cat = "general" | "appearance" | "account" | "notebooks" | "keyboard" | "ai" | "repair" | "about";
  const cats = $derived(
    (
      [
        ["general", "set.general", icons.general],
        ["appearance", "set.appearance", icons.appearance],
        ["account", "set.accountSync", icons.account],
        ["notebooks", "set.notebooks", icons.notebook],
        ["keyboard", "set.keyboard", icons.keyboard],
        ["ai", "set.ai", icons.robot],
        ["repair", "set.repair", icons.wrench],
        ["about", "set.about", icons.info],
      ] as [Cat, Key, string][]
    ).filter(([c]) => !(c === "ai" && status.mobile) && !(c === "repair" && status.local) && !(c === "keyboard" && status.mobile && !hasKeyboard)),
  );
  // iPad with a keyboard still benefits from the shortcut list; phones do not.
  const hasKeyboard = typeof matchMedia !== "undefined" && matchMedia("(pointer: fine)").matches;

  let cat = $state<Cat>("general");
  /** Narrow screens: false = the category list, true = the open pane. */
  let paneOpen = $state(false);
  let copied = $state("");
  let newNotebook = $state("");
  let busy = $state<"" | "resync" | "reupload">("");
  let version = $state("");

  $effect(() => {
    import("@tauri-apps/api/app")
      .then((m) => m.getVersion())
      .then((v) => (version = v))
      .catch(() => (version = "dev"));
  });

  function choose(c: Cat) {
    cat = c;
    paneOpen = true;
  }

  const catLabel = $derived(cats.find(([c]) => c === cat)?.[1] ?? "set.general");

  async function copy(text: string) {
    await navigator.clipboard?.writeText(text);
    copied = text;
    setTimeout(() => (copied = ""), 1500);
  }

  async function openLink(url: string) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      window.open(url, "_blank", "noopener");
    }
  }

  // ---------------------------------------------------------------- general

  function changeLang(l: Lang) {
    setLang(l);
    void api.setLanguage(l);
  }

  const notebooks = $derived(folders.filter((f) => f.parent_id === ""));
  const homeName = $derived(folders.find((f) => f.id === status.root_folder_id)?.title ?? "Omanote");
  const homeChoices = $derived(
    status.whole_joplin ? notebooks : folders.filter((f) => f.id === status.root_folder_id || isUnder(f.id, status.root_folder_id)),
  );

  function isUnder(id: string, root: string): boolean {
    let p = folders.find((f) => f.id === id)?.parent_id;
    while (p) {
      if (p === root) return true;
      p = folders.find((f) => f.id === p)?.parent_id;
    }
    return false;
  }

  async function chooseHome(id: string) {
    if (status.whole_joplin) {
      await api.setNotesHome(id);
      onRootChanged();
    }
  }

  const hyprBinding = "bindd = SUPER ALT, N, Omanote, exec, omanote --toggle";

  // -------------------------------------------------------------- notebooks

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

  // ----------------------------------------------------------------- repair

  async function resync() {
    busy = "resync";
    try {
      await api.resyncAll();
      onRootChanged();
    } finally {
      busy = "";
    }
  }

  async function reupload() {
    if (!confirm(t("set.reuploadConfirm"))) return;
    busy = "reupload";
    try {
      const r = await api.reuploadLocal();
      if (r)
        alert(
          t("set.reuploadDone", { n: String(r.uploaded) }) +
            (r.errors.length ? `\n${t("set.reuploadErrors", { n: String(r.errors.length) })}` : ""),
        );
      onRootChanged();
    } finally {
      busy = "";
    }
  }

  async function logout() {
    if (!confirm(t("set.logoutConfirm"))) return;
    await api.logout();
    onLogout();
  }

  // --------------------------------------------------------------------- AI

  // The app binary itself runs the MCP server (`<path> mcp`) and the terminal
  // commands, so nothing else has to be installed.
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
  const cliExample = $derived(`${sh(cli)} search budget --json`);
  let agentIndex = $state(0);
  const agent = $derived(agents[agentIndex]);

  // ---------------------------------------------------------------- keyboard

  // Ctrl instead of ⌘ outside Apple platforms. None of them collide with
  // Omarchy, whose own bindings all use the Super key.
  const shortcutGroups: [Key, [string, Key][]][] = [
    [
      "set.keyNotes",
      [
        [`${MOD} N`, "key.new"],
        [`${MOD} D`, "key.delete"],
        [`${MOD} E`, "key.moveNote"],
        [`${MOD} F`, "key.search"],
        [`${MOD} K`, "key.quickOpen"],
        ["↑  ↓", "key.listNav"],
        [`${MOD} S`, "key.syncKey"],
      ],
    ],
    [
      "set.keyEditing",
      [
        [`${MOD} ⇧ K`, "key.check"],
        [`${MOD} ⇧ M`, "key.cycle"],
        [`${MOD} B  I  U`, "key.format"],
        [`${MOD} ⇧ X`, "key.strike"],
        [`${MOD} ⇧ H`, "key.heading"],
        [`${MOD} /`, "key.comment"],
        ["⌥ ↑  ⌥ ↓", "key.moveLine"],
        [`${MOD} ⇧ D`, "key.draw"],
        [`${MOD} ⇧ O`, "key.capture"],
      ],
    ],
    [
      "set.keyWindow",
      [
        [`${MOD} \\`, "key.folders"],
        [`${MOD} ⇧ \\`, "key.focus"],
        [`${MOD} +  ${MOD} −`, "key.zoom"],
        [`${MOD} P`, "key.pin"],
        [`${MOD} W`, "key.hide"],
        [`${MOD} ,`, "nav.settings"],
      ],
    ],
  ];
</script>

<div class="scrim" onclick={onClose} role="presentation"></div>
<div class="sheet" class:narrow role="dialog" aria-label={t("set.title")}>
  {#if !narrow || !paneOpen}
    <nav class="cats" aria-label={t("set.title")}>
      <header>
        <h2>{t("set.title")}</h2>
        {#if narrow}<button class="x" onclick={onClose} aria-label={t("act.close")}>{@html icons.close}</button>{/if}
      </header>
      {#each cats as [c, label, glyph] (c)}
        <button class="cat" class:sel={!narrow && cat === c} onclick={() => choose(c)}>
          <span class="glyph">{@html glyph}</span>
          <span class="label">{t(label)}</span>
          {#if narrow}<span class="chev">{@html icons.next}</span>{/if}
        </button>
      {/each}
    </nav>
  {/if}

  {#if !narrow || paneOpen}
    <div class="pane">
      <header>
        {#if narrow}
          <button class="back" onclick={() => (paneOpen = false)}>{@html icons.prev}<span>{t("set.title")}</span></button>
        {:else}
          <h2>{t(catLabel)}</h2>
          <button class="x" onclick={onClose} aria-label={t("act.close")}>{@html icons.close}</button>
        {/if}
      </header>
      {#if narrow}<h2 class="pane-title">{t(catLabel)}</h2>{/if}

      <div class="body">
        {#if cat === "general"}
          <div class="group">
            <div class="row">
              <span class="label">{t("set.language")}</span>
              <select value={i18n.lang} onchange={(e) => changeLang((e.currentTarget as HTMLSelectElement).value as Lang)}>
                {#each Object.entries(LANGUAGES) as [code, name] (code)}
                  <option value={code}>{name}</option>
                {/each}
              </select>
            </div>
            <div class="row">
              <span class="label">{t("set.launch")}</span>
              <span class="seg" role="radiogroup">
                <button role="radio" aria-checked={prefs.launch === "last"} class:on={prefs.launch === "last"} onclick={() => (prefs.launch = "last")}
                  >{t("set.launchLast")}</button
                >
                <button role="radio" aria-checked={prefs.launch === "new"} class:on={prefs.launch === "new"} onclick={() => (prefs.launch = "new")}
                  >{t("set.launchNew")}</button
                >
              </span>
            </div>
            <p class="hint">{t("set.launchHint")}</p>
            <div class="row">
              <span class="label">{t("set.toolbar")}</span>
              <span class="seg" role="radiogroup">
                <button role="radio" aria-checked={!prefs.autoHideToolbar} class:on={!prefs.autoHideToolbar} onclick={() => (prefs.autoHideToolbar = false)}
                  >{t("set.toolbarAlways")}</button
                >
                <button role="radio" aria-checked={prefs.autoHideToolbar} class:on={prefs.autoHideToolbar} onclick={() => (prefs.autoHideToolbar = true)}
                  >{t("set.toolbarAuto")}</button
                >
              </span>
            </div>
            <p class="hint">{t("set.toolbarHint")}</p>
          </div>

          <div class="group">
            <div class="row">
              <span class="label">{t("set.notesHome")}</span>
              {#if status.whole_joplin}
                <select value={status.root_folder_id} onchange={(e) => void chooseHome((e.currentTarget as HTMLSelectElement).value)}>
                  {#each homeChoices as f (f.id)}
                    <option value={f.id}>{f.title}</option>
                  {/each}
                </select>
              {:else}
                <span class="value">{homeName}</span>
              {/if}
            </div>
            <p class="hint">{t(status.whole_joplin ? "set.notesHomeHint" : "set.notesHomeFixed")}</p>
          </div>

          {#if !status.mobile}
            <div class="group">
              <div class="row">
                <span class="label">{t("set.hotkey")}</span>
                <kbd>{status.omarchy ? "Super ⌥ N" : status.hotkey.replace("CommandOrControl", MOD).replace("Alt", "⌥").replaceAll("+", " ")}</kbd>
              </div>
              {#if status.omarchy || status.platform === "linux"}
                <p class="hint">{t("set.omarchyHotkey")}</p>
                <div class="code">
                  <code>{hyprBinding}</code>
                  <button onclick={() => copy(hyprBinding)}>{copied === hyprBinding ? t("set.copied") : t("set.copy")}</button>
                </div>
              {/if}
            </div>
          {/if}
        {:else if cat === "appearance"}
          <div class="group">
            <div class="row">
              <span class="label">{t("set.theme")}</span>
              <span class="value">{theme.current}</span>
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
          </div>
          <div class="group">
            <div class="row">
              <span class="label">{t("set.textSize")}</span>
              <span class="stepper">
                <button onclick={() => (prefs.fontSize = Math.max(FONT_MIN, prefs.fontSize - 1))} aria-label="−">−</button>
                <span>{prefs.fontSize}px</span>
                <button onclick={() => (prefs.fontSize = Math.min(FONT_MAX, prefs.fontSize + 1))} aria-label="+">+</button>
              </span>
            </div>
            <p class="sample" style="font-size: {prefs.fontSize}px">{t("set.sample")}</p>
            <div class="row">
              <span class="label">{t("set.sort")}</span>
              <span class="seg" role="radiogroup">
                <button role="radio" aria-checked={prefs.sort === "edited"} class:on={prefs.sort === "edited"} onclick={() => (prefs.sort = "edited")}
                  >{t("list.sortEdited")}</button
                >
                <button role="radio" aria-checked={prefs.sort === "title"} class:on={prefs.sort === "title"} onclick={() => (prefs.sort = "title")}
                  >{t("list.sortTitle")}</button
                >
              </span>
            </div>
          </div>
        {:else if cat === "account"}
          {#if status.local}
            <div class="group">
              <p class="hint">{t("set.localOnly")}</p>
              <button class="primary" onclick={onConnect}>{t("set.connect")}</button>
            </div>
          {:else}
            <div class="group">
              <div class="row"><span class="label">{t("set.server")}</span><span class="value">{status.server_url}</span></div>
              {#if isCleartextRemote(status.server_url)}<p class="hint warn">{t("setup.cleartext")}</p>{/if}
              <div class="row"><span class="label">{t("setup.email")}</span><span class="value">{status.email}</span></div>
              <div class="row"><span class="label">{t("set.e2ee")}</span><span class="value">{status.e2ee ? t("set.on") : t("set.off")}</span></div>
            </div>
            <div class="group">
              <div class="row">
                <span class="label">{t("set.lastSync")}</span>
                <span class="value"
                  >{syncState === "syncing" ? t("sync.syncing") : syncState === "error" ? t("sync.error") : lastSync ? when(lastSync, i18n.lang) : t("sync.never")}</span
                >
              </div>
              <button class="outline" disabled={syncState === "syncing"} onclick={onSync}>{@html icons.sync} {t("act.sync")}</button>
            </div>
            <div class="group">
              <button class="destructive" onclick={() => void logout()}>{t("set.logout")}</button>
              <p class="hint">{t("set.logoutHint")}</p>
            </div>
          {/if}
        {:else if cat === "notebooks"}
          <div class="group">
            <p class="hint">{t("set.notebookHint")}</p>
            <div class="choices">
              {#if !status.local}
                <button class="choice" class:sel={status.whole_joplin} onclick={() => void chooseWhole()}>
                  <span class="glyph">{@html icons.allNotes}</span>
                  <span class="label">{t("set.wholeJoplin")}</span>
                  <span class="count">{notebooks.length}</span>
                  {#if status.whole_joplin}<span class="tick">{@html icons.check}</span>{/if}
                </button>
              {/if}
              {#each notebooks as f (f.id)}
                {@const sel = !status.whole_joplin && status.root_folder_id === f.id}
                <button class="choice" class:sel onclick={() => void chooseRoot(f.id)}>
                  <span class="glyph">{#if f.icon}{f.icon}{:else}{@html icons.notebook}{/if}</span>
                  <span class="label">{f.title}</span>
                  <span class="count">{f.note_count}</span>
                  {#if sel}<span class="tick">{@html icons.check}</span>{/if}
                </button>
              {/each}
            </div>
            {#if status.whole_joplin}<p class="hint">{t("set.wholeJoplinHint", { name: homeName })}</p>{/if}
          </div>
          <div class="group">
            <span class="label">{t("setup.createNotebook")}</span>
            <div class="inline">
              <input placeholder={t("setup.notebookName")} bind:value={newNotebook} onkeydown={(e) => e.key === "Enter" && createRoot()} />
              <button class="primary" onclick={createRoot} disabled={!newNotebook.trim()}>{@html icons.plus}</button>
            </div>
          </div>
        {:else if cat === "keyboard"}
          {#each shortcutGroups as [group, list] (group)}
            <div class="group">
              <span class="group-title">{t(group)}</span>
              <table>
                <tbody>
                  {#each list as [keys, label] (label)}
                    <tr><td><kbd>{keys}</kbd></td><td>{t(label)}</td></tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/each}
        {:else if cat === "ai"}
          <div class="group">
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
          </div>
          <div class="group">
            <p class="hint">{t("set.agentCli")}</p>
            <div class="code">
              <code>{cliExample}</code>
              <button onclick={() => copy(cliExample)}>{copied === cliExample ? t("set.copied") : t("set.copy")}</button>
            </div>
            <p class="hint mono">list · show · search · new · append · edit · move · trash · sync · mcp</p>
          </div>
        {:else if cat === "repair"}
          <div class="group">
            <div class="row">
              <span class="label">{t("set.resync")}</span>
              <button class="outline" disabled={!!busy} onclick={() => void resync()}>{busy === "resync" ? t("setup.wait") : t("set.run")}</button>
            </div>
            <p class="hint">{t("set.resyncHint")}</p>
          </div>
          <div class="group">
            <div class="row">
              <span class="label">{t("set.reupload")}</span>
              <button class="outline" disabled={!!busy} onclick={() => void reupload()}>{busy === "reupload" ? t("setup.wait") : t("set.run")}</button>
            </div>
            <p class="hint">{t("set.reuploadHint")}</p>
          </div>
          {#if status.data_dir}
            <div class="group">
              <div class="row"><span class="label">{t("set.errorsLog")}</span></div>
              <div class="code">
                <code>{status.data_dir}/sync-errors.log</code>
                <button onclick={() => copy(`${status.data_dir}/sync-errors.log`)}
                  >{copied === `${status.data_dir}/sync-errors.log` ? t("set.copied") : t("set.copy")}</button
                >
              </div>
            </div>
          {/if}
        {:else if cat === "about"}
          <div class="group about">
            <span class="appname">Omanote</span>
            <span class="hint">{t("app.tagline")}</span>
          </div>
          <div class="group">
            <div class="row"><span class="label">{t("set.version")}</span><span class="value">{version}</span></div>
            <div class="row"><span class="label">{t("set.license")}</span><span class="value">MIT</span></div>
            <div class="row">
              <span class="label">{t("set.source")}</span>
              <button class="link" onclick={() => void openLink("https://github.com/TinyWorkshopDesign/omanote")}>github.com/TinyWorkshopDesign/omanote</button>
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .scrim {
    position: fixed;
    z-index: 10;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
  }
  .sheet {
    position: fixed;
    z-index: 11;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(52rem, calc(100vw - 32px));
    height: min(38rem, calc(100dvh - 32px));
    display: flex;
    background: var(--bg);
    border: 1px solid var(--accent);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.45);
  }
  .sheet.narrow {
    inset: 0;
    transform: none;
    width: auto;
    height: auto;
    border: 0;
    padding-top: env(safe-area-inset-top);
  }
  .cats {
    flex: 0 0 13.5rem;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 0 6px 10px;
    background: var(--bg-elev);
    border-right: 1px solid var(--line);
    overflow-y: auto;
  }
  .narrow .cats {
    flex: 1;
    border-right: 0;
    background: var(--bg);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: var(--bar-h);
    padding: 0 8px;
  }
  h2 {
    margin: 0;
    font-size: 1.05rem;
    color: var(--accent);
  }
  .pane-title {
    padding: 0 16px 6px;
    font-size: 1.35rem;
  }
  .x {
    color: var(--muted);
    font-size: var(--ico-l, 22px);
    min-width: 40px;
    min-height: 40px;
  }
  .x:hover {
    color: var(--accent);
  }
  .cat {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 38px;
    padding: 0 10px;
    text-align: left;
    border-left: 2px solid transparent;
  }
  .narrow .cat {
    min-height: 48px;
    border-bottom: 1px solid var(--line);
  }
  .cat:hover {
    background: color-mix(in srgb, var(--panel) 60%, transparent);
  }
  .cat.sel {
    background: var(--panel);
    border-left-color: var(--accent);
    color: var(--accent);
  }
  .cat .glyph {
    width: 22px;
    text-align: center;
    color: var(--accent);
    font-size: var(--ico-s, 15px);
  }
  .cat .label {
    flex: 1;
  }
  .chev {
    color: var(--muted);
  }
  .pane {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .back {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: 44px;
    color: var(--accent);
  }
  .back :global(.ico) {
    font-size: var(--ico-l, 22px);
  }
  .body {
    flex: 1;
    overflow-y: auto;
    padding: 6px 18px calc(18px + env(safe-area-inset-bottom));
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--line);
    background: var(--bg-elev);
  }
  .group-title {
    font-size: 0.72rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    min-height: 32px;
  }
  .label {
    font-size: 0.88rem;
  }
  .value {
    color: var(--muted);
    font-size: 0.85rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .hint {
    margin: -2px 0 0;
    font-size: 0.76rem;
    color: var(--muted);
    line-height: 1.45;
  }
  .hint.warn {
    color: var(--danger);
  }
  .mono {
    font-size: 0.72rem;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--line);
    flex: none;
  }
  .seg button {
    padding: 4px 10px;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .seg button + button {
    border-left: 1px solid var(--line);
  }
  .seg button.on {
    background: var(--accent);
    color: var(--bg);
  }
  select {
    font: inherit;
    font-size: 0.85rem;
    color: inherit;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 4px 6px;
    max-width: 55%;
  }
  .stepper {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    font-variant-numeric: tabular-nums;
  }
  .stepper button {
    padding: 2px 10px;
    border: 1px solid var(--line);
  }
  .sample {
    margin: 0;
    padding: 8px 10px;
    border: 1px dashed var(--line);
    line-height: 1.5;
  }
  .themes {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(34px, 1fr));
    gap: 6px;
  }
  .swatch {
    height: 30px;
    border: 2px solid;
    padding: 0;
    display: grid;
    place-items: center;
  }
  .swatch span {
    width: 10px;
    height: 10px;
  }
  .swatch.sel {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
  .choices {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 36px;
    padding: 0 10px;
    text-align: left;
    border-bottom: 1px solid var(--line);
  }
  .choice:last-child {
    border-bottom: 0;
  }
  .choice:hover {
    background: var(--panel);
  }
  .choice.sel {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .choice .glyph {
    width: 20px;
    text-align: center;
  }
  .choice .label {
    flex: 1;
  }
  .count {
    color: var(--muted);
    font-size: 0.78rem;
  }
  .tick {
    color: var(--accent);
  }
  .inline {
    display: flex;
    gap: 6px;
  }
  .inline input {
    flex: 1;
  }
  .primary {
    background: var(--accent);
    color: var(--bg);
    font-weight: 700;
    padding: 7px 12px;
  }
  .primary:disabled {
    opacity: 0.4;
  }
  .outline {
    border: 1px solid var(--accent);
    color: var(--accent);
    padding: 5px 12px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    flex: none;
  }
  .outline:disabled {
    opacity: 0.4;
  }
  .destructive {
    border: 1px solid var(--danger);
    color: var(--danger);
    padding: 7px 12px;
  }
  .link {
    color: var(--accent);
    font-size: 0.82rem;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .about {
    align-items: center;
    text-align: center;
    padding: 20px;
  }
  .appname {
    font-size: 1.3rem;
    font-weight: 700;
    color: var(--accent);
  }
  table {
    border-collapse: collapse;
    font-size: 0.84rem;
  }
  td {
    padding: 4px 0;
    vertical-align: top;
  }
  td:first-child {
    width: 9.5em;
    white-space: nowrap;
  }
  kbd {
    font-family: var(--font);
    font-size: 0.8rem;
    color: var(--accent);
  }
  .agents {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
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
  .code {
    display: flex;
    gap: 6px;
    align-items: stretch;
  }
  .code code {
    flex: 1;
    min-width: 0;
    font-size: 0.74rem;
    background: var(--bg);
    border: 1px solid var(--line);
    padding: 6px 8px;
    overflow-x: auto;
    white-space: nowrap;
  }
  .code code.block {
    white-space: pre;
  }
  .code button {
    padding: 0 10px;
    border: 1px solid var(--line);
    font-size: 0.78rem;
  }
</style>
