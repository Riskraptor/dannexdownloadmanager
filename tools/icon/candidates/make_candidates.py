"""Four candidate marks for Dannex Download Manager's new icon, as SVG, and a contact sheet each.

Kept as the record of the choice: the owner chose D · Comet, whose drawing now lives in
tools/icon/comet_logo.py (the source tools/icon/build_icon.py renders every size from). The
contact sheets aren't kept in the repository; this script draws them again.

Every candidate is drawn three ways, all vector:

- a full master (a 1000-unit square, with its light: gradients, gloss, shadow), used from 48 px up;
- pixel masters at 16, 24 and 32 px, the same shapes placed by hand on whole pixels so their edges
  stay sharp in a browser toolbar, a tray or a title bar (simplified where the full one has
  detail a few pixels can't carry);
- a mono master (one ink on nothing, for the tray's classic style and macOS's template image) at
  16, 24 and 32 px, and its full-size form;

plus the extension's "off" look at 16, 24 and 32 px (the pixel master in light grey, a dark disc
with a white X in its top right corner, where Chrome's and Firefox's badge never goes), also SVG.

Writes, for each candidate <name>, into tools/icon/candidates/<name>/:
  <name>.svg                   the full master
  <name>-16.svg, -24, -32      the pixel masters
  <name>-mono.svg, -mono-16/24/32.svg
  <name>-off-16/24/32.svg
  <name>-sheet.png             the contact sheet
and tools/icon/candidates/overview.png (all four at 16, 32 and 128 px on light and dark).
Intermediate PNGs go to target/icon-candidates/.

Needs Pillow and resvg 0.45 (the SVG renderer the workspace already depends on, as a command):
    cargo install resvg --version 0.45.1 --root target/icon-tools --locked
    python tools/icon/candidates/make_candidates.py
(or set RESVG to a resvg executable). Fonts for the sheets' captions: Windows' Segoe UI.
"""

import math
import os
import shutil
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
WORK = ROOT / "target" / "icon-candidates"
PX = (16, 24, 32)
SIZES = (16, 24, 32, 48, 64, 128, 256)
FONTS = Path("C:/Windows/Fonts")


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
    """A coordinate, short."""
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s == "-0" else s


def svg_doc(view: float, body: str, defs: str = "") -> str:
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{f(view)}" height="{f(view)}" '
            f'viewBox="0 0 {f(view)} {f(view)}">\n'
            + (f"<defs>\n{defs}\n</defs>\n" if defs else "") + body + "\n</svg>\n")


def _stops(stops) -> str:
    out = []
    for st in stops:
        o, c, a = (tuple(st) + (1,))[:3]
        out.append(f'<stop offset="{o}" stop-color="{c}"' + (f' stop-opacity="{a}"' if a != 1 else "") + "/>")
    return "".join(out)


def lin(id_, x1, y1, x2, y2, stops, user=True) -> str:
    units = ' gradientUnits="userSpaceOnUse"' if user else ""
    return (f'<linearGradient id="{id_}" x1="{f(x1)}" y1="{f(y1)}" x2="{f(x2)}" y2="{f(y2)}"{units}>'
            f"{_stops(stops)}</linearGradient>")


def rad(id_, cx, cy, r, stops) -> str:
    return (f'<radialGradient id="{id_}" cx="{f(cx)}" cy="{f(cy)}" r="{f(r)}" '
            f'gradientUnits="userSpaceOnUse">{_stops(stops)}</radialGradient>')


def blur(id_, sd) -> str:
    return (f'<filter id="{id_}" x="-50%" y="-50%" width="200%" height="200%">'
            f'<feGaussianBlur stdDeviation="{f(sd)}"/></filter>')


def poly(pts) -> str:
    return "M" + " L".join(f"{f(x)},{f(y)}" for x, y in pts) + " Z"


def polyline(pts) -> str:
    return "M" + " L".join(f"{f(x)},{f(y)}" for x, y in pts)


def rounded_poly(pts, r) -> str:
    """A closed polygon with every corner rounded by up to r (a circular arc tangent to both
    edges; smaller where an edge is too short). r: one radius, or one per corner."""
    n = len(pts)
    rs = list(r) if isinstance(r, (list, tuple)) else [r] * n
    d = []
    for i in range(n):
        p0, p1, p2 = pts[i - 1], pts[i], pts[(i + 1) % n]
        v1 = (p0[0] - p1[0], p0[1] - p1[1])
        v2 = (p2[0] - p1[0], p2[1] - p1[1])
        l1, l2 = math.hypot(*v1), math.hypot(*v2)
        u1, u2 = (v1[0] / l1, v1[1] / l1), (v2[0] / l2, v2[1] / l2)
        ang = math.acos(max(-1.0, min(1.0, u1[0] * u2[0] + u1[1] * u2[1])))
        ri = rs[i]
        op = "M" if i == 0 else "L"
        if ri <= 0 or ang < 1e-3 or abs(ang - math.pi) < 1e-3:
            d.append(f"{op}{f(p1[0])},{f(p1[1])}")
            continue
        t = min(ri / math.tan(ang / 2), l1 * 0.5, l2 * 0.5)
        ri = t * math.tan(ang / 2)
        a = (p1[0] + u1[0] * t, p1[1] + u1[1] * t)
        b = (p1[0] + u2[0] * t, p1[1] + u2[1] * t)
        sweep = 1 if (u1[0] * u2[1] - u1[1] * u2[0]) < 0 else 0
        d.append(f"{op}{f(a[0])},{f(a[1])}")
        d.append(f"A{f(ri)},{f(ri)} 0 0 {sweep} {f(b[0])},{f(b[1])}")
    return " ".join(d) + " Z"


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


def d_outline(L, T, B, C, rx, n, rc=0.0, steps=120):
    """A D's outline as points: straight left side (corners rounded by rc), flat top and bottom to
    C, then a half-superellipse bowl rx wide."""
    M, ry = (T + B) / 2, (B - T) / 2
    pts = [(L + rc, T), (C, T)]
    for i in range(1, steps):
        th = -math.pi / 2 + math.pi * i / steps
        c, s = math.cos(th), math.sin(th)
        pts.append((C + rx * abs(c) ** (2 / n), M + ry * math.copysign(abs(s) ** (2 / n), s)))
    pts += [(C, B), (L + rc, B)]
    if rc:
        k = 10
        for i in range(1, k):
            a = math.pi / 2 + (math.pi / 2) * i / k
            pts.append((L + rc + rc * math.cos(a), B - rc + rc * math.sin(a)))
        pts.append((L, T + rc))
        for i in range(1, k):
            a = math.pi + (math.pi / 2) * i / k
            pts.append((L + rc + rc * math.cos(a), T + rc + rc * math.sin(a)))
    else:
        pts.append((L, T))
    return pts


def d_rounded(L, T, B, R, r) -> str:
    """A D on whole pixels: a rectangle whose two right corners are rounded by r."""
    return (f"M{f(L)},{f(T)} H{f(R - r)} A{f(r)},{f(r)} 0 0 1 {f(R)},{f(T + r)} V{f(B - r)} "
            f"A{f(r)},{f(r)} 0 0 1 {f(R - r)},{f(B)} H{f(L)} Z")


# The extension's off look, as SVG: the icon in light grey (its luminance mapped to 96..226 of
# 255, a little transparent: build_icon.py's off_icon recipe), and a dark disc with a white X in
# the top right corner, placed on the pixel grid so the X stays an X at 16 px.
OFF_BADGE = {16: (9, 1.0, 1.9, 1.5), 24: (12, 1.0, 2.5, 2.0), 32: (16, 1.5, 3.4, 2.5)}


def off_svg(size: int, inner: str, inner_defs: str) -> str:
    k = 0.51
    row = " ".join(f"{v * k:.4f}" for v in (0.2126, 0.7152, 0.0722)) + " 0 0.376"
    grey = (f'<filter id="off-grey" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">'
            f'<feColorMatrix type="matrix" values="{row} {row} {row} 0 0 0 0.9 0"/></filter>')
    d, pad, arm, w = OFF_BADGE[size]
    cx, cy = size - d / 2, d / 2
    badge = (f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(d / 2)}" fill="#ffffff"/>'
             f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(d / 2 - pad)}" fill="#3a3a3a"/>')
    if size == 16:
        # At 16 px an X drawn with lines blurs into a grid: five whole pixels each way instead.
        px, py = int(cx), int(cy)
        cells = {(px + i, py + i) for i in range(-2, 3)} | {(px + i, py - i) for i in range(-2, 3)}
        badge += "".join(f'<rect x="{x}" y="{y}" width="1" height="1" fill="#ffffff"/>' for x, y in sorted(cells))
    else:
        badge += (f'<path d="M{f(cx - arm)},{f(cy - arm)} L{f(cx + arm)},{f(cy + arm)} '
                  f'M{f(cx + arm)},{f(cy - arm)} L{f(cx - arm)},{f(cy + arm)}" '
                  f'stroke="#ffffff" stroke-width="{f(w)}" stroke-linecap="round"/>')
    return svg_doc(size, f'<g filter="url(#off-grey)">{inner}</g>\n{badge}', inner_defs + "\n" + grey)


# ---- Candidate A: Slipstream -------------------------------------------------------------------
# The D in motion: three tapering trails (the parallel connections a download is fetched over)
# stream back from its stem and converge into the letter. A white D on a deep electric-blue tile,
# the website's board blue at its heart.

SLIP = {
    "name": "slipstream",
    "title": "A · Slipstream",
    "idea": ("The D in motion: three tapering trails (the parallel connections DDM fetches a file "
             "over) stream back from its stem and converge into the letter. Speed and segments in "
             "a monogram; the tile is the website's own board blue, so the site and the installer "
             "keep their colours."),
    "palette": [("Tile light", "#4C8DFF"), ("Board blue", "#1F63D6"), ("Tile deep", "#1240A8"),
                ("Glyph", "#FFFFFF"), ("Glyph shade", "#DCE7FF"), ("Ink (mono)", "#0E1B3D")],
}
# On whole pixels: the tile (margin, corner radius); the D's top and bottom; the stem's left edge;
# the D's right edge and corner radius; the counter (x0, y0, x1, y1, radius); the three trails,
# each its rows where it leaves the stem and the x of its point.
SLIP_PX = {
    16: dict(tile=(0, 3.5), T=3, B=13, sL=6, R=14, r=3, cnt=(8, 6, 12, 10, 1),
             trails=[(3, 6, 2), (7, 9, 0.5), (10, 13, 3)]),
    24: dict(tile=(0, 5), T=5, B=19, sL=10, R=21, r=4.5, cnt=(13, 9, 18, 15, 1.5),
             trails=[(5, 9, 3.5), (10, 14, 1), (15, 19, 5)]),
    32: dict(tile=(1, 7), T=7, B=25, sL=13, R=27, r=6, cnt=(17, 12, 23, 20, 2),
             trails=[(7, 12, 6), (14, 18, 3), (20, 25, 8)]),
}
SLIP_TILE = [(0, "#4C8DFF"), (0.5, "#1F63D6"), (1, "#1240A8")]


def trail(y0, y1, x_tip, x_stem):
    """A trail: as tall as its lane where it meets the stem, tapering to a point behind."""
    return poly([(x_stem, y0), (x_stem, y1), (x_tip, (y0 + y1) / 2)])


def slip_glyph_px(g, fill) -> str:
    L, T, B, R = g["sL"], g["T"], g["B"], g["R"]
    cx0, cy0, cx1, cy1, cr = g["cnt"]
    d = d_rounded(L, T, B, R, g["r"]) + " " + d_rounded(cx0, cy0, cy1, cx1, cr)
    trails = " ".join(trail(y0, y1, x, L + 0.5) for y0, y1, x in g["trails"])
    return f'<path d="{d}" fill-rule="evenodd" fill="{fill}"/><path d="{trails}" fill="{fill}"/>'


def slip_px(size):
    g = SLIP_PX[size]
    m, r = g["tile"]
    defs = lin("t", 0, 0, size, size, SLIP_TILE)
    return defs, f'<path d="{rrect(m, m, size - m, size - m, r)}" fill="url(#t)"/>' + slip_glyph_px(g, "#FFFFFF")


def slip_mono_px(size, ink="#000000"):
    return "", slip_glyph_px(SLIP_PX[size], ink)


# The full master (1000 units): the 32 px shapes scaled up and refined: a real bowl, the trails'
# long edges a little hollow and fading towards their points, the tile's light.
SLIP_FULL = dict(T=220, B=780, sL=410, C=560, R=850, cnt=dict(L=540, T=376, B=624, C=575, rx=150, n=2.3),
                 trails=[(220, 376, 190), (432, 568, 80), (624, 780, 260)])


def hollow_trail(y0, y1, x_tip, x_stem, sag=0.18, steps=24):
    """A trail whose two long edges curve in a little, like a streak of light."""
    ym = (y0 + y1) / 2
    top, bot = [], []
    for i in range(steps + 1):
        t = i / steps
        x = x_stem + (x_tip - x_stem) * t
        half = (y1 - y0) / 2 * (1 - t) * (1 - sag * math.sin(math.pi * t))
        top.append((x, ym - half))
        bot.append((x, ym + half))
    return poly(top + bot[::-1])


def slip_glyph_full(fill_main, fill_trail):
    g = SLIP_FULL
    outer = d_outline(g["sL"], g["T"], g["B"], g["C"], g["R"] - g["C"], 2.7)
    c = g["cnt"]
    inner = d_outline(c["L"], c["T"], c["B"], c["C"], c["rx"], c["n"])
    trails = " ".join(hollow_trail(y0, y1, x, g["sL"] + 2) for y0, y1, x in g["trails"])
    return (f'<path d="{trails}" fill="{fill_trail}"/>'
            f'<path d="{poly(outer)} {poly(inner)}" fill-rule="evenodd" fill="{fill_main}"/>')


def slip_full():
    tile = squircle(500, 500, 460)
    defs = "\n".join([
        lin("t", 60, 40, 940, 960, SLIP_TILE),
        lin("gloss", 0, 40, 0, 560, [(0, "#FFFFFF", 0.22), (1, "#FFFFFF", 0)]),
        lin("face", 0, 220, 0, 780, [(0, "#FFFFFF"), (1, "#DCE7FF")]),
        lin("trail", 80, 0, 420, 0, [(0, "#FFFFFF", 0.3), (0.6, "#FFFFFF", 0.88), (1, "#FFFFFF")]),
        rad("glow", 640, 300, 620, [(0, "#8DB8FF", 0.35), (1, "#8DB8FF", 0)]),
        blur("soft", 18),
        f'<clipPath id="clip-tile"><path d="{tile}"/></clipPath>',
    ])
    body = "\n".join([
        f'<path d="{tile}" fill="#06142F" opacity="0.35" filter="url(#soft)" transform="translate(0 14)"/>',
        f'<path d="{tile}" fill="url(#t)"/>',
        '<g clip-path="url(#clip-tile)">',
        '<rect x="0" y="0" width="1000" height="1000" fill="url(#glow)"/>',
        '<ellipse cx="500" cy="80" rx="760" ry="470" fill="url(#gloss)"/>',
        f'<g filter="url(#soft)" opacity="0.45" transform="translate(6 22)">{slip_glyph_full("#06205E", "#06205E")}</g>',
        slip_glyph_full("url(#face)", "url(#trail)"),
        "</g>",
        f'<path d="{tile}" fill="none" stroke="#FFFFFF" stroke-opacity="0.18" stroke-width="6"/>',
    ])
    return defs, body


def slip_mono_full(ink="#000000"):
    # Without the tile: the same shapes, larger, in the middle of the square.
    return "", f'<g transform="translate(500 500) scale(1.18) translate(-480 -500)">{slip_glyph_full(ink, ink)}</g>'


# ---- Candidate B: Loop -------------------------------------------------------------------------
# The D is a progress ring: drawn as one stroke that starts at the foot of the stem, runs up,
# across and round the bowl and comes home along the base, deepening from jade to ink as it goes,
# its rounded head stopping just short of where it began. A download in progress, as a letter;
# the tray's icon could fill it for real. Porcelain tile.

LOOP = {
    "name": "loop",
    "title": "B · Loop",
    "idea": ("The D is a progress ring: one stroke starts at the foot of the stem, runs up, across "
             "and round the bowl and comes home along the base, deepening from jade to ink, its "
             "round head stopping just short of where it began. A download in progress as a "
             "letter; the tray icon could fill it live. Porcelain tile."),
    "palette": [("Jade", "#2EE0C0"), ("Lagoon", "#12A7B4"), ("Cobalt", "#1F6AD6"),
                ("Ink", "#1B2A7A"), ("Porcelain", "#F4F6FA"), ("Rim", "#C9D1DD")],
}
LOOP_SWEEP = [(0, "#2EE0C0"), (0.35, "#12A7B4"), (0.7, "#1F6AD6"), (1, "#1B2A7A")]
# The D's centre line, on whole pixels: the stroke's width; the stem's x, the top's and base's y;
# where the bowl begins (C) and its width (rx) and exponent; the stem corners' radius; where the
# stroke starts on the base (xs, by the stem) and where its head's centre stops (xe).
LOOP_PX = {
    16: dict(tile=(0, 3.5), w=3, L=4.5, T=3.5, B=12.5, C=8.5, rx=4, n=2.2, rc=1.0, xs=6, xe=8.5),
    24: dict(tile=(0, 5), w=4, L=7, T=5, B=19, C=13, rx=6, n=2.2, rc=1.5, xs=9, xe=13),
    32: dict(tile=(1, 7), w=5, L=9.5, T=7.5, B=24.5, C=17, rx=8.5, n=2.2, rc=2.0, xs=12, xe=16.5),
}
LOOP_FULL = dict(w=150, L=300, T=290, B=710, C=540, rx=190, n=2.3, rc=70, xs=375, xe=530)


def lerp_colour(stops, t):
    t = max(0.0, min(1.0, t))
    for (p0, c0), (p1, c1) in zip(stops, stops[1:]):
        if t <= p1:
            q = (t - p0) / (p1 - p0) if p1 > p0 else 0
            a, b = hexrgb(c0), hexrgb(c1)
            return "#" + "".join(f"{round(a[i] + (b[i] - a[i]) * q):02x}" for i in range(3))
    return stops[-1][1]


def loop_path(g, steps=160):
    """The centre line, from the start on the base by the stem, round the D, to the head."""
    L, T, B, C, rx, n, rc = g["L"], g["T"], g["B"], g["C"], g["rx"], g["n"], g["rc"]
    pts = [(g["xs"], B), (L + rc, B)]
    k = 12
    for i in range(1, k + 1):  # the bottom left corner, then up the stem
        a = math.pi / 2 + (math.pi / 2) * i / k
        pts.append((L + rc + rc * math.cos(a), B - rc + rc * math.sin(a)))
    for i in range(0, k + 1):  # the top left corner
        a = math.pi + (math.pi / 2) * i / k
        pts.append((L + rc + rc * math.cos(a), T + rc + rc * math.sin(a)))
    pts.append((C, T))
    M, ry = (T + B) / 2, (B - T) / 2
    for i in range(1, steps):
        th = -math.pi / 2 + math.pi * i / steps
        c, s = math.cos(th), math.sin(th)
        pts.append((C + rx * abs(c) ** (2 / n), M + ry * math.copysign(abs(s) ** (2 / n), s)))
    pts += [(C, B), (g["xe"], B)]
    out = [pts[0]]
    for p in pts[1:]:
        if math.hypot(p[0] - out[-1][0], p[1] - out[-1][1]) > 1e-6:
            out.append(p)
    return out


def split_by_length(pts, pieces):
    """The polyline cut into pieces of equal length, each with its middle's share of the whole;
    every piece overlaps the next a little, so no seam shows between them."""
    seg = [math.hypot(b[0] - a[0], b[1] - a[1]) for a, b in zip(pts, pts[1:])]
    total = sum(seg)
    cum = [0.0]
    for s in seg:
        cum.append(cum[-1] + s)

    def at(d):
        d = max(0.0, min(total, d))
        for i in range(len(seg)):
            if cum[i + 1] >= d:
                q = (d - cum[i]) / seg[i] if seg[i] else 0
                return (pts[i][0] + (pts[i + 1][0] - pts[i][0]) * q, pts[i][1] + (pts[i + 1][1] - pts[i][1]) * q), i
        return pts[-1], len(seg) - 1

    out = []
    for j in range(pieces):
        a, b = total * j / pieces, total * (j + 1) / pieces + (total / pieces * 0.35 if j < pieces - 1 else 0)
        pa, ia = at(a)
        pb, ib = at(b)
        piece = [pa] + pts[ia + 1:ib + 1] + [pb]
        out.append((piece, (j + 0.5) / pieces))
    return out


def loop_ring(g, ink=None, pieces=48):
    """The ring: its pieces stroked in the sweep's colours (or one ink), a butt start, a round
    head."""
    pts = loop_path(g)
    parts = []
    for piece, t in split_by_length(pts, pieces):
        colour = ink or lerp_colour(LOOP_SWEEP, t)
        parts.append(f'<path d="{polyline(piece)}" fill="none" stroke="{colour}" stroke-width="{f(g["w"])}" '
                     f'stroke-linejoin="round" stroke-linecap="butt"/>')
    hx, hy = pts[-1]
    parts.append(f'<circle cx="{f(hx)}" cy="{f(hy)}" r="{f(g["w"] / 2)}" fill="{ink or LOOP_SWEEP[-1][1]}"/>')
    return "".join(parts)


def loop_px(size):
    g = LOOP_PX[size]
    m, r = g["tile"]
    defs = lin("t", 0, 0, 0, size, [(0, "#FFFFFF"), (1, "#E9EDF4")])
    tile = (f'<path d="{rrect(m, m, size - m, size - m, r)}" fill="url(#t)"/>'
            f'<path d="{rrect(m + 0.5, m + 0.5, size - m - 0.5, size - m - 0.5, r - 0.5)}" fill="none" '
            f'stroke="#B9C2D0" stroke-width="1"/>')
    return defs, tile + loop_ring(g, pieces=24)


def loop_mono_px(size, ink="#000000"):
    return "", loop_ring(LOOP_PX[size], ink=ink, pieces=1)


def loop_full():
    g = LOOP_FULL
    tile = squircle(500, 500, 460)
    defs = "\n".join([
        lin("t", 0, 40, 0, 960, [(0, "#FFFFFF"), (0.6, "#F4F6FA"), (1, "#E3E8F0")]),
        lin("gloss", 0, 40, 0, 520, [(0, "#FFFFFF", 0.9), (1, "#FFFFFF", 0)]),
        lin("shine", 0, g["T"] - g["w"] / 2, 0, g["B"] + g["w"] / 2,
            [(0, "#FFFFFF", 0.42), (0.45, "#FFFFFF", 0.05), (1, "#FFFFFF", 0)]),
        blur("soft", 18), blur("ringsh", 9),
        f'<clipPath id="clip-tile"><path d="{tile}"/></clipPath>',
    ])
    pts = loop_path(g)
    ring_ink = loop_ring(g, ink="#0B1A4A", pieces=1)
    hx, hy = pts[-1]
    body = "\n".join([
        f'<path d="{tile}" fill="#0B1A3A" opacity="0.28" filter="url(#soft)" transform="translate(0 14)"/>',
        f'<path d="{tile}" fill="url(#t)"/>',
        '<g clip-path="url(#clip-tile)">',
        f'<g filter="url(#ringsh)" opacity="0.22" transform="translate(0 10)">{ring_ink}</g>',
        loop_ring(g, pieces=72),
        # The enamel's light: brightest along the top, gone by the middle.
        f'<path d="{polyline(pts)}" fill="none" stroke="url(#shine)" stroke-width="{f(g["w"])}" stroke-linejoin="round"/>',
        f'<circle cx="{f(hx)}" cy="{f(hy)}" r="{f(g["w"] / 2)}" fill="url(#shine)"/>',
        "</g>",
        f'<path d="{tile}" fill="none" stroke="#B9C2D0" stroke-width="5"/>',
    ])
    return defs, body


def loop_mono_full(ink="#000000"):
    return "", f'<g transform="translate(500 500) scale(1.25) translate(-510 -500)">{loop_ring(LOOP_FULL, ink=ink, pieces=1)}</g>'


# ---- Candidate C: Delta ------------------------------------------------------------------------
# Delta is the Greek D; turned over, it points down. An inverted delta cut like a gem into three
# facets that meet at its point: the parallel streams of one download converging into one
# arrival. White on an ultraviolet tile.

DELTA = {
    "name": "delta",
    "title": "C · Delta",
    "idea": ("Delta is the Greek D; turned over, it points down. A white inverted delta cut like a "
             "gem into three facets that meet at its point: the parallel streams of one download "
             "converging into one arrival. Ultraviolet tile; plain geometry that reads at any "
             "size."),
    "palette": [("Tile light", "#8A63FF"), ("Ultraviolet", "#5B2EE6"), ("Tile deep", "#3A12B0"),
                ("Glyph", "#FFFFFF"), ("Facet light", "#E6DCFF"), ("Facet shade", "#C7B4FF")],
}
# On whole pixels: the tile (margin, corner radius); the triangle's top, tip, left and right; its
# corners' radius.
DELTA_PX = {
    16: dict(tile=(0, 3.5), T=4, tip=13, X0=2, X1=14, r=0.6),
    24: dict(tile=(0, 5), T=6, tip=20, X0=3, X1=21, r=0.9),
    32: dict(tile=(1, 7), T=8, tip=26, X0=4, X1=28, r=1.2),
}
# The mono mark has no tile, so it fills the square.
DELTA_MONO_PX = {
    16: dict(T=2, tip=15, X0=1, X1=15, r=0.8),
    24: dict(T=3, tip=22, X0=1, X1=23, r=1.0),
    32: dict(T=4, tip=30, X0=1, X1=31, r=1.5),
}
DELTA_FULL = dict(T=255, tip=800, X0=150, X1=850, r=36)
DELTA_MONO_FULL = dict(T=140, tip=900, X0=60, X1=940, r=50)
DELTA_TILE = [(0, "#8A63FF"), (0.5, "#5B2EE6"), (1, "#3A12B0")]


def delta_facets(g, id_, inks, gap=0.0, r=0.0):
    """The delta cut in three facets that meet at its point (each a third of its top edge), in
    `inks` (left, middle, right). gap: the cuts' width at the top, tapering to nothing at the
    point (a mask); r: the outer corners' radius."""
    T, tip, X0, X1 = g["T"], g["tip"], g["X0"], g["X1"]
    cx, a = (X0 + X1) / 2, (X1 - X0) / 6
    outline = rounded_poly([(X0, T), (X1, T), (cx, tip)], [r, r, r * 1.4])
    far = 40 * (X1 - X0) / 1000 + 1  # facets drawn past the outline; the outline clips them
    facets = [[(X0 - far, T - far), (cx - a, T - far), (cx - a, T), (cx, tip), (cx, tip + far), (X0 - far, tip + far)],
              [(cx - a, T - far), (cx + a, T - far), (cx + a, T), (cx, tip), (cx - a, T)],
              [(cx + a, T - far), (X1 + far, T - far), (X1 + far, tip + far), (cx, tip + far), (cx, tip), (cx + a, T)]]
    defs = f'<clipPath id="{id_}-c"><path d="{outline}"/></clipPath>'
    group = f'<g clip-path="url(#{id_}-c)"'
    if gap:
        cuts = "".join(f'<path d="{poly([(x - gap / 2, T - far), (x + gap / 2, T - far), (cx, tip)])}" fill="#000"/>'
                       for x in (cx - a, cx + a))
        defs += (f'<mask id="{id_}-m" maskUnits="userSpaceOnUse" x="{f(X0 - far)}" y="{f(T - far)}" '
                 f'width="{f(X1 - X0 + 2 * far)}" height="{f(tip - T + 2 * far)}">'
                 f'<rect x="{f(X0 - far)}" y="{f(T - far)}" width="{f(X1 - X0 + 2 * far)}" '
                 f'height="{f(tip - T + 2 * far)}" fill="#fff"/>{cuts}</mask>')
        group += f' mask="url(#{id_}-m)"'
    body = group + ">" + "".join(f'<path d="{poly(p)}" fill="{ink}"/>' for p, ink in zip(facets, inks)) + "</g>"
    return defs, body, outline


def delta_px(size):
    g = DELTA_PX[size]
    m, r = g["tile"]
    fdefs, facets, _ = delta_facets(g, "d", ("#E6DCFF", "#FFFFFF", "#C7B4FF"), gap=0.9 if size == 32 else 0,
                                    r=g["r"])
    defs = lin("t", 0, 0, size, size, DELTA_TILE) + "\n" + fdefs
    return defs, f'<path d="{rrect(m, m, size - m, size - m, r)}" fill="url(#t)"/>' + facets


def delta_mono_px(size, ink="#000000"):
    g = DELTA_MONO_PX[size]
    # One ink: the facets' cuts only where there is room for them (32 px).
    fdefs, body, outline = delta_facets(g, "d", (ink, ink, ink), gap=1.4, r=g["r"])
    if size < 32:
        return "", f'<path d="{outline}" fill="{ink}"/>'
    return fdefs, body


def delta_full():
    g = DELTA_FULL
    tile = squircle(500, 500, 460)
    fdefs, facets, outline = delta_facets(g, "d", ("url(#fl)", "url(#fm)", "url(#fr)"), gap=22, r=g["r"])
    defs = "\n".join([
        lin("t", 60, 40, 940, 960, DELTA_TILE),
        lin("gloss", 0, 40, 0, 560, [(0, "#FFFFFF", 0.2), (1, "#FFFFFF", 0)]),
        rad("glow", 500, 260, 640, [(0, "#C2A8FF", 0.4), (1, "#C2A8FF", 0)]),
        lin("fl", 150, 255, 500, 800, [(0, "#F4EFFF"), (1, "#D9CBFF")]),
        lin("fm", 0, 255, 0, 800, [(0, "#FFFFFF"), (1, "#F0EAFF")]),
        lin("fr", 850, 255, 500, 800, [(0, "#D4C3FF"), (1, "#A88CFF")]),
        blur("soft", 18),
        f'<clipPath id="clip-tile"><path d="{tile}"/></clipPath>',
        fdefs,
    ])
    body = "\n".join([
        f'<path d="{tile}" fill="#12053F" opacity="0.38" filter="url(#soft)" transform="translate(0 14)"/>',
        f'<path d="{tile}" fill="url(#t)"/>',
        '<g clip-path="url(#clip-tile)">',
        '<rect x="0" y="0" width="1000" height="1000" fill="url(#glow)"/>',
        '<ellipse cx="500" cy="80" rx="760" ry="470" fill="url(#gloss)"/>',
        f'<g filter="url(#soft)" opacity="0.5" transform="translate(0 24)"><path d="{outline}" fill="#1E0670"/></g>',
        facets,
        "</g>",
        f'<path d="{tile}" fill="none" stroke="#FFFFFF" stroke-opacity="0.18" stroke-width="6"/>',
    ])
    return defs, body


def delta_mono_full(ink="#000000"):
    g = DELTA_MONO_FULL
    fdefs, body, _ = delta_facets(g, "d", (ink, ink, ink), gap=30, r=g["r"])
    return fdefs, body


# ---- Candidate D: Comet ------------------------------------------------------------------------
# A file arriving from far away, fast: a white-hot head falling to the lower right, its tail
# three streaks (the parallel connections) converging into it. Ember on a night-sky tile.

COMET = {
    "name": "comet",
    "title": "D · Comet",
    "idea": ("A file arriving from far away, fast: a white-hot head falling towards the lower "
             "right, its tail three streaks (the parallel connections) converging into it. Ember "
             "colours on a night-sky tile; no letter, an object you remember."),
    "palette": [("Night", "#1E2A66"), ("Night deep", "#070A1E"), ("Head", "#FFF7E6"),
                ("Amber", "#FFB23E"), ("Ember", "#FF6A45"), ("Plum tail", "#B43A8C")],
}
# c: the head's centre, r: its radius, tip: the tail's far end; side: the side streaks (width,
# length of the upper one and of the lower one, as shares of the tail) or None; gap: the mono
# mark's ring of nothing round the head.
COMET_PX = {
    16: dict(tile=(0, 3.5), c=(10.5, 10.5), r=3.3, tip=(2.0, 2.0), side=None, gap=1.0),
    24: dict(tile=(0, 5), c=(15.5, 15.5), r=4.8, tip=(3.0, 3.0), side=None, gap=1.2),
    32: dict(tile=(1, 7), c=(20.5, 20.5), r=6.2, tip=(4.5, 4.5), side=(2.2, 0.6, 0.46), gap=1.6),
}
COMET_FULL = dict(c=(630, 630), r=150, tip=(150, 150), side=(46, 0.64, 0.5), gap=34)


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


def comet_shapes(g):
    c, r, tip = g["c"], g["r"], g["tip"]
    tail = tail_poly(c, r, tip)
    sides = []
    if g["side"]:
        w, la, lb = g["side"]
        sides = [streak(c, r, tip, 1.45, la, w), streak(c, r, tip, -1.45, lb, w)]
    return tail, sides


def comet_px(size):
    g = COMET_PX[size]
    m, rr = g["tile"]
    c, r, tip = g["c"], g["r"], g["tip"]
    tail, sides = comet_shapes(g)
    defs = "\n".join([
        lin("t", 0, 0, size, size, [(0, "#1E2A66"), (1, "#070A1E")]),
        lin("tail", tip[0], tip[1], c[0], c[1], [(0, "#B43A8C", 0.0), (0.45, "#FF6A45", 0.9), (1, "#FFB23E")]),
        rad("head", c[0] - r * 0.25, c[1] - r * 0.25, r * 1.25, [(0, "#FFFFFF"), (0.55, "#FFF1CF"), (1, "#FFC35A")]),
    ])
    body = (f'<path d="{rrect(m, m, size - m, size - m, rr)}" fill="url(#t)"/>'
            + "".join(f'<path d="{poly(s)}" fill="url(#tail)"/>' for s in sides)
            + f'<path d="{poly(tail)}" fill="url(#tail)"/>'
            + f'<circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(r)}" fill="url(#head)"/>')
    return defs, body


def comet_mono(g, ink, view):
    """One ink: the head, and the tail held off it by a ring of nothing, so the two read apart."""
    c, r = g["c"], g["r"]
    tail, sides = comet_shapes(g)
    defs = (f'<mask id="gap" maskUnits="userSpaceOnUse" x="0" y="0" width="{f(view)}" height="{f(view)}">'
            f'<rect width="{f(view)}" height="{f(view)}" fill="#fff"/>'
            f'<circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(r + g["gap"])}" fill="#000"/></mask>')
    body = (f'<g mask="url(#gap)">' + "".join(f'<path d="{poly(s)}" fill="{ink}"/>' for s in sides)
            + f'<path d="{poly(tail)}" fill="{ink}"/></g>'
            + f'<circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(r)}" fill="{ink}"/>')
    return defs, body


def comet_mono_px(size, ink="#000000"):
    return comet_mono(COMET_PX[size], ink, size)


def comet_full():
    g = COMET_FULL
    c, r, tip = g["c"], g["r"], g["tip"]
    tail, sides = comet_shapes(g)
    tile = squircle(500, 500, 460)
    stars = [(250, 690, 9, 0.55), (770, 250, 7, 0.45), (380, 830, 5, 0.35), (860, 430, 5, 0.3), (560, 140, 6, 0.4)]
    defs = "\n".join([
        lin("t", 80, 40, 920, 960, [(0, "#1E2A66"), (0.55, "#111842"), (1, "#070A1E")]),
        lin("tail", tip[0], tip[1], c[0], c[1], [(0, "#B43A8C", 0.0), (0.35, "#E2466E", 0.55), (0.7, "#FF6A45", 0.92), (1, "#FFB23E")]),
        lin("side", tip[0], tip[1], c[0], c[1], [(0, "#B43A8C", 0.0), (0.5, "#FF6A45", 0.55), (1, "#FFB23E", 0.95)]),
        rad("head", c[0] - 45, c[1] - 45, r * 1.2, [(0, "#FFFFFF"), (0.5, "#FFF3D6"), (0.85, "#FFCD6E"), (1, "#FFB23E")]),
        rad("halo", c[0], c[1], r * 2.6, [(0, "#FFB23E", 0.45), (0.4, "#FF7A45", 0.18), (1, "#FF6A45", 0)]),
        lin("gloss", 0, 40, 0, 560, [(0, "#FFFFFF", 0.12), (1, "#FFFFFF", 0)]),
        blur("soft", 18),
        f'<clipPath id="clip-tile"><path d="{tile}"/></clipPath>',
    ])
    body = "\n".join([
        f'<path d="{tile}" fill="#02040F" opacity="0.4" filter="url(#soft)" transform="translate(0 14)"/>',
        f'<path d="{tile}" fill="url(#t)"/>',
        '<g clip-path="url(#clip-tile)">',
        "".join(f'<circle cx="{x}" cy="{y}" r="{rr}" fill="#FFFFFF" opacity="{o}"/>' for x, y, rr, o in stars),
        '<ellipse cx="500" cy="60" rx="760" ry="440" fill="url(#gloss)"/>',
        f'<circle cx="{c[0]}" cy="{c[1]}" r="{r * 2.6}" fill="url(#halo)"/>',
        "".join(f'<path d="{poly(s)}" fill="url(#side)"/>' for s in sides),
        f'<path d="{poly(tail)}" fill="url(#tail)"/>',
        f'<circle cx="{c[0]}" cy="{c[1]}" r="{r}" fill="url(#head)"/>',
        "</g>",
        f'<path d="{tile}" fill="none" stroke="#8FA2FF" stroke-opacity="0.28" stroke-width="6"/>',
    ])
    return defs, body


def comet_mono_full(ink="#000000"):
    defs, body = comet_mono(COMET_FULL, ink, 1000)
    return defs, f'<g transform="translate(500 500) scale(1.12) translate(-470 -470)">{body}</g>'


CANDIDATES = [
    (SLIP, slip_full, slip_px, slip_mono_full, slip_mono_px),
    (LOOP, loop_full, loop_px, loop_mono_full, loop_mono_px),
    (DELTA, delta_full, delta_px, delta_mono_full, delta_mono_px),
    (COMET, comet_full, comet_px, comet_mono_full, comet_mono_px),
]


# ---- rendering ---------------------------------------------------------------------------------

def render(svg_path: Path, size: int, out: Path) -> Image.Image:
    subprocess.run([resvg_exe(), "-w", str(size), "-h", str(size), str(svg_path), str(out)], check=True)
    return Image.open(out).convert("RGBA")


def write(path: Path, text: str) -> Path:
    path.write_text(text, "utf-8", newline="\n")
    return path


def build(meta, full, px, mono_full, mono_px):
    name = meta["name"]
    src = HERE / name
    src.mkdir(parents=True, exist_ok=True)
    work = WORK / name
    work.mkdir(parents=True, exist_ok=True)
    defs, body = full()
    full_svg = write(src / f"{name}.svg", svg_doc(1000, body, defs))
    defs, body = mono_full()
    mono_svg = write(src / f"{name}-mono.svg", svg_doc(1000, body, defs))
    px_svg, mono_px_svg, off_px_svg = {}, {}, {}
    for s in PX:
        d, b = px(s)
        px_svg[s] = write(src / f"{name}-{s}.svg", svg_doc(s, b, d))
        off_px_svg[s] = write(src / f"{name}-off-{s}.svg", off_svg(s, b, d))
        d, b = mono_px(s)
        mono_px_svg[s] = write(src / f"{name}-mono-{s}.svg", svg_doc(s, b, d))
    icons = {s: render(px_svg[s] if s in px_svg else full_svg, s, work / f"{name}-{s}.png") for s in SIZES}
    off = {s: render(off_px_svg[s], s, work / f"{name}-off-{s}.png") for s in PX}
    mono = {s: render(mono_px_svg[s], s, work / f"{name}-mono-{s}.png") for s in PX}
    mono[256] = render(mono_svg, 256, work / f"{name}-mono-256.png")
    return dict(meta=meta, icons=icons, off=off, mono=mono)


# ---- sheets ------------------------------------------------------------------------------------

def font(size, bold=False):
    try:
        return ImageFont.truetype(str(FONTS / ("segoeuib.ttf" if bold else "segoeui.ttf")), size)
    except OSError:
        return ImageFont.load_default(size)


LIGHT = (243, 244, 246)
DARK = (30, 31, 34)
INK = (28, 30, 36)
INK2 = (96, 102, 114)
INK_DARK = (214, 218, 226)


def hexrgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def tinted(im, rgb):
    out = Image.new("RGBA", im.size, rgb + (0,))
    out.putalpha(im.getchannel("A"))
    return out


def magnify(im, bg, k):
    tile = Image.new("RGB", im.size, bg)
    tile.paste(im, (0, 0), im)
    return tile.resize((im.width * k, im.height * k), Image.NEAREST)


def placeholder(draw, x, y, s, shade, kind):
    """A neutral stand-in icon (never another program's): a rounded square, a disc or a ring."""
    if kind == "sq":
        draw.rounded_rectangle((x, y, x + s - 1, y + s - 1), radius=max(2, s // 5), fill=shade)
    elif kind == "ci":
        draw.ellipse((x, y, x + s - 1, y + s - 1), fill=shade)
    else:
        draw.ellipse((x, y, x + s - 1, y + s - 1), outline=shade, width=max(2, s // 8))


def taskbar(icons, mono, dark):
    """A Windows 11 taskbar strip at 100% scale: centred app buttons (24 px), DDM's running (its
    pill under it), and the notification area with DDM's tray icon, classic and 3D (16 px)."""
    W, H = 780, 48
    bg = (32, 32, 32) if dark else (238, 240, 243)
    ph = [(86, 86, 92), (70, 74, 82), (104, 104, 110)] if dark else [(170, 174, 182), (150, 156, 166), (190, 192, 198)]
    im = Image.new("RGB", (W, H), bg)
    d = ImageDraw.Draw(im)
    d.line((0, 0, W, 0), fill=(52, 52, 52) if dark else (220, 222, 226))
    x = 200
    for i, kind in enumerate(["sq", "ci", "sq", "app", "ri", "sq"]):
        if kind == "app":
            d.rounded_rectangle((x - 8, 4, x + 31, 43), radius=5, fill=(48, 48, 50) if dark else (252, 252, 253))
            im.paste(icons[24], (x, 12), icons[24])
            d.rounded_rectangle((x + 8, 40, x + 15, 42), radius=1, fill=(96, 205, 255) if dark else (0, 95, 184))
        else:
            placeholder(d, x, 12, 24, ph[i % 3], kind)
        x += 44
    # Notification area: an up chevron, the two tray looks, a clock.
    tx = W - 190
    ink = (220, 220, 220) if dark else (40, 40, 40)
    d.line((tx, 26, tx + 4, 22, tx + 8, 26), fill=ink, width=1)
    tx += 26
    m = tinted(mono[16], (255, 255, 255) if dark else (27, 27, 31))
    im.paste(m, (tx, 16), m)
    tx += 28
    im.paste(icons[16], (tx, 16), icons[16])
    tx += 34
    d.text((tx, 9), "12:00", font=font(12), fill=ink)
    d.text((tx, 25), "22/11/2026", font=font(12), fill=ink)
    return im


def chrome_toolbar(icons, off, dark):
    """A browser toolbar (Chromium-like, neutral): back, forward, reload, the address field, then
    extension buttons: placeholders, DDM's on and off looks, a profile disc."""
    W, H = 780, 40
    bg = (53, 54, 58) if dark else (255, 255, 255)
    field = (32, 33, 36) if dark else (237, 242, 250)
    ink = (199, 199, 199) if dark else (71, 71, 71)
    dim = (110, 110, 110) if dark else (190, 190, 190)
    ph = (120, 124, 130) if dark else (160, 164, 172)
    im = Image.new("RGB", (W, H), bg)
    d = ImageDraw.Draw(im)
    d.line((14, 20, 22, 12), fill=ink, width=2)
    d.line((14, 20, 22, 28), fill=ink, width=2)
    d.line((48, 12, 56, 20), fill=dim, width=2)
    d.line((48, 28, 56, 20), fill=dim, width=2)
    d.arc((76, 12, 92, 28), 40, 340, fill=ink, width=2)
    d.rounded_rectangle((108, 6, 520, 34), radius=14, fill=field)
    d.rounded_rectangle((128, 17, 300, 23), radius=3, fill=ph)
    x = 552
    for kind, which in (("ph", "ci"), ("ddm", "on"), ("ph", "sq"), ("ddm", "off")):
        if kind == "ph":
            placeholder(d, x, 12, 16, ph, which)
        else:
            src = (icons if which == "on" else off)[16]
            im.paste(src, (x, 12), src)
        x += 36
    d.ellipse((x + 6, 10, x + 26, 30), fill=ph)
    return im


def installer_header(icons, dark):
    """Inno Setup's modern wizard, its top: the title bar with the setup's icon, the header with
    the page's title and subtitle and the small wizard image at its right, the rule under it."""
    W = 500
    title_h, head_h = 31, 58
    H = title_h + head_h + 46
    win = (32, 32, 32) if dark else (243, 243, 243)
    head = (43, 43, 43) if dark else (255, 255, 255)
    body = (32, 32, 32) if dark else (240, 240, 240)
    ink = (240, 240, 240) if dark else (0, 0, 0)
    ink2 = (200, 200, 200) if dark else (60, 60, 60)
    im = Image.new("RGB", (W, H), body)
    d = ImageDraw.Draw(im)
    d.rectangle((0, 0, W, title_h), fill=win)
    im.paste(icons[16], (10, 8), icons[16])
    d.text((34, 7), "Setup - Dannex Download Manager", font=font(12), fill=ink)
    # minimise and close, drawn
    d.line((W - 92, 16, W - 82, 16), fill=ink2, width=1)
    d.line((W - 30, 11, W - 20, 21), fill=ink2, width=1)
    d.line((W - 30, 21, W - 20, 11), fill=ink2, width=1)
    d.rectangle((0, title_h, W, title_h + head_h), fill=head)
    d.text((24, title_h + 10), "Select Destination Location", font=font(12, True), fill=ink)
    d.text((38, title_h + 29), "Where should Dannex Download Manager be installed?", font=font(12), fill=ink2)
    im.paste(icons[48], (W - 18 - 48, title_h + 5), icons[48])
    d.line((0, title_h + head_h, W, title_h + head_h), fill=(70, 70, 70) if dark else (160, 160, 160))
    d.text((38, title_h + head_h + 14), "Setup will install Dannex Download Manager into the following folder.",
           font=font(12), fill=ink2)
    return im


def wrap(d, text, fnt, width):
    words, line, lines = text.split(), "", []
    for w in words:
        t = (line + " " + w).strip()
        if d.textlength(t, font=fnt) > width:
            lines.append(line)
            line = w
        else:
            line = t
    lines.append(line)
    return lines


def contact_sheet(c):
    meta, icons, off, mono = c["meta"], c["icons"], c["off"], c["mono"]
    W, pad = 1640, 32
    sheet = Image.new("RGB", (W, 2700), (255, 255, 255))
    d = ImageDraw.Draw(sheet)
    y = pad
    d.text((pad, y), meta["title"], font=font(34, True), fill=INK)
    y += 50
    for ln in wrap(d, meta["idea"], font(17), W - 2 * pad - 580):
        d.text((pad, y), ln, font=font(17), fill=INK2)
        y += 24
    px0 = W - pad - 540
    for i, (label, hx) in enumerate(meta["palette"]):
        sx, sy = px0 + (i % 3) * 180, pad + 6 + (i // 3) * 62
        d.rounded_rectangle((sx, sy, sx + 44, sy + 44), radius=8, fill=hexrgb(hx), outline=(210, 212, 218))
        d.text((sx + 54, sy + 2), label, font=font(14), fill=INK2)
        d.text((sx + 54, sy + 20), hx.upper(), font=font(16, True), fill=INK)
    y = max(y, pad + 140) + 18

    # Every size at 1:1 on light and dark; then the off and mono looks.
    row_h = 256 + 52
    for bg, ink, label in ((LIGHT, INK2, "light"), (DARK, INK_DARK, "dark")):
        d.rectangle((0, y, W, y + row_h), fill=bg)
        d.text((pad, y + 10), f"1:1 on {label}", font=font(14), fill=ink)
        x = pad
        for s in reversed(SIZES):
            sheet.paste(icons[s], (x, y + 34 + (256 - s)), icons[s])
            d.text((x, y + 34 + 256 + 2), f"{s}", font=font(13), fill=ink)
            x += s + 34
        x += 30
        d.text((x, y + 10), "off (extension)", font=font(14), fill=ink)
        for s in (16, 24, 32):
            sheet.paste(off[s], (x, y + 34 + 256 - s), off[s])
            x += s + 18
        x += 30
        mink = (255, 255, 255) if label == "dark" else (27, 27, 31)
        d.text((x, y + 10), "mono (tray classic)", font=font(14), fill=ink)
        for s in (16, 24, 32):
            m = tinted(mono[s], mink)
            sheet.paste(m, (x, y + 34 + 256 - s), m)
            x += s + 18
        m = tinted(mono[256].resize((96, 96), Image.LANCZOS), mink)
        sheet.paste(m, (x - 2, y + 34 + 256 - 96 - 60), m)
        y += row_h

    # Magnified pixels.
    zoom_h = 32 * 6 + 54
    for bg, ink, label in ((LIGHT, INK2, "light"), (DARK, INK_DARK, "dark")):
        d.rectangle((0, y, W, y + zoom_h), fill=bg)
        d.text((pad, y + 10), f"pixels, magnified, on {label}", font=font(14), fill=ink)
        x = pad
        mink = (255, 255, 255) if label == "dark" else (27, 27, 31)
        for im, k, cap in ((icons[16], 12, "16"), (icons[24], 8, "24"), (icons[32], 6, "32"),
                           (off[16], 6, "off 16"), (off[32], 3, "off 32"),
                           (tinted(mono[16], mink), 6, "mono 16"), (tinted(mono[24], mink), 4, "mono 24")):
            z = magnify(im, bg, k)
            sheet.paste(z, (x, y + 36))
            d.text((x, y + 36 + z.height + 2), cap, font=font(13), fill=ink)
            x += z.width + 26
        y += zoom_h

    # In context: 1:1, and the same magnified twice (nearest neighbour) under it.
    y += 16
    d.text((pad, y), "In context: 100% scale, and the same magnified 2x (each pixel shown)", font=font(18, True), fill=INK)
    y += 34
    for dark, cap in ((True, "Windows taskbar, dark: app button 24 px; notification area: tray classic, then 3D, 16 px"),
                      (False, "Windows taskbar, light")):
        tb = taskbar(icons, mono, dark)
        d.text((pad, y), cap, font=font(14), fill=INK2)
        y += 22
        sheet.paste(tb, (pad, y))
        z = tb.crop((180, 0, 780, 48)).resize((1200, 96), Image.NEAREST)
        y += 48 + 6
        sheet.paste(z, (pad, y))
        y += 96 + 20
    for dark, cap in ((False, "Browser toolbar, light: placeholder, DDM on, placeholder, DDM off"),
                      (True, "Browser toolbar, dark")):
        bar = chrome_toolbar(icons, off, dark)
        d.text((pad, y), cap, font=font(14), fill=INK2)
        y += 22
        sheet.paste(bar, (pad, y))
        z = bar.crop((530, 0, 780, 40)).resize((500, 80), Image.NEAREST)
        sheet.paste(z, (pad + 800, y - 20))
        y += 40 + 34
    d.text((pad, y), "Installer wizard (Inno Setup, modern): title bar icon 16 px, header image 48 px; light and dark",
           font=font(14), fill=INK2)
    y += 22
    a, b = installer_header(icons, False), installer_header(icons, True)
    sheet.paste(a, (pad, y))
    sheet.paste(b, (pad + a.width + 30, y))
    y += a.height + pad
    return sheet.crop((0, 0, W, y))


def overview(built):
    """All four side by side at 128, 32 and 16 px on light and dark, the 32 and 16 magnified
    under them."""
    col_w, pad = 420, 28
    W, H = pad + col_w * len(built), 660
    im = Image.new("RGB", (W, H), (255, 255, 255))
    d = ImageDraw.Draw(im)
    half = (H - 60) // 2
    grounds = ((LIGHT, INK), (DARK, INK_DARK))
    for i, (bg, _) in enumerate(grounds):
        d.rectangle((0, 60 + i * half, W, 60 + (i + 1) * half), fill=bg)
    for j, c in enumerate(built):
        x0 = pad + j * col_w
        d.text((x0, 16), c["meta"]["title"], font=font(22, True), fill=INK)
        for i, (bg, _) in enumerate(grounds):
            y0 = 60 + i * half
            x = x0
            for s in (128, 32, 16):
                ic = c["icons"][s]
                im.paste(ic, (x, y0 + 30 + 128 - s), ic)
                x += s + 26
            im.paste(magnify(c["icons"][32], bg, 3), (x0, y0 + 30 + 128 + 22))
            im.paste(magnify(c["icons"][16], bg, 5), (x0 + 96 + 24, y0 + 30 + 128 + 22 + 16))
    return im


def main():
    built = [build(*cand) for cand in CANDIDATES]
    for c in built:
        name = c["meta"]["name"]
        contact_sheet(c).save(HERE / name / f"{name}-sheet.png", optimize=True)
    overview(built).save(HERE / "overview.png", optimize=True)
    print("wrote", ", ".join(c["meta"]["name"] for c in built), "and overview.png")


if __name__ == "__main__":
    main()
