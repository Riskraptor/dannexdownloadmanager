"""The README's four block icons (faster, languages, free, torrent), in the app icon's style
(tools/icon/comet_logo.py): ember and amber light on night blue, soft gradients, one object each
on nothing, so they sit on GitHub's light and dark pages alike. Drawn from geometry as SVG and
rendered by resvg; our own pictures, nothing taken from any other program.

    python tools/readme/icons.py

Writes docs/images/icons/<name>.png (256 px, the README) and website/assets/icons/<name>.webp
(192 px), and keeps each SVG in tools/readme/icons/.
"""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "icon"))
import comet_logo as cl  # noqa: E402
from comet_logo import AMBER, EMBER, NIGHT, NIGHT_DEEP, PLUM, f, lin, poly, rad, rrect  # noqa: E402,I001

SRC = Path(__file__).resolve().parent / "icons"
# A lighter rim on the night parts, so they hold on a dark page too.
RIM = "#6F80D8"


def head(cx, cy, r, ids):
    """The comet's white-hot head with its halo."""
    defs = "\n".join([
        rad(f"h{ids}", cx - r * 0.3, cy - r * 0.3, r * 1.2,
            [(0, "#FFFFFF"), (0.5, "#FFF3D6"), (0.85, "#FFCD6E"), (1, AMBER)]),
        rad(f"halo{ids}", cx, cy, r * 1.9, [(0, AMBER, 0.42), (0.45, "#FF7A45", 0.15), (1, EMBER, 0)]),
    ])
    body = (f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r * 1.9)}" fill="url(#halo{ids})"/>'
            f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r)}" fill="url(#h{ids})"/>')
    return defs, body


def tail(c, r, tip, ids, side=None):
    """The comet's tail, tip to head, and its side streaks."""
    g = dict(c=c, r=r, tip=tip, side=side)
    t, sides = cl.shapes(g)
    defs = "\n".join([
        lin(f"t{ids}", tip[0], tip[1], c[0], c[1], [(0, PLUM, 0.0), (0.4, "#E2466E", 0.6), (0.75, EMBER, 0.95), (1, AMBER)]),
        lin(f"s{ids}", tip[0], tip[1], c[0], c[1], [(0, PLUM, 0.0), (0.5, EMBER, 0.6), (1, AMBER, 0.95)]),
    ])
    body = "".join(f'<path d="{poly(s)}" fill="url(#s{ids})"/>' for s in sides) + f'<path d="{poly(t)}" fill="url(#t{ids})"/>'
    return defs, body


def faster():
    """The comet streaking to the right, flat out."""
    td, tb = tail((172, 132), 40, (10, 104), "a", side=(13, 0.62, 0.5))
    hd, hb = head(172, 132, 40, "a")
    return td + hd, tb + hb


def bubble(x0, y0, x1, y1, r, tail_side):
    """A speech bubble: a rounded box and its pointer at the foot, left or right."""
    box = rrect(x0, y0, x1, y1, r)
    if tail_side == "left":
        pointer = poly([(x0 + 26, y1 - 2), (x0 + 62, y1 - 2), (x0 + 18, y1 + 30)])
    else:
        pointer = poly([(x1 - 62, y1 - 2), (x1 - 26, y1 - 2), (x1 - 18, y1 + 30)])
    return box + " " + pointer


def languages():
    """Two speech bubbles, one night blue with a letter, one ember with three dots."""
    back = bubble(18, 30, 160, 136, 30, "left")
    front = bubble(96, 104, 238, 210, 30, "right")
    # A geometric A, drawn (no font).
    a = poly([(66, 116), (82, 50), (98, 50), (114, 116), (99, 116), (95, 100), (85, 100), (81, 116)])
    a_hole = poly([(88, 70), (92, 70), (93, 86), (87, 86)])
    defs = "\n".join([
        lin("bk", 18, 30, 160, 166, [(0, "#3A4CA8"), (1, "#1B2560")]),
        lin("fr", 96, 104, 238, 240, [(0, AMBER), (0.55, EMBER), (1, "#E2466E")]),
        lin("gl", 0, 104, 0, 160, [(0, "#FFFFFF", 0.35), (1, "#FFFFFF", 0)]),
    ])
    body = "\n".join([
        f'<path d="{back}" fill="url(#bk)"/>',
        f'<path d="{a} {a_hole}" fill-rule="evenodd" fill="#FFF7E6"/>',
        f'<path d="{front}" fill="url(#fr)"/>',
        f'<path d="{rrect(104, 108, 230, 150, 24)}" fill="url(#gl)"/>',
        "".join(f'<circle cx="{x}" cy="157" r="11" fill="{NIGHT}"/>' for x in (134, 167, 200)),
    ])
    return defs, body


def free():
    """A gift: a night-blue box tied with an ember ribbon, a spark beside it."""
    defs = "\n".join([
        lin("box", 0, 116, 0, 236, [(0, "#2C3C8E"), (1, NIGHT_DEEP)]),
        lin("lid", 0, 92, 0, 126, [(0, "#3A4CA8"), (1, "#24327A")]),
        lin("rib", 0, 60, 0, 236, [(0, AMBER), (0.6, EMBER), (1, "#E2466E")]),
        lin("bow", 0, 40, 0, 100, [(0, AMBER), (1, EMBER)]),
    ])
    sd, sb = head(214, 52, 9, "f")
    loop_l = "M128,96 C96,92 70,74 80,54 C90,36 116,52 128,96 Z"
    loop_r = "M128,96 C160,92 186,74 176,54 C166,36 140,52 128,96 Z"
    body = "\n".join([
        f'<path d="{rrect(44, 118, 212, 236, 12)}" fill="url(#box)" stroke="{RIM}" stroke-opacity="0.55" stroke-width="3"/>',
        f'<path d="{rrect(34, 90, 222, 126, 10)}" fill="url(#lid)" stroke="{RIM}" stroke-opacity="0.6" stroke-width="3"/>',
        '<rect x="112" y="90" width="32" height="146" fill="url(#rib)"/>',
        f'<path d="{loop_l}" fill="url(#bow)"/><path d="{loop_r}" fill="url(#bow)"/>',
        '<circle cx="128" cy="94" r="12" fill="#FFCD6E"/>',
        '<rect x="44" y="126" width="168" height="6" fill="#050817" opacity="0.35"/>',
        sb,
    ])
    return defs + "\n" + sd, body


def torrent():
    """A horseshoe magnet, its poles up, three streaks coming in from far away into it: a file
    gathered from many sources at once."""
    # The magnet: a thick U (outer and inner arcs), poles at the top.
    cx, cy, ro, ri = 128, 150, 86, 40
    u = (f"M{cx - ro},{cy} A{ro},{ro} 0 0 0 {cx + ro},{cy} L{cx + ro},{cy - 64} L{cx + ri},{cy - 64} "
         f"L{cx + ri},{cy} A{ri},{ri} 0 0 1 {cx - ri},{cy} L{cx - ri},{cy - 64} L{cx - ro},{cy - 64} Z")
    defs = "\n".join([
        lin("mag", cx - ro, 0, cx + ro, 0, [(0, AMBER), (0.5, EMBER), (1, "#C9356F")]),
        lin("pole", 0, cy - 64, 0, cy - 30, [(0, "#FFFFFF"), (1, "#C3CDE0")]),
        lin("in", 0, 0, 0, 1, [(0, "#FFFFFF", 0.3), (1, "#FFFFFF", 0)]),
    ])
    # Three streaks from far apart, converging on the gap between the poles.
    streaks = (((128, 70), 8, (128, 2)), ((102, 72), 6, (44, 14)), ((154, 72), 6, (212, 14)))
    sdefs = "\n".join(lin(f"st{i}", tip[0], tip[1], c[0], c[1], [(0, PLUM, 0), (0.6, EMBER, 0.85), (1, AMBER)])
                      for i, (c, r, tip) in enumerate(streaks))
    sbody = "".join(f'<path d="{poly(cl.tail_poly(c, r, tip))}" fill="url(#st{i})"/>'
                    for i, (c, r, tip) in enumerate(streaks))
    body = "\n".join([
        sbody,
        f'<path d="{u}" fill="url(#mag)"/>',
        f'<path d="{u}" fill="url(#in)"/>',
        f'<rect x="{cx - ro}" y="{cy - 64}" width="{ro - ri}" height="30" fill="url(#pole)"/>',
        f'<rect x="{cx + ri}" y="{cy - 64}" width="{ro - ri}" height="30" fill="url(#pole)"/>',
    ])
    return defs + "\n" + sdefs, body


ICONS = {"faster": faster, "languages": languages, "free": free, "torrent": torrent}


def main():
    SRC.mkdir(exist_ok=True)
    for name, draw in ICONS.items():
        defs, body = draw()
        svg = cl.svg_doc(256, body, defs)
        im = cl.rasterise(svg, 256, keep=SRC / f"{name}.svg")
        im.save(ROOT / "docs" / "images" / "icons" / f"{name}.png", optimize=True)
        cl.rasterise(svg, 192).save(ROOT / "website" / "assets" / "icons" / f"{name}.webp", lossless=True, method=6)
        print(f"wrote {name}")


if __name__ == "__main__":
    main()
