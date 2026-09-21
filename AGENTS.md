# Omanote — guide for AI agents and contributors

Omanote is a quick-notes scratchpad for Linux, macOS, iOS and Android that syncs
with an unmodified **Joplin Server** and looks like **Omarchy**. Read this before
changing code.

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
