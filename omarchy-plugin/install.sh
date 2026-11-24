#!/usr/bin/env bash
# Installs the Omanote bar widget into the Omarchy shell (copy, validate, enable).
set -euo pipefail

ID="tinyworkshop.omanote"
SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="$HOME/.config/omarchy/plugins/$ID"

mkdir -p "$DEST"
# Start clean so renamed or removed files do not linger.
rm -f "$DEST"/*.qml "$DEST"/*.js
rm -rf "$DEST/i18n"
# Plugin folders must not contain symlinks: copy real files only.
cp -f "$SRC"/manifest.json "$SRC"/*.qml "$SRC"/*.js "$SRC"/README.md "$SRC"/LICENSE "$DEST"/
cp -rf "$SRC/i18n" "$DEST"/
[[ -f "$SRC/preview.png" ]] && cp -f "$SRC/preview.png" "$DEST"/

omarchy plugin validate "$DEST"
# The shell must discover the plugin before it can be enabled.
omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
enabled=$(omarchy plugin list --json | python3 -c 'import json,sys; print(any(p["id"] == sys.argv[1] and p.get("enabled") for p in json.load(sys.stdin)))' "$ID")
[[ $enabled == True ]] || omarchy plugin enable "$ID" right
command -v omanote-cli >/dev/null || command -v omanote >/dev/null \
  || echo "Note: neither omanote nor omanote-cli is on PATH; the panel needs the Omanote app." >&2
echo "Installed $ID"
