#!/usr/bin/env python3
"""Builds the icon font used by the app UI, the app/tray icons and the Omarchy bar plugin.

Omanote draws its interface icons the way Omarchy draws the bar, its menu and its
panels: as monochrome glyphs from a Nerd Font (Omarchy's default is JetBrainsMono
Nerd Font; see Omarchy's manual/38-fonts.md and shell/Ui/OpticalGlyph.qml). This
script takes those glyphs from Nerd Fonts' Symbols Nerd Font (MIT) and cuts a
subset down to the ones we use, so the app can bundle it: the whole vocabulary is
a few dozen KB in woff2 instead of hand-drawn SVG paths.

Outputs:
  app/static/fonts/omanote-symbols.woff2   the subset font served to the webview
  app/src/lib/symbols.generated.ts         the name -> codepoint map

Usage:
  python3 tools/gen-symbol-font.py [--check]

  --check  verify the map and the font already in the tree without rewriting
           anything (used by the tests / CI).

The upstream tarball is pinned by version and SHA-256 and cached in
~/.cache/omanote/. Glyph names come from Nerd Fonts' naming (md-* = Material
Design Icons, cod-* = Codicons, fa-* = Font Awesome); the same set the Omarchy
menu names in default/omarchy/omarchy-menu.jsonc.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import pathlib
import shutil
import subprocess
import sys
import tarfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
WOFF2 = ROOT / "app" / "static" / "fonts" / "omanote-symbols.woff2"
GENERATED = ROOT / "app" / "src" / "lib" / "symbols.generated.ts"

# Pinned upstream: https://github.com/ryanoasis/nerd-fonts/releases (MIT).
NERD_VERSION = "3.5.1"
TARBALL = f"NerdFontsSymbolsOnly.tar.xz"
TARBALL_SHA256 = "01172f37db8543edb102e5cb5c64101c9f4686630804d49b419aa07b23a69996"
TARBALL_URL = (
    f"https://github.com/ryanoasis/nerd-fonts/releases/download/v{NERD_VERSION}/{TARBALL}"
)
FONT_IN_ARCHIVE = "SymbolsNerdFont-Regular.ttf"

# The icons the app uses, in the order they appear in icons.ts. Names are Nerd
# Fonts glyph names; a missing one is a hard error so the map never drifts.
ICONS: dict[str, str] = {
    "menu": "md-menu",
    "plus": "md-plus",
    "close": "md-close",
    "settings": "md-cog",
    "search": "md-magnify",
    "pin": "md-pin_outline",
    "capture": "md-crop_free",
    "sync": "md-sync",
    "more": "md-dots_horizontal",
    "prev": "md-chevron_left",
    "next": "md-chevron_right",
    "expand": "md-chevron_right",
    "collapse": "md-chevron_down",
    "warn": "md-alert_outline",
    "image": "md-image_outline",
    "ocr": "md-text_recognition",
    "lock": "md-lock_outline",
    "folderAdd": "md-folder_plus_outline",
    "note": "md-note_text_outline",
    "trash": "md-trash_can_outline",
    "restore": "md-backup_restore",
    "pencil": "md-pencil",
}

# Kept in the subset even though nothing draws them yet: the UI grows, and a
# subset is cheap (~150 bytes a glyph). Adding one here means running this script.
RESERVE: tuple[str, ...] = (
    "md-chevron_up",
    "md-arrow_left",
    "md-arrow_right",
    "md-check",
    "md-checkbox_marked_outline",
    "md-close_circle_outline",
    "md-information_outline",
    "md-alert_circle_outline",
    "md-content_copy",
    "md-content_cut",
    "md-content_paste",
    "md-paperclip",
    "md-download_outline",
    "md-upload_outline",
    "md-cloud_outline",
    "md-cloud_sync_outline",
    "md-wifi_off",
    "md-key_outline",
    "md-eye_outline",
    "md-eye_off_outline",
    "md-server",
    "md-folder_outline",
    "md-file_outline",
    "md-delete_outline",
    "md-restore",
    "md-undo",
    "md-redo",
    "md-drag",
    "md-format_list_bulleted",
    "md-magnify_close",
    "md-text_box_outline",
    "md-image_multiple_outline",
    "md-timer_outline",
    "md-calendar_blank_outline",
    "md-tag_outline",
    "md-database_outline",
)


def require(module: str, package: str) -> None:
    """The tools need fontTools (and brotli for woff2): say so plainly when missing."""
    try:
        __import__(module)
    except ImportError:
        sys.exit(f"ERRORE: manca {module}. Installa con: python3 -m pip install {package}")


def cache_dir() -> pathlib.Path:
    return pathlib.Path.home() / ".cache" / "omanote"


def fetch_font() -> bytes:
    """Returns the Symbols Nerd Font TTF, downloading and verifying it once."""
    cached = cache_dir() / f"{NERD_VERSION}" / FONT_IN_ARCHIVE
    if cached.exists():
        return cached.read_bytes()
    print(f"scarico {TARBALL_URL}")
    with urllib.request.urlopen(TARBALL_URL, timeout=120) as r:  # noqa: S310
        blob = r.read()
    digest = hashlib.sha256(blob).hexdigest()
    if digest != TARBALL_SHA256:
        sys.exit(f"ERRORE: SHA-256 di {TARBALL} inatteso\n  atteso {TARBALL_SHA256}\n  visto  {digest}")
    cached.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(blob)) as tar:
        member = next((m for m in tar.getmembers() if m.name.endswith(FONT_IN_ARCHIVE)), None)
        if member is None:
            sys.exit(f"ERRORE: {FONT_IN_ARCHIVE} non trovato in {TARBALL}")
        data = tar.extractfile(member)
        assert data is not None
        cached.write_bytes(data.read())
    return cached.read_bytes()


def codepoints(font_bytes: bytes) -> dict[str, int]:
    """Maps glyph name -> codepoint for every glyph we want, failing loudly otherwise."""
    from fontTools.ttLib import TTFont
    font = TTFont(io.BytesIO(font_bytes))
    by_name = {name: cp for cp, name in font.getBestCmap().items()}
    wanted = {**ICONS, **{name: name for name in RESERVE}}
    out: dict[str, int] = {}
    missing: list[str] = []
    for key, glyph_name in wanted.items():
        if glyph_name in by_name:
            out[key] = by_name[glyph_name]
        else:
            missing.append(f"{key} ({glyph_name})")
    if missing:
        sys.exit("ERRORE: glifi assenti nel Symbols Nerd Font:\n  " + "\n  ".join(missing))
    return out


def build_woff2(font_bytes: bytes, points: dict[str, int]) -> bytes:
    import tempfile

    from fontTools.ttLib import TTFont
    with tempfile.TemporaryDirectory() as tmp:
        src = pathlib.Path(tmp) / "src.ttf"
        dst = pathlib.Path(tmp) / "subset.woff2"
        src.write_bytes(font_bytes)
        unicodes = ",".join(f"U+{cp:X}" for cp in sorted(set(points.values())))
        subprocess.run(
            [
                sys.executable, "-m", "fontTools.subset", str(src),
                f"--unicodes={unicodes}",
                "--layout-features=",
                "--no-hinting",
                "--desubroutinize",
                f"--output-file={dst}",
            ],
            check=True,
            capture_output=True,
        )
        font = TTFont(dst)
        font.flavor = "woff2"
        font.save(dst)
        return dst.read_bytes()


def ts_source(points: dict[str, int]) -> str:
    lines = [
        "// Generated by tools/gen-symbol-font.py — do not edit by hand.",
        f"// {len(set(points.values()))} glyphs from Nerd Fonts v{NERD_VERSION}, Symbols Nerd Font (MIT).",
        "// The app draws them the way Omarchy draws its bar and menu: monochrome",
        "// glyphs coloured with the active theme's foreground.",
        "",
        f'export const SYMBOL_FONT_VERSION = "{NERD_VERSION}";',
        'export const SYMBOL_FONT_URL = "/fonts/omanote-symbols.woff2";',
        "",
        "/** Glyph codepoints, keyed by the name used in icons.ts. */",
        "export const CODEPOINTS = {",
    ]
    for key, cp in points.items():
        # Quoted: reserve keys are Nerd Fonts glyph names, which contain dashes.
        lines.append(f'  "{key}": 0x{cp:05x},')
    lines.append("} as const;")
    lines.append("")
    lines.append("export type IconName = keyof typeof CODEPOINTS;")
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="verify the tree, write nothing")
    args = ap.parse_args()

    # Fail with the install command rather than a bare ModuleNotFoundError: the
    # system python3 on macOS has neither package.
    require("fontTools", "fonttools")
    require("brotli", "brotli")

    font_bytes = fetch_font()
    points = codepoints(font_bytes)

    if args.check:
        problems = []
        if not WOFF2.exists():
            problems.append(f"manca {WOFF2.relative_to(ROOT)}")
        if not GENERATED.exists():
            problems.append(f"manca {GENERATED.relative_to(ROOT)}")
        elif GENERATED.read_text() != ts_source(points):
            problems.append(f"{GENERATED.relative_to(ROOT)} non e' aggiornato con Nerd Fonts v{NERD_VERSION}")
        if problems:
            sys.exit("Controllo icone fallito:\n  " + "\n  ".join(problems))
        print(f"icone ok: {len(set(points.values()))} glifi, woff2 {WOFF2.stat().st_size / 1024:.1f} KB")
        return

    WOFF2.parent.mkdir(parents=True, exist_ok=True)
    WOFF2.write_bytes(build_woff2(font_bytes, points))
    GENERATED.write_text(ts_source(points))
    print(f"scritto {WOFF2.relative_to(ROOT)}  ({WOFF2.stat().st_size / 1024:.1f} KB)")
    print(f"scritto {GENERATED.relative_to(ROOT)}  ({len(set(points.values()))} glifi)")


if __name__ == "__main__":
    main()
