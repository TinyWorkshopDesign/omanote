#!/usr/bin/env python3
"""Draws design/icon.svg (app icon) and design/tray.svg (menu bar / tray) from the
pencil glyph of the icon font, on the Omarchy logo grid.

The mark is the "pencil" glyph of app/static/fonts/omanote-symbols.woff2 — the same
glyph the interface draws (app/src/lib/icons.ts) and the same shape the Omarchy bar
plugin shows (omarchy-plugin/PencilIcon.qml) — so app icon, tray, window and mobile
icons all carry the mark the UI uses. The tile keeps the Omarchy logo grid (square
cells, stepped corners) and the terminal-green palette.

Then:
  rsvg-convert -w 1024 design/icon.svg -o design/icon.png
  cd app && PATH=/usr/bin:$PATH npx tauri icon ../design/icon.png
  rsvg-convert -w 64 design/tray.svg -o app/src-tauri/icons/tray.png

(`tools/gen-symbol-font.py` must have run first: it writes the subset font.)
"""
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
DESIGN = ROOT / "design"
FONT = ROOT / "app" / "static" / "fonts" / "omanote-symbols.woff2"
ICON_MAP = ROOT / "app" / "src" / "lib" / "symbols.generated.ts"
ICON = "pencil"  # key in the app's icon map (app/src/lib/icons.ts)
G = "#2bff88"  # terminal green, the accent Omanote shipped with

CANVAS = 1024
C = 32  # Omarchy logo grid: 32 cells of 32px on a 1024 canvas
MARK = 560  # side of the square the glyph is scaled into


def codepoint(key: str) -> int:
    """The glyph's codepoint, straight from the map the app itself renders from."""
    import re

    try:
        import fontTools.ttLib  # noqa: F401
    except ImportError:
        raise SystemExit(
            "ERROR: fontTools is missing. Install it with: python3 -m pip install fonttools"
        )

    match = re.search(rf'"{key}": 0x([0-9a-f]+),', ICON_MAP.read_text())
    if match is None:
        raise SystemExit(f"ERROR: '{key}' is not in {ICON_MAP.name}; "
                         f"add it to ICONS in tools/gen-symbol-font.py")
    return int(match.group(1), 16)


def glyph_path(point: int):
    """The glyph as an SVG path plus its box in font units (y up, baseline at 0)."""
    from fontTools.pens.svgPathPen import SVGPathPen
    from fontTools.ttLib import TTFont

    font = TTFont(str(FONT))
    glyph_set = font.getGlyphSet()
    # Subset fonts rename glyphs (uF03EB), so look the name up through the cmap.
    name = font.getBestCmap()[point]
    pen = SVGPathPen(glyph_set)
    glyph_set[name].draw(pen)
    box = font["glyf"][name]
    return pen.getCommands(), (box.xMin, box.yMin, box.xMax, box.yMax)


def centred(path_d: str, bbox, size: float, cx: float, cy: float) -> str:
    """Places the glyph, scaled to `size` on its longer side and centred on (cx, cy)."""
    x0, y0, x1, y1 = bbox
    scale = size / max(x1 - x0, y1 - y0)
    xc, yc = (x0 + x1) / 2, (y0 + y1) / 2
    return (
        f'<g transform="translate({cx} {cy}) scale({scale:.6f} {-scale:.6f}) '
        f'translate({-xc:.1f} {-yc:.1f})"><path d="{path_d}"/></g>'
    )


d, bbox = glyph_path(codepoint(ICON))

# Tile: 26x26 cells (cols/rows 3..28) with Omarchy-style stepped corners.
pts = [(2, 0), (24, 0), (24, 1), (25, 1), (25, 2), (26, 2), (26, 24), (25, 24), (25, 25), (24, 25),
       (24, 26), (2, 26), (2, 25), (1, 25), (1, 24), (0, 24), (0, 2), (1, 2), (1, 1), (2, 1)]
tile = "M" + " L".join(f"{(3 + x) * C} {(3 + y) * C}" for x, y in pts) + "Z"

svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{CANVAS}" height="{CANVAS}" viewBox="0 0 {CANVAS} {CANVAS}">
  <!-- Omanote app icon: the pencil glyph of the icon font (the mark the app's own
       buttons use) on the Omarchy logo grid, in terminal green. -->
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#101814"/>
      <stop offset="1" stop-color="#040706"/>
    </linearGradient>
    <pattern id="grid" width="{C}" height="{C}" patternUnits="userSpaceOnUse">
      <path d="M{C} 0H0V{C}" fill="none" stroke="{G}" stroke-opacity="0.06" stroke-width="2"/>
    </pattern>
    <pattern id="scan" width="8" height="8" patternUnits="userSpaceOnUse">
      <rect width="8" height="3" fill="#000" fill-opacity="0.32"/>
    </pattern>
    <filter id="glow" x="-30%" y="-30%" width="160%" height="160%">
      <feGaussianBlur stdDeviation="12" result="b"/>
      <feMerge><feMergeNode in="b"/><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
    <radialGradient id="haze">
      <stop offset="0" stop-color="{G}" stop-opacity="0.13"/>
      <stop offset="1" stop-color="{G}" stop-opacity="0"/>
    </radialGradient>
    <clipPath id="tile"><path d="{tile}"/></clipPath>
  </defs>

  <path d="{tile}" fill="url(#bg)"/>
  <g clip-path="url(#tile)">
    <rect width="{CANVAS}" height="{CANVAS}" fill="url(#grid)"/>
    <circle cx="512" cy="512" r="430" fill="url(#haze)"/>
    <g fill="{G}" filter="url(#glow)">{centred(d, bbox, MARK, 512, 512)}</g>
    <rect width="{CANVAS}" height="{CANVAS}" fill="url(#scan)"/>
  </g>
  <path d="{tile}" fill="none" stroke="{G}" stroke-opacity="0.4" stroke-width="6"/>
</svg>
'''
(DESIGN / "icon.svg").write_text(svg)

# Tray: same glyph, monochrome white, 64x64. On macOS it is a template image
# (tinted to match the menu bar); on Linux the bar plugin draws the same mark.
TRAY = 64
(TRAY_SVG := DESIGN / "tray.svg").write_text(f'''<svg xmlns="http://www.w3.org/2000/svg" width="{TRAY}" height="{TRAY}" viewBox="0 0 {TRAY} {TRAY}">
  <!-- Menu bar / tray icon: the pencil glyph, monochrome. -->
  <g fill="#fff">{centred(d, bbox, TRAY - 8, TRAY / 2, TRAY / 2)}</g>
</svg>
''')

print(f"wrote {DESIGN / 'icon.svg'}")
print(f"wrote {TRAY_SVG}")
print("next: rsvg-convert -w 1024 design/icon.svg -o design/icon.png")
print("      cd app && PATH=/usr/bin:$PATH npx tauri icon ../design/icon.png")
print("      rsvg-convert -w 64 design/tray.svg -o app/src-tauri/icons/tray.png")
