"""Dannex Download Manager's icon, "Comet": a file arriving from far away, fast. A white-hot head
falls towards the lower right of a night-sky tile, its tail three streaks (the parallel
connections a download is fetched over) converging into it. Ember colours on night; no letter.

Drawn as SVG from geometry (written to tools/icon/comet/, the sources) and rendered by resvg:

- the full drawing (a 1000-unit square: the tile with its light, a halo round the head, faint
  stars, a shadow under the tile), used from 40 px up, and a square form of it without the
  tile's shape or shadow for places that cut their own (an iPhone's home screen);
- pixel masters at 16, 20, 24 and 32 px: the same shapes placed for whole pixels (the tile
  filling the square, the tail one tapered streak, the side streaks only from 32 px);
- the mark alone (MONO): the head and its tail, held apart by a ring of nothing, one colour on
  nothing (the notification area's classic icon, inked white or dark by the app; macOS's
  template image), at 16, 20, 24, 32 px and full size;
- the extension's off look (the toolbar button while it takes no downloads): the pixel master
  in grey and a dark badge with a white X in the top right corner, the night sky's empty corner;
- the installer's wizard panels: a tall night sky with the comet for the Welcome and Finished
  pages, and the icon for the header.

    python tools/icon/comet_logo.py --size 256 --out x.png
    python tools/icon/comet_logo.py --mark --size 32 --out mark.png

tools/icon/build_icon.py imports render(), render_mark(), render_full() and off_icon() for every
size the app, its installers, the extension and the website use. Needs Pillow and resvg 0.45
(cargo install resvg --version 0.45.1 --root target/icon-tools --locked, or RESVG=<path>).
"""

import argparse
import math
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SRC = Path(__file__).resolve().parent / "comet"
MASTER = ROOT / "art" / "Dannex Download Manager.png"
MASTER_SIZE = 1024

# The palette (also the website's and the installer's).
NIGHT = "#1E2A66"
NIGHT_MID = "#111842"
NIGHT_DEEP = "#070A1E"
HEAD = "#FFF7E6"
AMBER = "#FFB23E"
EMBER = "#FF6A45"
PLUM = "#B43A8C"


def resvg_exe() -> str:
    env = os.environ.get("RESVG")
    if env:
        return env
    local = ROOT / "target" / "icon-tools" / "bin" / ("resvg.exe" if os.name == "nt" else "resvg")
    if local.exists():
        return str(local)
    found = shutil.which("resvg")
    if found:
        return found
    raise SystemExit("resvg not found: cargo install resvg --version 0.45.1 --root target/icon-tools --locked")


# ---- SVG helpers -------------------------------------------------------------------------------

def f(v: float) -> str:
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s == "-0" else s


def svg_doc(w: float, body: str, defs: str = "", h: float | None = None) -> str:
    h = w if h is None else h
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{f(w)}" height="{f(h)}" '
            f'viewBox="0 0 {f(w)} {f(h)}">\n'
            + (f"<defs>\n{defs}\n</defs>\n" if defs else "") + body + "\n</svg>\n")


def _stops(stops) -> str:
    out = []
    for st in stops:
        o, c, a = (tuple(st) + (1,))[:3]
        out.append(f'<stop offset="{o}" stop-color="{c}"' + (f' stop-opacity="{a}"' if a != 1 else "") + "/>")
    return "".join(out)


def lin(id_, x1, y1, x2, y2, stops) -> str:
    return (f'<linearGradient id="{id_}" x1="{f(x1)}" y1="{f(y1)}" x2="{f(x2)}" y2="{f(y2)}" '
            f'gradientUnits="userSpaceOnUse">{_stops(stops)}</linearGradient>')


def rad(id_, cx, cy, r, stops) -> str:
    return (f'<radialGradient id="{id_}" cx="{f(cx)}" cy="{f(cy)}" r="{f(r)}" '
            f'gradientUnits="userSpaceOnUse">{_stops(stops)}</radialGradient>')


def blur(id_, sd) -> str:
    return (f'<filter id="{id_}" x="-50%" y="-50%" width="200%" height="200%">'
            f'<feGaussianBlur stdDeviation="{f(sd)}"/></filter>')


def poly(pts) -> str:
    return "M" + " L".join(f"{f(x)},{f(y)}" for x, y in pts) + " Z"


def squircle(cx, cy, r, n=5.0, steps=240) -> str:
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, s = math.cos(t), math.sin(t)
        pts.append((cx + r * math.copysign(abs(c) ** (2 / n), c), cy + r * math.copysign(abs(s) ** (2 / n), s)))
    return poly(pts)


def rrect(x0, y0, x1, y1, r) -> str:
    return (f"M{f(x0 + r)},{f(y0)} H{f(x1 - r)} A{f(r)},{f(r)} 0 0 1 {f(x1)},{f(y0 + r)} V{f(y1 - r)} "
            f"A{f(r)},{f(r)} 0 0 1 {f(x1 - r)},{f(y1)} H{f(x0 + r)} A{f(r)},{f(r)} 0 0 1 {f(x0)},{f(y1 - r)} "
            f"V{f(y0 + r)} A{f(r)},{f(r)} 0 0 1 {f(x0 + r)},{f(y0)} Z")


# ---- the comet's shapes ------------------------------------------------------------------------

def tail_poly(c, r, tip, steps=60):
    """The tail: from the tip, tangent to the head, round its far side."""
    cx, cy = c
    tx, ty = tip
    d = math.hypot(cx - tx, cy - ty)
    base = math.atan2(ty - cy, tx - cx)  # from the centre towards the tip
    a = math.acos(min(1.0, r / d))
    pts = [tip]
    for i in range(steps + 1):
        t = base + a + (2 * math.pi - 2 * a) * i / steps
        pts.append((cx + r * math.cos(t), cy + r * math.sin(t)))
    return pts


def streak(c, r, tip, offset, length, width):
    """A side streak, parallel to the tail, offset sideways by offset*r: a round blunt end beside
    the head, tapering to a point length*|tail| back."""
    cx, cy = c
    tx, ty = tip
    L = math.hypot(cx - tx, cy - ty)
    ux, uy = (tx - cx) / L, (ty - cy) / L  # towards the tip
    vx, vy = -uy, ux  # sideways
    sx, sy = cx + vx * offset * r + ux * 0.15 * r, cy + vy * offset * r + uy * 0.15 * r
    ex, ey = sx + ux * length * L, sy + uy * length * L
    w = width / 2
    pts = []
    for i in range(13):
        t = math.pi * i / 12
        pts.append((sx + w * (vx * math.cos(t) - ux * math.sin(t)), sy + w * (vy * math.cos(t) - uy * math.sin(t))))
    pts.append((ex, ey))
    return pts


def shapes(g):
    c, r, tip = g["c"], g["r"], g["tip"]
    sides = []
    if g.get("side"):
        w, la, lb = g["side"]
        sides = [streak(c, r, tip, 1.45, la, w), streak(c, r, tip, -1.45, lb, w)]
    return tail_poly(c, r, tip), sides


# c: the head's centre, r: its radius, tip: the tail's far end; side: the side streaks (width,
# length of the upper one and of the lower one, as shares of the tail) or None; gap: the mark's
# ring of nothing round the head; tile: its margin and corner radius.
PX = {
    16: dict(tile=(0, 3.5), c=(10.5, 10.5), r=3.3, tip=(2.0, 2.0), side=None, gap=1.0),
    20: dict(tile=(0, 4.5), c=(13.0, 13.0), r=4.1, tip=(2.5, 2.5), side=None, gap=1.1),
    24: dict(tile=(0, 5), c=(15.5, 15.5), r=4.8, tip=(3.0, 3.0), side=None, gap=1.2),
    32: dict(tile=(1, 7), c=(20.5, 20.5), r=6.2, tip=(4.5, 4.5), side=(2.2, 0.6, 0.46), gap=1.6),
}
FULL = dict(c=(630, 630), r=150, tip=(150, 150), side=(46, 0.64, 0.5), gap=34)


def svg_px(size) -> str:
    g = PX[size]
    m, rr = g["tile"]
    c, r, tip = g["c"], g["r"], g["tip"]
    tail, sides = shapes(g)
    defs = "\n".join([
        lin("t", 0, 0, size, size, [(0, NIGHT), (1, NIGHT_DEEP)]),
        lin("tail", tip[0], tip[1], c[0], c[1], [(0, PLUM, 0.0), (0.45, EMBER, 0.9), (1, AMBER)]),
        rad("head", c[0] - r * 0.25, c[1] - r * 0.25, r * 1.25, [(0, "#FFFFFF"), (0.55, "#FFF1CF"), (1, "#FFC35A")]),
    ])
    body = (f'<path d="{rrect(m, m, size - m, size - m, rr)}" fill="url(#t)"/>'
            + "".join(f'<path d="{poly(s)}" fill="url(#tail)"/>' for s in sides)
            + f'<path d="{poly(tail)}" fill="url(#tail)"/>'
            + f'<circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(r)}" fill="url(#head)"/>')
    return svg_doc(size, body, defs)


STARS = [(250, 690, 9, 0.55), (770, 250, 7, 0.45), (380, 830, 5, 0.35), (860, 430, 5, 0.3), (560, 140, 6, 0.4)]


def comet_group(g, ids="") -> tuple[str, str]:
    """The comet (halo, side streaks, tail, head) in the full drawing's units, and its defs."""
    c, r, tip = g["c"], g["r"], g["tip"]
    tail, sides = shapes(g)
    defs = "\n".join([
        lin(f"tail{ids}", tip[0], tip[1], c[0], c[1],
            [(0, PLUM, 0.0), (0.35, "#E2466E", 0.55), (0.7, EMBER, 0.92), (1, AMBER)]),
        lin(f"side{ids}", tip[0], tip[1], c[0], c[1], [(0, PLUM, 0.0), (0.5, EMBER, 0.55), (1, AMBER, 0.95)]),
        rad(f"head{ids}", c[0] - r * 0.3, c[1] - r * 0.3, r * 1.2,
            [(0, "#FFFFFF"), (0.5, "#FFF3D6"), (0.85, "#FFCD6E"), (1, AMBER)]),
        rad(f"halo{ids}", c[0], c[1], r * 2.6, [(0, AMBER, 0.45), (0.4, "#FF7A45", 0.18), (1, EMBER, 0)]),
    ])
    body = "\n".join([
        f'<circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(r * 2.6)}" fill="url(#halo{ids})"/>',
        "".join(f'<path d="{poly(s)}" fill="url(#side{ids})"/>' for s in sides),
        f'<path d="{poly(tail)}" fill="url(#tail{ids})"/>',
        f'<circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(r)}" fill="url(#head{ids})"/>',
    ])
    return defs, body


def svg_full(square=False) -> str:
    """The full drawing; square: the tile fills the picture, without its shape, rim or shadow."""
    tile = rrect(0, 0, 1000, 1000, 0) if square else squircle(500, 500, 460)
    cdefs, comet = comet_group(FULL)
    defs = "\n".join([
        lin("t", 80, 40, 920, 960, [(0, NIGHT), (0.55, NIGHT_MID), (1, NIGHT_DEEP)]),
        lin("gloss", 0, 40, 0, 560, [(0, "#FFFFFF", 0.12), (1, "#FFFFFF", 0)]),
        blur("soft", 18),
        f'<clipPath id="clip-tile"><path d="{tile}"/></clipPath>',
        cdefs,
    ])
    body = [] if square else [
        f'<path d="{tile}" fill="#02040F" opacity="0.4" filter="url(#soft)" transform="translate(0 14)"/>']
    body += [
        f'<path d="{tile}" fill="url(#t)"/>',
        '<g clip-path="url(#clip-tile)">',
        "".join(f'<circle cx="{x}" cy="{y}" r="{rr}" fill="#FFFFFF" opacity="{o}"/>' for x, y, rr, o in STARS),
        '<ellipse cx="500" cy="60" rx="760" ry="440" fill="url(#gloss)"/>',
        comet,
        "</g>",
    ]
    if not square:
        body.append(f'<path d="{tile}" fill="none" stroke="#8FA2FF" stroke-opacity="0.28" stroke-width="6"/>')
    return svg_doc(1000, "\n".join(body), defs)


def mono_body(g, ink, view) -> tuple[str, str]:
    """One ink: the head, and the tail held off it by a ring of nothing, so the two read apart."""
    c, r = g["c"], g["r"]
    tail, sides = shapes(g)
    defs = (f'<mask id="gap" maskUnits="userSpaceOnUse" x="0" y="0" width="{f(view)}" height="{f(view)}">'
            f'<rect width="{f(view)}" height="{f(view)}" fill="#fff"/>'
            f'<circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(r + g["gap"])}" fill="#000"/></mask>')
    body = ('<g mask="url(#gap)">' + "".join(f'<path d="{poly(s)}" fill="{ink}"/>' for s in sides)
            + f'<path d="{poly(tail)}" fill="{ink}"/></g>'
            + f'<circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(r)}" fill="{ink}"/>')
    return defs, body


def svg_mono_px(size, ink="#FFFFFF") -> str:
    defs, body = mono_body(PX[size], ink, size)
    return svg_doc(size, body, defs)


def svg_mono_full(ink="#FFFFFF") -> str:
    defs, body = mono_body(FULL, ink, 1000)
    # Without the tile the mark fills the square.
    return svg_doc(1000, f'<g transform="translate(500 500) scale(1.12) translate(-470 -470)">{body}</g>', defs)


# The off look's badge, per size: its diameter, its white rim, then the X: at 16 px whole pixels
# (two wide, six across: X_16), larger as lines (arm, width).
OFF_BADGE = {16: (10, 1.0, None, None), 24: (13, 1.0, 2.6, 2.2), 32: (17, 1.5, 3.4, 2.8)}
X_16 = ["XX..XX",
        ".XXXX.",
        "..XX..",
        "..XX..",
        ".XXXX.",
        "XX..XX"]


def svg_off(size) -> str:
    """The toolbar button while it takes no downloads: the icon in grey (its luminance mapped to
    96..226 of 255, a little transparent), a dark badge with a white X in the top right corner,
    where Chrome's and Firefox's own badge never goes and the night sky is empty."""
    inner = svg_px(size)
    defs_at = inner.index("<defs>") + len("<defs>")
    defs_end = inner.index("</defs>")
    inner_defs = inner[defs_at:defs_end]
    inner_body = inner[defs_end + len("</defs>"):inner.rindex("</svg>")]
    k = 0.51
    row = " ".join(f"{v * k:.4f}" for v in (0.2126, 0.7152, 0.0722)) + " 0 0.376"
    grey = (f'<filter id="off-grey" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">'
            f'<feColorMatrix type="matrix" values="{row} {row} {row} 0 0 0 0.9 0"/></filter>')
    d, rim, arm, w = OFF_BADGE[size]
    cx, cy = size - d / 2, d / 2
    badge = (f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(d / 2)}" fill="#FFFFFF"/>'
             f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(d / 2 - rim)}" fill="#2B2B2E"/>')
    if size == 16:
        # Lines would blur into a grey blot here: an X of whole pixels, two wide, six across.
        x0, y0 = round(cx) - 3, round(cy) - 3
        badge += "".join(f'<rect x="{x0 + i}" y="{y0 + j}" width="1" height="1" fill="#FFFFFF"/>'
                         for j, row in enumerate(X_16) for i, ch in enumerate(row) if ch == "X")
    else:
        badge += (f'<path d="M{f(cx - arm)},{f(cy - arm)} L{f(cx + arm)},{f(cy + arm)} '
                  f'M{f(cx + arm)},{f(cy - arm)} L{f(cx - arm)},{f(cy + arm)}" '
                  f'stroke="#FFFFFF" stroke-width="{f(w)}" stroke-linecap="round"/>')
    return svg_doc(size, f'<g filter="url(#off-grey)">{inner_body}</g>\n{badge}', inner_defs + "\n" + grey)


# ---- the drop target ---------------------------------------------------------------------------

def svg_drop_target(hot=False) -> str:
    """Tasks → Show drop target's face: the night tile filling its small square window (Windows 11
    rounds the window's corners and draws its border), the comet a little smaller than on the icon
    so it has room round it, faint stars, the tile's rim. `hot`: something droppable is over it:
    the head burns brighter, a warm glow rises behind it and the tile takes the double amber rule
    of the website's night fields (2 px, then a 1 px line 2 px inside it, at 56 px), its corners
    following the window's 8 px ones. The app draws it at the window's size on screen (an SVG, so
    crisp at every scale): crates/app/assets/icons/app/drop-target*.svg, written by --drop-target."""
    g = dict(FULL)
    cdefs, comet = comet_group(g, ids="d")
    if hot:
        # The halo hotter and wider: amber at its heart.
        cdefs = cdefs.replace(f'stop-color="{AMBER}" stop-opacity="0.45"', f'stop-color="{AMBER}" stop-opacity="0.8"')
    px = 1000 / 56  # one pixel of the 56 px window
    defs = [
        lin("t", 80, 40, 920, 960, [(0, NIGHT), (0.55, NIGHT_MID), (1, NIGHT_DEEP)]),
        lin("gloss", 0, 40, 0, 560, [(0, "#FFFFFF", 0.12), (1, "#FFFFFF", 0)]),
        cdefs,
    ]
    if hot:
        defs.append(rad("warm", 612, 612, 560, [(0, AMBER, 0.3), (0.5, EMBER, 0.12), (1, EMBER, 0)]))
    body = [
        '<rect width="1000" height="1000" fill="url(#t)"/>',
        "".join(f'<circle cx="{x}" cy="{y}" r="{rr * 1.6}" fill="#FFFFFF" opacity="{o}"/>' for x, y, rr, o in STARS),
        '<ellipse cx="500" cy="60" rx="760" ry="440" fill="url(#gloss)"/>',
    ]
    if hot:
        body.append('<rect width="1000" height="1000" fill="url(#warm)"/>')
    body.append(f'<g transform="translate(500 500) scale(0.86) translate(-500 -500)">{comet}</g>')
    if hot:
        outer, inner = 2 * px, 5.5 * px  # the rules' centre lines, in from the edge
        body.append(f'<path d="{rrect(outer, outer, 1000 - outer, 1000 - outer, 8 * px - outer)}" fill="none" '
                    f'stroke="{AMBER}" stroke-width="{f(2 * px)}"/>')
        body.append(f'<path d="{rrect(inner, inner, 1000 - inner, 1000 - inner, 8 * px - inner)}" fill="none" '
                    f'stroke="{AMBER}" stroke-opacity="0.7" stroke-width="{f(px)}"/>')
    else:
        body.append(f'<path d="{rrect(px / 2, px / 2, 1000 - px / 2, 1000 - px / 2, 8 * px - px / 2)}" fill="none" '
                    f'stroke="#8FA2FF" stroke-opacity="0.3" stroke-width="{f(px)}"/>')
    return svg_doc(1000, "\n".join(body), "\n".join(defs))


def write_drop_target(folder: Path) -> None:
    for hot, name in ((False, "drop-target.svg"), (True, "drop-target-hot.svg")):
        (folder / name).write_text(svg_drop_target(hot), "utf-8", newline="\n")


# ---- the installer's wizard panels -------------------------------------------------------------

def svg_wizard_side(w, h) -> str:
    """The Welcome and Finished pages' tall panel: a night sky, faint stars, the comet crossing it
    from the upper left towards a warm glow low on the panel. No words: those are translated."""
    sx, sy = w / 164, h / 314  # drawn on a 164x314 grid, the panel's aspect
    k = min(sx, sy)
    stars = [(22, 36, 1.1, 0.55), (131, 24, 0.9, 0.5), (96, 62, 0.7, 0.4), (144, 108, 1.0, 0.45),
             (30, 150, 0.7, 0.35), (120, 182, 0.8, 0.35), (58, 238, 0.9, 0.3), (148, 262, 0.7, 0.3),
             (14, 280, 0.8, 0.28), (70, 18, 0.6, 0.35), (152, 60, 0.6, 0.3), (40, 96, 0.5, 0.3)]
    g = dict(c=(104, 176), r=15, tip=(-30, 20), side=(4.4, 0.6, 0.46))
    cdefs, comet = comet_group(g, ids="w")
    defs = "\n".join([
        lin("sky", 0, 0, 0, 314, [(0, "#232F70"), (0.5, NIGHT_MID), (1, NIGHT_DEEP)]),
        rad("glow", 104, 330, 150, [(0, EMBER, 0.32), (0.45, PLUM, 0.14), (1, PLUM, 0)]),
        cdefs,
    ])
    body = "\n".join([
        f'<g transform="scale({f(sx)} {f(sy)})">',
        '<rect width="164" height="314" fill="url(#sky)"/>',
        '<rect width="164" height="314" fill="url(#glow)"/>',
        "".join(f'<circle cx="{x}" cy="{y}" r="{f(r * k / sx)}" fill="#FFFFFF" opacity="{o}"/>' for x, y, r, o in stars),
        comet,
        "</g>",
    ])
    return svg_doc(w, body, defs, h)


# The image areas' sizes at 100, 125, 150, 175, 200, 225 and 250% (Inno Setup 6.6 and later,
# its help: WizardImageFile, WizardSmallImageFile).
WIZARD_SIDE = [(202, 386), (269, 515), (336, 643), (403, 772), (430, 824), (498, 953), (534, 1022)]
WIZARD_SMALL = [58, 77, 97, 116, 124, 143, 159]


# ---- rendering ---------------------------------------------------------------------------------

def rasterise(svg: str, w: int, h: int | None = None, keep: Path | None = None) -> Image.Image:
    """`svg` rendered at w x h by resvg (kept as `keep` when given, the source others render)."""
    h = w if h is None else h
    if keep:
        keep.parent.mkdir(parents=True, exist_ok=True)
        keep.write_text(svg, "utf-8", newline="\n")
    with tempfile.TemporaryDirectory() as tmp:
        src = keep or Path(tmp) / "in.svg"
        if not keep:
            src.write_text(svg, "utf-8")
        out = Path(tmp) / "out.png"
        subprocess.run([resvg_exe(), "-w", str(w), "-h", str(h), str(src), str(out)], check=True)
        return Image.open(out).convert("RGBA").copy()


def render_full(size, square=False):
    return rasterise(svg_full(square), size, keep=SRC / ("comet-square.svg" if square else "comet.svg"))


def render_macos(size):
    """The full drawing on macOS's icon grid (since Big Sur): the tile 824 of 1024 wide, centred,
    so the Dock shows it the size of the others."""
    svg = svg_full()
    body_at = svg.index("</defs>") + len("</defs>")
    scale = 824 / 920  # the full drawing's tile is 920 of 1000 wide (squircle radius 460)
    svg = (svg[:body_at] + f'\n<g transform="translate(500 500) scale({scale:.4f}) translate(-500 -500)">'
           + svg[body_at:svg.rindex("</svg>")] + "</g>\n</svg>\n")
    return rasterise(svg, size, keep=SRC / "comet-macos.svg")


def render(size):
    """The icon at a size: its pixel master where there is one, the full drawing otherwise."""
    if size in PX:
        return rasterise(svg_px(size), size, keep=SRC / f"comet-{size}.svg")
    return render_full(size)


def render_mark(size):
    """The mark alone, white on nothing: its pixel master where there is one, else the full one."""
    if size in PX:
        return rasterise(svg_mono_px(size), size, keep=SRC / f"comet-mono-{size}.svg")
    return rasterise(svg_mono_full(), size, keep=SRC / "comet-mono.svg")


def off_icon(size):
    return rasterise(svg_off(size), size, keep=SRC / f"comet-off-{size}.svg")


def wizard_side(w, h):
    return rasterise(svg_wizard_side(w, h), w, h, keep=SRC / "wizard-side.svg" if (w, h) == WIZARD_SIDE[0] else None)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--size", type=int, default=MASTER_SIZE)
    ap.add_argument("--out", type=Path, default=MASTER)
    ap.add_argument("--mark", action="store_true", help="the mark alone (white on nothing)")
    ap.add_argument("--drop-target", action="store_true",
                    help="write the drop target's two SVGs into crates/app/assets/icons/app")
    args = ap.parse_args()
    if args.drop_target:
        write_drop_target(ROOT / "crates" / "app" / "assets" / "icons" / "app")
        print("wrote drop-target.svg and drop-target-hot.svg")
        return
    (render_mark(args.size) if args.mark else render(args.size)).save(args.out, optimize=True)
    print(f"wrote {args.out} ({args.size} px)")


if __name__ == "__main__":
    main()
