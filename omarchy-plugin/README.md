# Omanote for the Omarchy bar

A bar widget for [Omanote](../README.md), the quick-notes scratchpad that syncs
with Joplin Server.

The panel works like Omanote on a phone: a stack **Folders → Notes → Note**, with a
back arrow at the top left.

- **Click** the pencil in the bar to open the panel. It opens where you left it.
  - **Folders**: All Notes and the folder tree with note counts. **↑ / ↓** (or j / k)
    and **Enter** open a folder; **n** starts a new note.
  - **Notes**: the folder's notes in date sections (Today, Yesterday, Previous 7 Days,
    Previous 30 Days, then months). Typing searches the folder; **↓ / ↑** pick a note,
    **Enter** opens it in the panel, **Ctrl+Enter** opens it in the app. **Enter** with
    no note picked saves the text as a new note in the folder (quick capture).
    **Ctrl+N** opens a new note in the panel. **←** on an empty field goes back to the
    folders; **Esc** clears the field, then closes the panel.
  - **Note**: the note, editable. It saves as you type; the first line is the title.
    Calculations show their results as in the app (`2+2=`, variables, `sum` / `avg` /
    `count` / `math` notes, `100 € in $ =` with the app's exchange rates). Images and
    sketches show in place; hover one to remove it from the note. **Esc** goes back to
    the list, **Ctrl+O** (or the icon at the top right) opens the note in the app,
    **Ctrl+S** saves at once. The bin moves the note to the trash (click it twice).
    Encrypted notes are read-only here.
  - **Draw** (the pen at the bottom, or **Ctrl+Shift+D**): a canvas with a short
    toolbar: pen (**P**), marker (**H**), eraser (**E**), four colours, size
    (**1**–**3**), undo (**Ctrl+Z**), clear. **Enter** (or the tick) inserts the drawing
    after the line with the cursor, **Esc** cancels. Click a sketch in a note to edit it.
    Drawings are saved like the app's: a PNG on the theme background with the strokes
    inside, so the app (and the panel) can reopen and edit them; other Joplin clients
    see the picture.
- **Right-click**: show or hide the Omanote window (`omanote --toggle`).
- **Middle-click**: new note in the app (`omanote --new`).

Notes are read and written through `omanote-cli`, which uses the app's own
database: the app picks up changes immediately and syncs them with Joplin.
The widget does nothing in the background; it reads the data when the panel opens.

The panel edits the note's Markdown as plain text around its images: checkboxes,
headings and rich text show as Markdown. Recently Deleted, new folders and sorting are
in the app only. Images are downloaded from Joplin Server the first time they show
(`omanote-cli resource`), drawings are stored with `omanote-cli sketch`.

`Calc.js` is the app's calculator (`app/src/lib/calc.ts`) made to run in QML's
JavaScript engine by `tools/gen-plugin-calc.mjs`: do not edit it, regenerate it after
changing `calc.ts` (CI checks it).

## Requirements

`omanote` and `omanote-cli` on the `PATH` (e.g. `~/.local/bin`). See the main
README to build them.

## Install

From the repository root:

```bash
omarchy-plugin/install.sh
```

It copies this folder to `~/.config/omarchy/plugins/tinyworkshop.omanote/` (plugin
folders must not contain symlinks), validates it and adds the widget to the right
of the bar. Run it again after changes; the shell reloads plugins on save, but it may
keep the previous version of the QML files until `omarchy restart shell`.

With the widget installed, the Omanote app does not add its own tray icon on Linux,
so Omanote shows up once in the bar.

Summon the panel from a key binding with:

```bash
omarchy-shell shell toggle tinyworkshop.omanote
```

## Settings

`showPreview` (default on): the first lines of each note under its title; turn it off
to show titles only.
