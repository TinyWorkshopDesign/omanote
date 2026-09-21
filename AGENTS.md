# Omanote — guide for AI agents and contributors

Omanote is a quick-notes scratchpad for Linux, macOS, iOS and Android that syncs
with an unmodified **Joplin Server** and looks like **Omarchy**. Read this before
changing code.

## Project status (2026-09-21)

Owner: Michele Belleri (writes in Italian; UI text and commits are in Italian, code
and comments in English). Working and verified on macOS with a real Joplin Server.

**Done and verified**
- Sync with Joplin Server incl. E2EE (both directions) and conflicts, tested against
  `joplin/server` in Docker with the official Joplin CLI (`crates/omanote-core/examples/sync_e2e.rs`).
- Scratchpad behaviour: first-line keywords, lists with `/x` and `[] `, double Enter ends a
  list, inline math and variables (also in list notes, only `code` notes are raw), timers,
  swipe navigation, auto-deleted empty notes, keyboard shortcuts.
- OCR with Apple Vision (tested in the real app); tesseract/grim/slurp path for Linux.
- 8 LTR languages; Omarchy look (JetBrains Mono, 22 bundled themes, live system theme).
- Navigation: tree panel (`Sidebar.svelte`: notebook → folders → notes, expand/collapse,
  current note revealed, per-folder actions, drag a note onto a folder to move it; picking
  a folder limits the note stack to it) and an auto-hiding bottom bar
  (‹ dots › + "+" slot, shown near the bottom edge or for 2 s after moving).
- macOS window: no title bar, traffic lights shown only with the hover menu (58 px bar).
- Icons: `design/icon.svg` (cyber pencil, terminal green `#2bff88`) → `npx tauri icon
  ../design/icon.png` from `app/`; `design/tray.svg` → `app/src-tauri/icons/tray.png`
  (macOS template image). Render SVGs with `@resvg/resvg-js`.
- `omanote-cli` + MCP server tested on real synced data.
- Local mode: "use without a server" creates a local notebook (`Config.local_only`);
  connecting a Joplin Server later uploads those notes instead of resetting the store
  (only switching from one server/account to another resets it). Verified with
  `crates/omanote-core/examples/local_then_sync.rs` + the official Joplin CLI.

**Not verified yet**
- Linux/Omarchy build on real hardware (theme from `colors.toml`, tesseract, grim/slurp,
  `omanote --toggle` Hyprland binding, undecorated window).
- iOS/Android: projects not initialised (`tauri ios init` / `android init`); the Android
  SDK is not installed on the dev Mac.

**Known gaps / next steps**
- Attachments: OCR inserts text only; images are not stored as Joplin resources yet.
- Tags are synced but not shown; no in-app trash view.
- Mobile: timer notifications are not scheduled, so they do not fire while the app is
  suspended; OCR on Android missing (ML Kit plugin planned).
- Search is a linear scan (fine for hundreds of notes; add FTS5 if needed).
- Source of truth: **GitHub** `TinyWorkshopDesign/omanote` (private for now, to be made
  public), branch `main`. Share code between machines with git, never through Syncthing:
  `.git` is excluded from Syncthing on the dev Mac.
- The repo lives in a **Syncthing** folder (`~/Sync/...`) shared with another device.
  Syncthing may drop `*.sync-conflict-*` copies next to sources: SvelteKit fails with
  "Files prefixed with + are reserved" if one appears in `src/routes`. Such copies are
  git-ignored; move them out (they are usually older versions) and keep `target/`,
  `node_modules/`, `.svelte-kit/`, `build/` out of Syncthing. On the dev Mac this is
  done in `~/Sync/.stignore` and `~/Sync/AI/.stignore` (two nested Syncthing folders);
  `.stignore` is local, so other devices need the same lines.
- macOS keychain: each debug rebuild changes the binary signature, so macOS may ask
  again for keychain access; the sync waits on that prompt (the UI must not).
- Release build on the dev Mac: `cd app && PATH=/usr/bin:$PATH npm run tauri build -- --bundles app`.
  A Python `xattr` (from python.org's framework) shadows `/usr/bin/xattr` and lacks `-r`,
  which breaks Tauri's ad-hoc signing step. Install with `ditto` into /Applications.
- In dev (`tauri dev`) the Dock icon is embedded at compile time: after changing icons
  touch `app/src-tauri/build.rs` to rebuild.

## Layout

| Path | What |
| --- | --- |
| `crates/omanote-core` | Rust core: Joplin item format (`item.rs`), E2EE (`e2ee.rs`), server client (`api.rs`), SQLite store (`store.rs`), sync engine (`sync.rs`), shared config (`config.rs`), keychain (`secrets.rs`) |
| `crates/omanote-cli` | `omanote-cli`: terminal commands and the MCP server (`mcp.rs`) over the same database |
| `app/src-tauri` | Tauri 2 shell: commands (`lib.rs`), timers (`timer.rs`), OCR (`ocr.rs`), Omarchy theme (`theme.rs`) |
| `app/src` | Svelte 5 UI: editor (`lib/editor.ts`), inline math and note modes (`lib/calc.ts`), translations (`lib/i18n.svelte.ts`), themes (`lib/theme.ts`, generated `themes.css`) |
| `tools/gen-themes.py` | Regenerates `app/src/themes.css` from the Omarchy repo |

## Commands

```bash
cargo test                          # all Rust tests (core, CLI/MCP, app: timer, theme, OCR)
npm run --prefix app test           # calculator and note-mode tests
npm run --prefix app check          # Svelte/TypeScript type check
npm run --prefix app dev            # UI in a browser with mock data (src/lib/mock.ts)
npm run --prefix app tauri dev      # desktop app
cargo run -p omanote-cli -- --help  # CLI
```

## Invariants — do not break

- **Joplin Server is never modified.** Omanote only reads `info.json` and writes
  `<id>.md` items through the public API. Never upload `info.json`, never create or
  change master keys, never touch `locks/`, `.resource/` or `temp/` except through the API.
- **Byte-compatible item format.** `RawItem` keeps unknown properties verbatim; keep
  `serialize`/`parse` symmetric with `BaseItem.serialize/unserialize` in `@joplin/lib`.
- **E2EE**: new data is written with `StringV1` (AES-256-GCM, PBKDF2-SHA512, 3 rounds)
  under the active master key. Compatibility vectors in
  `crates/omanote-core/tests/joplin_vectors.json` come from Joplin's own JS crypto:
  keep those tests green.
- **Conflicts** follow Joplin: remote wins, the local version becomes a separate note
  with `is_conflict = 1`.
- **Deleting** sets `deleted_time` (Joplin trash). Permanent deletion is explicit.
- **Notes are plain Markdown.** Line 1 is the Joplin title. Checklists are stored as
  `- [ ] ` / `- [x] `; the editor only *renders* them. Keywords (`list`, `sum`, …) live in
  the text, so every Joplin client sees the same content.
- **One database, several processes.** The app, `omanote-cli` and the MCP server share
  `<data dir>/app.omanote/omanote.sqlite` (WAL + busy timeout). The app notices external
  writes through `PRAGMA data_version`.
- **Errors to the UI** are codes (`CODE` or `CODE|detail`) translated in `i18n.svelte.ts`.
- **Languages**: left-to-right only. Add strings to the `en` dictionary first; other
  languages fall back to English.
- **Shortcuts** (`Mod` = ⌘ on Apple, Ctrl elsewhere). Omarchy binds
  almost everything to Super, so avoid Super and `Alt+Tab` inside the app. On Wayland the
  global hotkey is a Hyprland binding calling `omanote --toggle`.
- **Omarchy theme**: on Omarchy the palette comes from
  `~/.config/omarchy/current/theme/colors.toml` and follows theme changes live; the
  bundled themes are only a fallback for other platforms.

## Debugging the real webview

`npm run tauri dev` builds in debug mode, where the webview forwards JS errors to the
terminal as `[webview] …` lines and runs `<data dir>/debug-eval.js` once if the file
appears (then deletes it); `window.__omanoteLog(msg)` prints to the same log. Both
exist only with `debug_assertions`. Keep diagnostics read-only: the app may be
connected to the user's real Joplin account.

## Using Omanote from an agent

```bash
claude mcp add omanote -- omanote-cli mcp     # MCP tools: list/search/read/create/update/append/move/trash/sync
omanote-cli new --folder Lavoro "Riunione\n- [ ] slide"
omanote-cli search budget --json
```

Tool descriptions and the server `instructions` in `crates/omanote-cli/src/mcp.rs`
explain the note format to the model; keep them accurate when behaviour changes.
