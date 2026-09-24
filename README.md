# Omanote

Quick notes synced with **Joplin Server**, with the look of **Omarchy**. One app for
**Linux, macOS, iOS and Android**, in 8 languages.

The server is never modified: Omanote speaks the same protocol as the official Joplin
clients (sync target version 3), writes the same `<id>.md` items and uses the same
end-to-end encryption. Joplin and Omanote live side by side on the same account.

> **Status: preview.** Verified end to end on macOS against a real Joplin Server with E2EE,
> and on Omarchy 4. iOS and Android builds are not set up yet.

## Features

**Quick notes**
- Opens on a fresh note; `⌘[` / `⌘]` or a two-finger swipe move between notes. Going past
  the newest one starts a new note; a note left empty deletes itself.
- Hidden menu: it shows when the pointer nears the top edge (always visible on touch screens).
- Auto-hiding bottom bar: ‹ one dot per note › and "+" for a new note; it shows near the
  bottom edge or briefly when you switch notes.
- Folder tree panel (notebook → folders → notes): expandable branches, current note
  highlighted, per-folder actions, drag and drop, cut / copy / paste, and a **Trash** with
  restore, delete permanently and empty.
- Keywords on the first line (also translated):
  - `list` / `list: Title`: every line becomes a checkbox; `/x` at the end of a line ticks it;
  - `math`: results show even without `=`;
  - `sum`, `avg`: sum or average of all the numbers in the note;
  - `count`: items, lines, words, characters;
  - `code`: no formatting.
- Checkboxes `[] ` or `- [ ] `, bulleted and numbered lists that continue on Enter, `//` to
  comment out a line.
- **Real Markdown with live preview**: headings `#`…`######`, bold and italic (also across
  lines), `~~strikethrough~~`, `` `code` ``, quotes, links, and `++underline++` as in Joplin.
  HTML from Joplin's rich-text editor renders too: coloured text (`<span style="color:…">`),
  `<img>` images, `&nbsp;`. Markup hides and comes back on the cursor line for editing.
- Calculations: end a line with `=` and the result appears right after it: `2+3*4=`,
  `100 € + vat =`, `20% of 50 =`, `sqrt(16) =`. Without `=` the line stays text. Variables
  (`vat = 22%`) are stored silently; `total =` / `avg =` add up the block above. In notes
  starting with `math` every result shows. Click a result to copy it.
- **Timers** on any line, then Enter: `timer` (stopwatch), `timer 5` / `timer 3:30`
  (countdown), `timer 9am` / `timer 21:15` (until a time), `timer 5: Pasta` (named),
  `timer 25 5` / `timer pomo` (pomodoro), `timer p` / `r` / `s` (pause, restart, stop). Timers
  keep running with the window hidden, notify when done and show in the macOS menu bar.
- **Images**: dropping or pasting an image asks whether to insert it as an **image** (`I`) or
  as **OCR text** (`T`). Images become Joplin attachments (E2EE-encrypted) and show in Joplin
  clients; images attached in Joplin show in Omanote.
- **OCR**: "Text" puts the image's content into the note. `⌘⇧O` captures a screen region.
  Everything runs on the device: Apple Vision on macOS/iOS, tesseract on Linux (already part
  of Omarchy, honours `OMARCHY_OCR_LANGS`).

**Joplin**
- **Working notebook**: Omanote shows one Joplin notebook (its sub-notebooks become the
  folders), or **all of Joplin**. Pick it and change it any time in Settings, so the rest of
  your Joplin stays untouched.
- Full E2EE: reads `KeyV1`, `StringV1`, `FileV1` and the legacy `SJCL1a`, `SJCL1b`, `SJCL3`,
  `SJCL4`; writes `StringV1` like Joplin. Only the old OCB2 method (before 2020) is not
  supported: Joplin desktop offers to upgrade those keys.
- Conflicts are handled like Joplin: the remote version wins and the local one is kept as a
  conflict copy.
- Deleting a note moves it to Joplin's trash.
- Repair tools in Settings: "Resync everything" and "Re-upload local data" (like Joplin
  desktop's, for a server that lost data).

**Omarchy**
- Follows the system theme (`~/.local/state/omarchy/current/theme`) and switches live with the
  theme menu; elsewhere it ships the 22 Omarchy themes. JetBrains Mono font, Nerd Font icons.
- Keyboard shortcuts use `Ctrl` instead of `⌘`: no clash with Omarchy, which uses `Super`.
- Global shortcut on Hyprland, in `~/.config/hypr/bindings.conf`:

  ```
  bindd = SUPER ALT, N, Omanote, exec, omanote --toggle
  ```

  (`omanote --new` opens a new note, `omanote --capture` starts screen OCR,
  `omanote --open <id>` opens a note.)
- **Omarchy bar plugin** (`omarchy-plugin/`): a pencil in the bar; click to jot a note or
  search and open recent ones, right-click to show or hide the app, middle-click for a new
  note. Install it with `omarchy-plugin/install.sh`.
- On macOS the global shortcut is `⌥A`.

**AI friendly**
- The command line works on the same notes as the app: `list`, `show`, `search`, `new`,
  `append`, `edit`, `move`, `trash`, `sync`, with `--json` for scripts. It is both
  `omanote-cli` and the app itself (`omanote <command>`): on macOS Omanote.app is enough,
  `/Applications/Omanote.app/Contents/MacOS/omanote list`.
- `mcp` starts an MCP server (an open standard, not tied to one agent). Settings → AI has a
  ready-to-copy setup for Claude Code, Codex, Gemini CLI, VS Code, Cursor, Claude Desktop,
  Hermes, Zed and other MCP clients, with the right path.
- The app refreshes by itself when an agent changes notes.
- `AGENTS.md` describes the architecture and rules for anyone developing with AI.

## Shortcuts

| | |
| --- | --- |
| `⌘N` | new note |
| `⌘[` `⌘]` | previous / next note |
| `⌘1` / `⌘⇧1` | go to the newest / bring to the top |
| `⌘D` | delete note |
| `⌘F` | search (Enter with no results creates a note) |
| `⌘⇧K` / `⌘⇧M` | tick line / checkbox → bullet → number |
| `⌘B` `⌘I` `⌘U` `⌘⇧X` | bold, italic, underline (`++`), strikethrough |
| `⌘⇧H` / `⌘/` | heading level / comment |
| `⌥↑` `⌥↓` | move line |
| `⌘P` / `⌘W` | keep on top / hide window |
| `⌘+` `⌘−` | text size |
| `⌘\` / `⌘E` | folders / move note |
| `⌘⇧O` / `⌘S` / `⌘,` | screen OCR / sync / settings |
| `Esc` | stop the timer |

On Linux and Windows use `Ctrl` instead of `⌘`.

## How it is built

| Part | Technology |
| --- | --- |
| Core (`crates/omanote-core`) | Rust: Joplin item format, E2EE, server client, SQLite, sync |
| CLI + MCP (`crates/omanote-cli`) | Rust, same database as the app |
| App (`app/src-tauri`) | Tauri 2: system webview, binaries of a few MB |
| UI (`app/src`) | Svelte 5 + CodeMirror 6 |

## Development

```bash
npm install --prefix app
npm run --prefix app tauri dev      # desktop app
npm run --prefix app dev            # UI only, in a browser, with mock data
cargo test                          # Rust tests
npm run --prefix app test           # calculator and note-mode tests
cargo install --path crates/omanote-cli
```

The E2EE tests use vectors generated with Joplin's own JavaScript crypto code.

## Roadmap

Tags, signed iOS and Android builds, OCR on Android, scheduled timer notifications on mobile.

## License

[MIT](LICENSE). Third-party material and its licenses: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
