# Omanote for the Omarchy bar

A bar widget for [Omanote](../README.md), the quick-notes scratchpad that syncs
with Joplin Server.

- **Click** the note icon: a panel with a capture field and your recent notes.
  - Type and press **Enter** to save a new note, without opening the app.
  - Typing also filters the list; **↓ / ↑** pick a note and **Enter** opens it in Omanote.
  - **Esc** clears the field, then closes the panel.
- **Right-click**: show or hide the Omanote window (`omanote --toggle`).
- **Middle-click**: new note in the app (`omanote --new`).

Notes are read and written through `omanote-cli`, which uses the app's own
database: the app picks up new notes immediately and syncs them with Joplin.
The widget does nothing in the background; it reads the notes when the panel opens.

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
of the bar. Run it again after changes; the shell reloads plugins on save (changes to
`PencilIcon.qml`, or new and renamed QML files, may only show after `omarchy restart shell`).

With the widget installed, the Omanote app does not add its own tray icon on Linux,
so Omanote shows up once in the bar.

Summon the panel from a key binding with:

```bash
omarchy-shell shell toggle tinyworkshop.omanote
```

## Settings

`recentNotes` (3–30, default 8): how many notes the panel lists.
`showPreview` (default on): the first lines of each note under its title; turn it off
to show titles only.
