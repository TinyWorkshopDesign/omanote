# Omanote — guide for AI agents and contributors

Omanote is a quick-notes scratchpad for Linux, macOS, iOS and Android that syncs
with an unmodified **Joplin Server** and looks like **Omarchy**. Read this before
changing code.

## Project status (2026-10-23)

Owner: Michele Belleri (writes in Italian). The repository is **English only**: code,
comments, test data, docs and **commit messages**; Italian lives only in the UI
translations and in tests of translated keywords. Commit only as
`Michele Belleri <23453441+TinyWorkshopDesign@users.noreply.github.com>` (set in this clone's
git config; set it in other clones too): no personal email addresses anywhere in the repo or
its history. Do not name other note apps as models or references (code, docs, commits). Working and verified on macOS with a real Joplin Server.

**Done and verified**
- Sync with Joplin Server incl. E2EE (both directions) and conflicts, tested against
  `joplin/server` in Docker with the official Joplin CLI (`crates/omanote-core/examples/sync_e2e.rs`).
- Scratchpad behaviour: first-line keywords, lists with `/x` and `[] `, double Enter ends a
  list, inline math and variables (also in list notes, only `code` notes are raw; a result
  shows right after a trailing "=" — "2+2=" — while "math" notes show every result; variable
  names may start with a digit and hold several words), currency conversion (`100 € in $ =`,
  ECB rates cached in `fx-rates.json` next to the database and refreshed at most once a day;
  a "1 € = 1.05 $" line overrides them for one note; two different currencies in one sum are
  refused instead of silently added with the left unit), timers,
  swipe navigation, auto-deleted empty notes, keyboard shortcuts.
- Editor rendering: `@codemirror/lang-markdown` (CommonMark + GFM) with a `HighlightStyle`
  for headings/emphasis/code/quotes, plus a live-preview plugin in `editor.ts` that hides
  Markdown marks, renders Joplin rich-text HTML (`<span style="color">` across lines, `<img
  src=":/id" width>`, simple tags, `&nbsp;`/entities, `\` escapes, `++underline++`) and
  inline attachment images (several per line). Raw syntax reappears on the lines holding the
  cursor; images stay visible there too. Underline is `++…++` (Joplin), not `__…__`
  (Markdown bold).
- OCR with Apple Vision (tested in the real app); tesseract/grim/slurp path for Linux.
- 8 LTR languages; Omarchy look (JetBrains Mono, 22 bundled themes, live system theme).
- Navigation: tree panel (`Sidebar.svelte`: notebook → folders → notes, expand/collapse,
  current note revealed; notes *and* folders can be dragged onto folders or the root,
  cut/copy/paste with Mod-X/C/V or the right-click / ⋯ menu (copy duplicates notes and
  whole folders), arrows/Enter/⌫ on the selection; picking a folder limits the note stack
  to it). Tree rows are `div`s: WebKit does not start drags from `<button>`s. The in-app
  clipboard lives in `treeClipboard.svelte.ts` and an auto-hiding bottom bar
  (‹ dots › + "+" slot, shown near the bottom edge or for 2 s after moving).
- macOS window: no title bar, traffic lights shown only with the hover menu (58 px bar).
- Icons: the interface draws Nerd Font glyphs, the way Omarchy draws its bar, menu and
  panels (its `shell/Ui/OpticalGlyph.qml`; the menu names glyphs such as U+F035C for
  "menu"). `tools/gen-symbol-font.py` cuts a subset of Nerd Fonts' Symbols Nerd Font
  (MIT, release and SHA-256 pinned) into `app/static/fonts/omanote-symbols.woff2` and
  writes `app/src/lib/symbols.generated.ts`; `icons.ts` renders those glyphs and
  `app.css` sizes them (`--ico-fill`). Add an icon by naming its glyph in that script
  and rerunning it; `--check` verifies the tree without writing. `tools/gen-icons.py`
  composes the app icon and the tray from the same font's "pencil" glyph on the Omarchy
  logo grid, terminal green `#2bff88` → `npx tauri icon
  ../design/icon.png --ios-color '#040706'` from `app/` (it also writes the iOS icons into
  `gen/apple/Assets.xcassets`; iOS icons must be opaque, so flatten any leftover alpha onto
  `#040706` — the tile's darkest tone — and the iOS icon looks like the macOS one); `design/tray.svg` → `app/src-tauri/icons/tray.png`
  (macOS template image). Render SVGs with `rsvg-convert`. The bar plugin draws the same
  glyph in QML (`omarchy-plugin/PencilIcon.qml`, through `OpticalGlyph`).
- `omanote-cli` + MCP server tested on real synced data.
- Trash (Joplin ≥ 3 semantics, `store.trashed/restore/purge/empty_trash`): a "Cestino" row
  at the bottom of the tree panel lists trashed notes and folders (content of a trashed
  folder nested inside it); restore brings a folder back with its content, a note whose
  folder is gone goes to `root_folder_id`, a folder whose parent is gone to the top level;
  "delete permanently" / "empty trash" delete on the server at the next sync (verified with
  the Joplin CLI). Dropping a tree item on the trash row trashes it.
- Re-upload of local data (Settings, `reupload_local` → `Synchronizer::reupload_all`), like
  Joplin desktop's: download first, then every readable item is marked dirty with
  `sync_time = updated_time` and uploaded (attachment blobs too when the server lacks the
  item); items the server has in the same version are skipped. Items already edited
  locally keep their real `sync_time`, so a change made elsewhere meanwhile still becomes a
  conflict copy instead of being overwritten (unit test `reupload_keeps_local_edits_detectable`).
  Verified by wiping the Docker server (`crates/omanote-core/examples/reupload_e2e.rs`, also
  `edit`/`edit-sync`/`show` modes for the conflict case), without E2EE only.
- Images as Joplin attachments: paste/drop asks "Image or Text (OCR)" (keys I / T / Esc).
  Drops are handled by the editor as DOM files (`dragDropEnabled: false` in tauri.conf.json:
  Tauri's native interception only yields file paths, which images dragged from browsers
  or Photos do not have). UI icons come from `app/src/lib/icons.ts` (see Icons above), never
  emoji;
  images become resources (`store.add_resource`, blob uploaded to `.resource/<id>` before
  the item, FileV1-encrypted under E2EE, `encryption_blob_encrypted` set) and render in the
  editor from `![name](:/id)` lines; remote blobs are fetched on demand
  (`Synchronizer::fetch_resource`, served via Tauri's asset protocol scoped to
  `$APPDATA/resources`). Verified both ways against Joplin CLI with identical SHA-256
  (`crates/omanote-core/examples/resources_e2e.rs`). Images never go on line 1 (title).
- Handwriting and sketches (`lib/components/Sketch.svelte`, header button and Mod-Shift-D):
  one Omarchy toolbar over two engines. Tools: ballpoint pen, pencil, highlighter (and
  fountain pen on iPadOS), a pixel eraser and a stroke eraser (tap the eraser again to
  switch), 3 sizes per tool, 4 theme colours plus any colour from the system picker
  (`<input type="color">`; the last 4 picks are kept in localStorage `omanote.sketchColors`),
  undo/redo (Mod-Z, Mod-Shift-Z), clear, keys P/B/H/E. "Insert" stores the drawing, cropped
  to its strokes on the theme background at up to 2x, as a PNG attachment through the same
  `add_image` path as pasted images, written as `<img src=":/id" alt="sketch-….png"
  width="<css px>">` so it shows at the size it was drawn in every Joplin client. "Text"
  renders black ink on white at 2x and runs it through the existing OCR (`ocr_image`).
  - Web engine (macOS, Linux, everything but iPadOS): Pointer Events with coalesced points;
    strokes are drawn as filled outlines by `perfect-freehand` (input smoothing, width from
    pen pressure or, for mice and fingers, from speed); finished strokes are cached in an
    offscreen canvas. The pixel eraser is a stroke painted with `destination-out`; export
    paints the strokes on a transparent layer first so it never cuts the background. The
    eraser end of a pen (`buttons & 32`) erases; once a pen has touched the canvas, fingers
    no longer draw (palm rejection). Verified in Chrome with emulated pen input.
  - Native engine (iPadOS, `crates/tauri-plugin-pencil`, JS side `lib/pencil.ts`): a
    transparent `PKCanvasView` laid over the paper area of the page; the web toolbar drives
    it (`set_tool`, `undo`, `export`, …) and it reports `state`, `tool` and `palette` events.
    It handles the Pencil's double tap and Apple Pencil Pro squeeze as set in the system
    settings (switch to eraser / previous tool, or open the web contextual palette next to
    the hovering tip, during which the canvas stops taking input), barrel roll on the
    fountain pen, PencilKit's smoothing and palm rejection, "only draw with Apple Pencil".
    The canvas forces `overrideUserInterfaceStyle = .light` (PencilKit would otherwise
    remap ink colours in dark mode) and keeps its own `UndoManager`. **Coordinates**: the
    page has no `viewport-fit=cover`, so CSS pixel 0 is below the status bar; CSS rects and
    points go through `webview.scrollView.convert(_:to:)`, never straight into the web
    view's coordinates (that put the canvas ~24 pt too high, over the toolbar). Drawing
    verified on an iPad Pro 11" M4 (iPadOS 27.0) with an Apple Pencil Pro, 2026-10-17;
    squeeze, double tap, barrel roll and haptics not confirmed yet. Package.swift
    needs swift-tools-version ≥ 5.7 for `.iOS(.v16)`.
  - Editable sketches (`lib/sketchData.ts`): the strokes travel inside the PNG, in a
    zlib-compressed iTXt chunk `omanote-sketch` (JSON `{v: 1, strokes, pencilkit?}`,
    coordinates in CSS px from the PNG's corner), so the attachment stays a plain image
    for Joplin. `strokes` is the common format; the iPad also stores its `PKDrawing`
    (base64) and prefers it. Cross-engine reopening is approximate (no pixel-eraser masks
    from PencilKit, no web pixel-eraser passes in PencilKit). Tapping a sketch (alt
    `sketch-…`) or its pencil button sends `SKETCH_EDIT` from the editor; "Insert" then
    replaces the **same** attachment (`update_image` → `store.update_resource`: new file,
    `size`, `blob_updated_time`, dirty) and only fixes the tag's `width`. The sync uploads
    a resource's file again when `blob_updated_time > sync_time` (Joplin's rule), and
    `put_remote` drops the local file when a newer one arrives, to be fetched on demand.
    Sketch images are read as bytes (`resource_bytes`) into blob URLs, never through the
    asset protocol, so an edited one never comes from the web view cache (a `?v=` query
    on the asset URL made images fail to load on iPadOS). Verified on the iPad and in
    Chrome with mock data.
  - Below 1000 px (iPad portrait) the toolbar tightens and shows only the 2 latest
    picked colours; the Pencil palette shows all of them.
- Whole-Joplin mode (`Config.whole_joplin`, Settings or first-run setup → "Tutto Joplin"): the tree root is
  Joplin's top level (`tree_root()` = "") so every notebook shows as a folder; new notes and
  notes dropped at the top level still go to `root_folder_id` (Joplin notes need a notebook).
  `create_folder` with parent "" makes a top-level notebook. The notebook for new notes is
  changed with `set_notes_home` (Settings button or tree menu "Usa per le nuove note").
- Local mode: "use without a server" creates a local notebook (`Config.local_only`);
  connecting a Joplin Server later uploads those notes instead of resetting the store
  (only switching from one server/account to another resets it). Verified with
  `crates/omanote-core/examples/local_then_sync.rs` + the official Joplin CLI.
- Linux/Omarchy (Omarchy 4, real hardware, 2026-09-21): release build installed in `~/.local/bin`
  with a desktop entry, theme from `colors.toml` incl. ANSI-only themes, `omanote --toggle` on a
  Hyprland binding (Super+Alt+N), undecorated window, keyring via gnome-keyring, bar plugin.
  Build it from a git clone outside Syncthing (`~/Projects/omanote` on the Omarchy box).

**Not verified yet**
- Linux: OCR with tesseract/grim/slurp, live theme switching (the watcher reads the new path).
- iOS/iPadOS: project initialised and committed in `app/src-tauri/gen/apple` (iPhone + iPad,
  all iPad orientations, minimum iOS 16 in `tauri.conf.json` *and* `gen/apple/project.yml` +
  `Podfile`: `tauri ios init` does not rewrite an existing project, Xcode 27 rejects the
  default 14.0). `project.yml` declares `UIApplicationSceneManifest` with multiple scenes:
  iOS 27 kills apps without the scene lifecycle at launch (`EXC_BREAKPOINT` in
  `_UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption`, seen only on a real
  device), and tao attaches its window to a scene only when that key is set. Runs in the iPad
  simulator and on a real iPhone (iOS 27.2); crash reports: `idevicecrashreport -k -e <dir>`. Build: `cd app && LANG=en_US.UTF-8 npx tauri ios
  build --debug --target aarch64-sim` (needs `rustup target add aarch64-apple-ios
  aarch64-apple-ios-sim`, Homebrew `cocoapods`; xcodegen and libimobiledevice are installed by
  Tauri). After editing `project.yml` run `xcodegen generate` in `gen/apple`. Device builds:
  pass the Apple team only as `APPLE_DEVELOPMENT_TEAM=<id>` (`--target aarch64`, then
  `xcrun devicectl device install app`); Tauri writes it into `project.pbxproj` as
  `DEVELOPMENT_TEAM`: **never commit that** (personal team id), restore the file after building.
  The device build **must** pass `--debug`: without it `tauri ios build` defaults to **release**, and
  the release link fails on Xcode 27 / Swift 6.4 with "Undefined symbols for architecture arm64"
  (`_register_plugin`, `_on_webview_created`, `_log_stdout`, `_init_plugin_notification`, …), because
  the Swift static libs swift-rs generates (`libTauri.a`, `libtauri-plugin-*.a`) then carry local
  symbols. Diagnose with `nm -arch arm64 <lib>.a | grep register_plugin`: lowercase `t` is broken
  (release), uppercase `T` is good (debug). `--release` is not a valid flag at all. Verified device
  flow (iPhone 16 Pro, iOS 27.2, 2026-10-15): `cd app && PATH="$HOME/.cargo/bin:/usr/bin:$PATH"
  LANG=en_US.UTF-8 APPLE_DEVELOPMENT_TEAM=<id> npm run tauri ios build -- --debug --target aarch64`;
  restore `project.pbxproj`; `xcrun devicectl device install app --device <udid>
  gen/apple/build/omanote_iOS.xcarchive/Products/Applications/Omanote.app` (the iPhone must be
  unlocked, or the install fails with the misleading "developer disk image could not be mounted");
  `xcrun devicectl device process launch --device <udid> app.omanote` proves it starts.
- iPad + Apple Pencil, still to check: **Scribble** (iPadOS turns handwriting into text in
  any text field) in the CodeMirror `contenteditable`, including the scratch-out /
  insert-space gestures, and Apple Vision on handwriting through the sketch "Text" exit.
  A new device must be in the team's provisioning profile: `tauri ios build` passes only
  `-allowProvisioningUpdates`, so register it once with `xcodebuild -project
  gen/apple/omanote.xcodeproj -scheme omanote_iOS -destination 'id=<udid>'
  -allowProvisioningUpdates -allowProvisioningDeviceRegistration DEVELOPMENT_TEAM=<id> build`
  (the Rust build phase fails outside Tauri, but the profile is refreshed first), then build
  with Tauri again. The device also needs Developer Mode (Settings → Privacy & Security; the
  switch appears only after Xcode has seen the device).
- Android: project initialised and committed in `app/src-tauri/gen/android`. Debug APK built on
  the Omarchy box and installed on a Pixel Fold (Android 17 beta, 2026-10-23): it starts, shows
  the setup screen and fetches the ECB rates over HTTPS. **Not verified yet**: login and sync
  with a real Joplin Server, the launcher icon and the system-bar insets as seen on screen,
  the sketch canvas with touch and stylus, the Fold's inner display and posture changes.
  - TLS: reqwest 0.13 checks certificates with `rustls-platform-verifier`, which panics on
    Android ("Expect rustls-platform-verifier to be initialized") unless it gets the JVM and
    the app context first. `src/android_tls.rs` does that at setup from tao's
    `main_android_context()` (retrying up to 5 s if the activity is not registered yet); its
    Kotlin half comes from the local Maven repository inside the `rustls-platform-verifier-android`
    crate, found with `cargo metadata` in `gen/android/app/build.gradle.kts`, plus a ProGuard
    keep rule in `proguard-rules.pro` for release builds.
  - Binary IPC: Android has no custom-protocol IPC, so Tauri's postMessage fallback sends a
    `Uint8Array` argument as a JSON array of numbers, never `InvokeBody::Raw`. Commands taking
    bytes (`add_image`, `update_image`, `ocr_image`) read them through `body_bytes()`, which
    accepts both; until 2026-10-23 sketches, pasted images and OCR failed on Android with
    "expected raw image bytes". Headers (`x-name`, `x-id`) do arrive. Binary *responses*
    (`resource_bytes`) reach JS as an `ArrayBuffer` on Android too.
  - The sync fails with "network error" while the phone dozes (Android cuts background apps
    off the network); not a bug, it resumes when the app is in front.
  - `MainActivity.kt`: Android 15+ always draws edge to edge, so the content view is padded
    by the system bars, cutout and keyboard insets; the strip behind the bars is fixed at the
    default theme's `#1a1b26` with light icons (does not follow the chosen theme yet).
  - Icon: an adaptive icon (launchers cut it to a circle on Pixel, which would clip the stepped
    tile): `tools/gen-icons.py` also writes `design/android-{bg,fg,mono}.svg` (tile surface full
    bleed, glowing pencil inside the safe circle, monochrome for themed icons); render them to
    PNG with `rsvg-convert -w 1024`, run `npx tauri icon ../design/android-icon.json -o <tmp>`
    from `app/` and copy its `android/` folder over `gen/android/app/src/main/res` and
    `src-tauri/icons/android` (writing to `src-tauri/icons` directly would also replace the
    desktop and iOS icons).
  - Toolchain (no root needed; Arch's `rust` package has no Android targets): rustup in
    `~/.cargo` with `--no-modify-path` and the four `*-linux-android*` targets, a Temurin JDK 21,
    Google's command-line tools in `~/Android/Sdk/cmdline-tools/latest` (`sdkmanager` is now
    replaced by `android sdk install platform-tools ndk/<ver> build-tools/<ver>
    platforms/android-36`). Put `JAVA_HOME`, `ANDROID_HOME`, `NDK_HOME` and `~/.cargo/bin` first
    in `PATH`, then `cd app && npx tauri android build --debug --target aarch64 --apk` and
    `adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`.
    Debug builds may use cleartext `http://` servers (`usesCleartextTraffic`), release builds
    may not. On a Fold, `adb exec-out screencap -p -d <id>` needs a display id
    (`adb shell dumpsys SurfaceFlinger --display-id`).

**Known gaps / next steps**
- Tags are synced but not shown.
- Sketches made before editable sketches (2026-10-17) hold no strokes and can only be
  replaced; another client that re-encodes a sketch PNG drops its strokes too.
- Mobile: timer notifications are not scheduled, so they do not fire while the app is
  suspended; OCR on Android missing (ML Kit plugin planned).
- Items the sync cannot read are skipped; their errors now go to
  `<data dir>/sync-errors.log` (last ~500 lines). `fetch_resource` fetches an unknown
  resource item from the server on demand, and Settings → "Risincronizza tutto"
  (`resync_all`) clears the delta cursor to re-examine every item. Solved (2026-09-22):
  the owner's images and folders "missing" in Omanote were not on the server at all: the
  server had been recreated, and Joplin desktop never re-uploads items its `sync_items`
  table already marks as synced. Fix on the Joplin side: Sync → Advanced → "Re-upload local
  data to sync target". `cargo run -p omanote-core --example diagnose` lists what the server
  really holds (read-only GETs) to tell such cases from Omanote bugs.
- Not re-tested end to end after the 2026-09-23 fixes: re-upload with a local edit that
  conflicts, and any re-upload under E2EE. Docker.app is no longer installed on the dev Mac
  (the `/usr/local/bin/docker` link is dangling) and the Joplin CLI lived in a temporary
  folder: reinstall both (`npm i joplin` in a scratch dir) before rerunning the examples.
- Search is a linear scan (fine for hundreds of notes; add FTS5 if needed).
- Source of truth: **GitHub** `TinyWorkshopDesign/omanote` (**public** since 2026-09-24, MIT;
  `omanote-archive` is the old private repo with pre-cleanup history, do not push to it),
  branch `main`. Share code between machines with git, never through Syncthing:
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
- Public builds: pushing a tag `vX.Y.Z` (matching `version` in `app/src-tauri/tauri.conf.json`)
  runs `.github/workflows/release.yml`, which builds a universal macOS `.dmg` (ad-hoc signed,
  not notarized) and Linux AppImage/.deb/.rpm and attaches them to a **draft** release (not marked pre-release, so `/releases/latest` keeps working);
  check the assets, then publish the draft.
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
| `omarchy-plugin` | Omarchy shell bar widget `tinyworkshop.omanote` (QML): quick capture and recent notes through `omanote-cli`, opens notes with `omanote --open <id>`; `install.sh` copies it into `~/.config/omarchy/plugins/`; when it is installed the app skips its Linux tray icon (`omarchy_bar_plugin_installed`) |
| `tools/gen-symbol-font.py` | Cuts the icon font subset (`app/static/fonts/omanote-symbols.woff2`) and the glyph map from the pinned Nerd Fonts release (needs `fonttools` + `brotli`) |
| `tools/gen-themes.py` | Regenerates `app/src/themes.css` from the Omarchy repo |
| `tools/gen-icons.py` | Composes the app and tray icons in `design/` from the icon font's pencil glyph |

## Commands

```bash
cargo test                          # all Rust tests (core, CLI/MCP, app: timer, theme, OCR)
npm run --prefix app test           # calculator and note-mode tests
npm run --prefix app check          # Svelte/TypeScript type check
npm run --prefix app dev            # UI in a browser with mock data (src/lib/mock.ts; add ?setup for first run)
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
- **Shortcuts** use `Mod` = ⌘ on Apple, Ctrl elsewhere. Omarchy binds
  almost everything to Super, so avoid Super and `Alt+Tab` inside the app. On Wayland the
  global hotkey is a Hyprland binding calling `omanote --toggle`. `omanote --open <id>` shows a
  given note (used by the bar plugin), also when it starts the app.
- **Omarchy theme**: on Omarchy the palette comes from
  `~/.local/state/omarchy/current/theme/colors.toml` (Omarchy 4; `~/.config/omarchy/current/theme`
  before) and follows theme changes live; ANSI-only palettes (`color0`…`color15`) are completed
  in `theme.rs`; the bundled themes are only a fallback for other platforms.

## Debugging the real webview

`npm run tauri dev` builds in debug mode, where the webview forwards JS errors to the
terminal as `[webview] …` lines and runs `<data dir>/debug-eval.js` once if the file
appears (then deletes it); `window.__omanoteLog(msg)` prints to the same log. Both
exist only with `debug_assertions`. Keep diagnostics read-only: the app may be
connected to the user's real Joplin account.

## Using Omanote from an agent

The CLI lives in `crates/omanote-cli` as a library (`omanote_cli::main_with`) used by two
binaries: `omanote-cli`, and the app binary itself, whose `main.rs` hands the commands in
`omanote_cli::COMMANDS` over before starting Tauri. So `omanote mcp` works wherever the app is
installed (on macOS `/Applications/Omanote.app/Contents/MacOS/omanote mcp`, nothing else to
install). The MCP server is generic stdio JSON-RPC: Settings → AI shows ready-made setups
for Claude Code, Codex, Gemini CLI, VS Code, Cursor, Claude Desktop, Hermes Agent, Zed and
generic `mcpServers` JSON, built from the real binary path (`cli_path` command).

```bash
claude mcp add omanote -- omanote-cli mcp     # MCP tools: list/search/read/create/update/append/move/trash/sync
codex mcp add omanote -- omanote-cli mcp
gemini mcp add -s user omanote omanote-cli mcp
omanote-cli new --folder Work "Meeting\n- [ ] slides"
omanote-cli search budget --json
```

Tool descriptions and the server `instructions` in `crates/omanote-cli/src/mcp.rs`
explain the note format to the model; keep them accurate when behaviour changes.
