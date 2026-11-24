#!/usr/bin/env bash
# Publishes omarchy-plugin/ to its own repository, TinyWorkshopDesign/omarchy-omanote,
# which `omarchy plugin add` and the Omarchy plugin marketplace need: a public repo with
# manifest.json at its root. The source stays here (Calc.js is generated from the app's
# calc.ts); the mirror only ever receives copies, one commit per publish, tagged with
# the manifest's version.
#
#   tools/publish-plugin.sh            # show what would change
#   tools/publish-plugin.sh --push     # commit, tag and push it
set -euo pipefail

REPO="TinyWorkshopDesign/omarchy-omanote"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/omarchy-plugin"
WORK="${OMANOTE_PLUGIN_MIRROR:-$HOME/Projects/build-cache/omarchy-omanote}"
PUSH=false
[[ ${1:-} == --push ]] && PUSH=true

node "$ROOT/tools/gen-plugin-calc.mjs" --check
omarchy plugin validate "$SRC"
git -C "$ROOT" diff --quiet -- omarchy-plugin \
  || { echo "omarchy-plugin/ has uncommitted changes: commit them first" >&2; exit 1; }

version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$SRC/manifest.json")
source_rev=$(git -C "$ROOT" rev-parse --short HEAD)

if [[ ! -d $WORK/.git ]]; then
  mkdir -p "$(dirname "$WORK")"
  git clone -q "https://github.com/$REPO.git" "$WORK"
fi
git -C "$WORK" fetch -q --tags origin
git -C "$WORK" checkout -q -B main
# A new, empty mirror has no main yet: the first publish creates it.
if git -C "$WORK" rev-parse -q --verify refs/remotes/origin/main >/dev/null; then
  git -C "$WORK" reset -q --hard origin/main
fi
# The repository's own rule: commit only as the project's GitHub identity.
git -C "$WORK" config user.name "TinyWorkshopDesign"
git -C "$WORK" config user.email "23453441+TinyWorkshopDesign@users.noreply.github.com"

# Replace everything but .git and .github with the plugin's files. install.sh stays
# behind: it installs a working copy of this repository, not the mirror.
find "$WORK" -mindepth 1 -maxdepth 1 ! -name .git ! -name .github -exec rm -rf {} +
cp "$SRC"/manifest.json "$SRC"/*.qml "$SRC"/*.js "$SRC"/README.md "$SRC"/LICENSE "$WORK"/
cp -r "$SRC/i18n" "$WORK"/
[[ -f $SRC/preview.png ]] && cp "$SRC/preview.png" "$WORK"/
mkdir -p "$WORK/.github"
printf 'ko_fi: K8C82AR8S0\n' > "$WORK/.github/FUNDING.yml"
omarchy plugin validate "$WORK"

git -C "$WORK" add -A
if git -C "$WORK" diff --cached --quiet; then
  echo "The mirror already matches omarchy-plugin/ ($version)."
  exit 0
fi
git -C "$WORK" status --short

if ! $PUSH; then
  echo "Dry run: rerun with --push to publish $version from omanote@$source_rev."
  exit 0
fi
if git -C "$WORK" rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "v$version is already published: raise \"version\" in omarchy-plugin/manifest.json" >&2
  exit 1
fi
git -C "$WORK" commit -q -m "Omanote plugin $version (from omanote@$source_rev)"
git -C "$WORK" tag "v$version"
git -C "$WORK" push -q origin main "v$version"
echo "Published $REPO $version"
