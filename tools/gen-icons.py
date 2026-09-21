#!/usr/bin/env python3
"""Draws design/icon.svg (app icon) and design/tray.svg (tray/menu bar) as pixel art
on the Omarchy logo grid. Then: rsvg-convert -w 1024 design/icon.svg -o design/icon.png,
`npx tauri icon ../design/icon.png` from app/, and tray.svg -> app/src-tauri/icons/tray.png."""
import pathlib
DESIGN = pathlib.Path(__file__).resolve().parent.parent / "design"
# 32x32 cell grid, 32px per cell, on a 1024 canvas.
C=32
G="#2bff88"
def cells(art, ox, oy):
    rows=[r.strip() for r in art.strip().split("\n")]
    d=""
    for y,row in enumerate(rows):
        x=0
        while x<len(row):
            if row[x] in "X#":
                n=1
                while x+n<len(row) and row[x+n]==row[x]: n+=1
                d+=f"M{(ox+x)*C} {(oy+y)*C}h{n*C}v{C}h-{n*C}z"
                x+=n
            else: x+=1
    return d
def run(art, ox, oy, ch):
    rows=[r.strip() for r in art.strip().split("\n")]
    return cells("\n".join("".join(c if c==ch else "." for c in r) for r in rows), ox, oy)

# Tile: 26x26 cells (cols/rows 3..28) with Omarchy-style stepped corners.
pts=[(2,0),(24,0),(24,1),(25,1),(25,2),(26,2),(26,24),(25,24),(25,25),(24,25),(24,26),(2,26),(2,25),(1,25),(1,24),(0,24),(0,2),(1,2),(1,1),(2,1)]
tile="M"+" L".join(f"{(3+x)*C} {(3+y)*C}" for x,y in pts)+"Z"

def pencil(M, h, eraser, bands, base, graphite):
    """Pencil pointing to the bottom-left corner of an MxM grid, in 1-cell lines.

    The axis is the diagonal x+y = M-1; the body edges are the diagonals at
    +-h. Caps and bands are perpendicular lines x-y = c (eraser end, ferrule
    bands, base of the wooden cone). "X" = outline, "#" = filled (graphite, eraser)."""
    g = [["."] * M for _ in range(M)]
    lo, hi = M - 1 - h, M - 1 + h

    def put(x, y, ch="X"):
        if 0 <= x < M and 0 <= y < M and g[y][x] != "X":
            g[y][x] = ch

    def cross(c):                      # both edge points of the perpendicular x-y=c
        return ((lo + c) // 2, (lo - c) // 2), ((hi + c) // 2, (hi - c) // 2)

    (ux, uy), _ = cross(base)
    _, (ex, ey) = cross(eraser)
    for x in range(ux, (lo + eraser) // 2 + 1):
        put(x, lo - x)                 # upper-left edge
    for x in range((hi + base) // 2, ex + 1):
        put(x, hi - x)                 # lower-right edge
    for c in [eraser, *bands, base]:
        (x0, y0), (x1, _) = cross(c)
        for i in range(x1 - x0 + 1):
            put(x0 + i, y0 + i)
    # Wooden cone from the base line down to the graphite, a solid t×t square in the corner.
    t = graphite

    def line(x0, y0, x1, y1):
        n = max(abs(x1 - x0), abs(y1 - y0))
        for i in range(n + 1):
            put(round(x0 + (x1 - x0) * i / n), round(y0 + (y1 - y0) * i / n))

    (bx0, by0), (bx1, by1) = cross(base)
    line(bx0, by0, 0, M - 1 - t)
    line(bx1, by1, t, M - 1)
    for y in range(M - t, M):
        for x in range(t):
            put(x, y)
    # Eraser: filled between the last band and the cap.
    band = max(bands)
    for y in range(M):
        for x in range(M):
            if band < x - y < eraser and lo < x + y < hi:
                put(x, y, "#")
    return "\n".join("".join(r) for r in g)

# App icon: a 20-cell pencil centred in the tile.
PENCIL_APP = pencil(20, 4, eraser=13, bands=(9, 7), base=-9, graphite=3)
ox, oy = 6, 6
frame = run(PENCIL_APP, ox, oy, "X")
fill = run(PENCIL_APP, ox, oy, "#")

svg=f'''<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <!-- Omanote app icon: a pixel pencil in 1-cell lines, drawn on the Omarchy
       logo grid (square cells, stepped corners) in terminal green. -->
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
      <feGaussianBlur stdDeviation="10" result="b"/>
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
    <rect width="1024" height="1024" fill="url(#grid)"/>
    <circle cx="512" cy="512" r="430" fill="url(#haze)"/>
    <g filter="url(#glow)" shape-rendering="crispEdges">
      <path d="{fill}" fill="{G}" fill-opacity="0.45"/>
      <path d="{frame}" fill="{G}"/>
    </g>
    <rect width="1024" height="1024" fill="url(#scan)"/>
  </g>
  <path d="{tile}" fill="none" stroke="{G}" stroke-opacity="0.4" stroke-width="6" shape-rendering="crispEdges"/>
</svg>
'''
open(DESIGN / "icon.svg", "w").write(svg)

# Tray: same page, monochrome white, 64x64 (template image on macOS).
T=4
def tcells(art):
    d=""
    for y,row in enumerate(art.strip().split("\n")):
        row=row.strip(); x=0
        while x<len(row):
            if row[x]=="X":
                n=1
                while x+n<len(row) and row[x+n]=="X": n+=1
                d+=f"M{x*T} {y*T}h{n*T}v{T}h-{n*T}z"; x+=n
            else: x+=1
    return d
tray = pencil(16, 3, eraser=10, bands=(6,), base=-8, graphite=2).replace("#", ".")
open(DESIGN / "tray.svg", "w").write(f'''<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">
  <!-- Menu bar / tray icon: the pixel pencil, monochrome. On macOS it is a template
       image (tinted to match the menu bar). -->
  <path d="{tcells(tray)}" fill="#fff" shape-rendering="crispEdges"/>
</svg>
''')

# Omarchy bar widget: a smaller pencil (13 cells) centred on the 16-cell canvas,
# so it matches the optical size of the other bar icons while each cell stays
# one pixel. Written into omarchy-plugin/PencilIcon.qml.
def pad(art, n, dx, dy):
    rows = art.split("\n")
    out = ["." * n for _ in range(dy)] + ["." * dx + r + "." * (n - dx - len(r)) for r in rows]
    return (out + ["." * n] * n)[:n]

bar = pad(pencil(13, 2, eraser=8, bands=(4,), base=-6, graphite=2).replace("#", "."), 16, 2, 1)
qml = DESIGN.parent / "omarchy-plugin" / "PencilIcon.qml"
src = qml.read_text()
a = src.index("  readonly property var art: [")
b = src.index("  ]", a) + 3
qml.write_text(src[:a] + "  readonly property var art: [\n" + ",\n".join(f'    "{r}"' for r in bar) + "\n  ]" + src[b:])
