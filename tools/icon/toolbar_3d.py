"""DDM's "3D style" toolbar icons (View → Toolbar → 3D style), drawn from geometry with Pillow
so anyone can rebuild them exactly. Original to Dannex: glossy, shaded, colourful objects, every
one lit the same way (from above and a little to the left), in one palette:

- Add URL: a green plus; Add Torrent: a purple horseshoe magnet with steel poles;
- Resume: a blue button with a white arrow down; Stop: a red button with a white square;
  Stop All: two red buttons, one behind the other;
- Delete: a steel bin; Delete Completed: the bin with a green check badge;
- Options: a steel-blue gear; Scheduler: an amber alarm clock;
- Start Queue / Stop Queue: a stack of blue cards with a green play / red stop badge;
- Grabber: a sea-blue globe, its meridians and parallels white, with a green arrow-down badge.

Each object has a body shaded from light at its top to dark at its foot, a darker rim (so it
stands out of a light toolbar; its bright body stands out of a dark one), a gloss over its upper
part, a shade along its lower edge and a soft shadow under it. Symbols on a button are white,
sunk a little into it. Badges stand off what they sit on by a gap of nothing.

Sizes: 16 and 32 (the toolbar's small and large buttons), 20, 24, 40 and 48 (125 % and 150 %
screens) and 64 (200 %). Below 24 px the objects keep fewer details (no bin ribs, no clock ticks)
and their straight edges are put on whole pixels.

    python tools/icon/toolbar_3d.py            # crates/app/assets/icons/toolbar3d/<size>/tb-*.png
    python tools/icon/toolbar_3d.py --preview  # and art/toolbar-3d-preview.png
    python tools/icon/toolbar_3d.py --only tb-grabber  # one icon's files, every size (a new one)

The app draws a greyed copy of each for a disabled button (icons.rs, `greyed`), the same as the
preview's greyed row. Needs Pillow only; plain python, not python -I, when Pillow is in the
user site.
"""

import argparse
import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "crates" / "app" / "assets" / "icons" / "toolbar3d"
PREVIEW = ROOT / "art" / "toolbar-3d-preview.png"
SIZES = [16, 20, 24, 32, 40, 48, 64]
GRID = 48.0  # every shape is placed on a 48-unit square


def hexc(h: str) -> tuple:
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


# Materials: the body's top and foot, its rim, and its shade's ink.
MATERIALS = {
    "green": dict(top="#9df07c", foot="#119443", rim="#0a5a2a", shade="#05401c"),
    "blue": dict(top="#7fd0ff", foot="#1652d4", rim="#0c2f86", shade="#071d5c"),
    "red": dict(top="#ff9a86", foot="#d3141f", rim="#7a0710", shade="#55040a"),
    "purple": dict(top="#dfc0ff", foot="#7b3cec", rim="#43189a", shade="#2c0e6a"),
    "amber": dict(top="#ffe27a", foot="#ee8a07", rim="#8a4a02", shade="#5c3000"),
    "steel": dict(top="#ffffff", foot="#8c99b0", rim="#465065", shade="#2b3344"),
    "gearsteel": dict(top="#e6eeff", foot="#5d76a8", rim="#2a3a62", shade="#1a2645"),
    "paper": dict(top="#ffffff", foot="#d6deea", rim="#6f7b90", shade="#4a5568"),
    "card": dict(top="#c6e6ff", foot="#5b8fe0", rim="#23468f", shade="#142e66"),
    "globe": dict(top="#9cf0ff", foot="#0b84b8", rim="#064a70", shade="#03304c"),
    "glyph": dict(top="#ffffff", foot="#e1e9f5", rim=None, shade="#1a2030"),
    "ink": dict(top="#3a4256", foot="#1d2230", rim=None, shade="#000000"),
}
SHADOW = (8, 12, 30)


class Canvas:
    """One icon at `size` pixels, drawn at k times that (S sub-pixels a side), in 48 units."""

    def __init__(self, size):
        self.size = size
        self.k = max(8, math.ceil(640 / size))
        self.S = size * self.k
        self.u = self.S / GRID  # sub-pixels per unit
        self.small = size < 24
        self.parts = []  # (mask, material, options) in z order

    # -- units, and whole pixels ------------------------------------------------------------
    def px(self, v):
        """v units in sub-pixels."""
        return v * self.u

    def snap(self, v):
        """v units, on the nearest whole pixel of the icon (for straight edges at small sizes)."""
        if self.size > 32:
            return v
        p = v * self.size / GRID
        return round(p) * GRID / self.size

    def line_w(self):
        """The rim's width in sub-pixels: one pixel up to 32 px, a little more above."""
        return max(1.0, self.size / 30) * self.k

    # -- masks -----------------------------------------------------------------------------
    def new(self):
        return Image.new("L", (self.S, self.S), 0)

    def circle(self, cx, cy, r):
        m = self.new()
        p = self.px
        ImageDraw.Draw(m).ellipse((p(cx - r), p(cy - r), p(cx + r), p(cy + r)), fill=255)
        return m

    def rrect(self, x0, y0, x1, y1, r, snap=True):
        if snap:
            x0, y0, x1, y1 = (self.snap(v) for v in (x0, y0, x1, y1))
        m = self.new()
        p = self.px
        ImageDraw.Draw(m).rounded_rectangle((p(x0), p(y0), p(x1) - 1, p(y1) - 1), radius=p(r),
                                            fill=255)
        return m

    def poly(self, pts):
        m = self.new()
        ImageDraw.Draw(m).polygon([(self.px(x), self.px(y)) for x, y in pts], fill=255)
        return m

    def capsule(self, x0, y0, x1, y1, w):
        """A stroke from (x0, y0) to (x1, y1), w units wide, round ends."""
        m = self.new()
        d = ImageDraw.Draw(m)
        p = self.px
        d.line((p(x0), p(y0), p(x1), p(y1)), fill=255, width=max(1, round(p(w))))
        for x, y in ((x0, y0), (x1, y1)):
            d.ellipse((p(x - w / 2), p(y - w / 2), p(x + w / 2), p(y + w / 2)), fill=255)
        return m

    def add(self, mask, material, **opts):
        self.parts.append((mask, material, opts))
        return mask

    # -- painting --------------------------------------------------------------------------
    def render(self):
        S, k = self.S, self.k
        out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        # Knock-outs: a badge stands off what is under it by a gap of nothing.
        parts = []
        for i, (mask, mat, opts) in enumerate(self.parts):
            for later, _, lopts in self.parts[i + 1:]:
                gap = lopts.get("gap")
                if gap:
                    mask = ImageChops.subtract(mask, dilate(later, self.px(gap)))
            parts.append((mask, mat, opts))
        # The shadow under the whole object.
        union = self.new()
        for mask, _, opts in parts:
            union = ImageChops.lighter(union, mask)
        drop = max(1.0, self.px(1.6)) if not self.small else k * 0.7
        sh = blur(shift(union, 0, drop), max(k * 0.5, self.px(1.4)))
        sh = sh.point(lambda v: round(v * (0.30 if self.small else 0.38)))
        out.paste(Image.new("RGBA", (S, S), SHADOW + (255,)), (0, 0), sh)
        for mask, mat, opts in parts:
            layer = self.material(mask, MATERIALS[mat], opts)
            out = Image.alpha_composite(out, layer)
        return out.convert("RGBa").reduce(k).convert("RGBA")

    def material(self, mask, m, opts):
        S, k = self.S, self.k
        box = mask.getbbox() or (0, 0, S, S)
        x0, y0, x1, y1 = box
        top, foot = hexc(m["top"]), hexc(m["foot"])
        rgb = vgrad(S, top, foot, y0, y1, tilt=opts.get("tilt", 0.18), x0=x0, x1=x1)
        alpha = mask
        gloss = opts.get("gloss", 1.0)
        rw = self.line_w()
        inner = erode(mask, rw) if m["rim"] else mask
        # The shade along the lower and right edges, inside.
        e = max(k * 0.8, self.px(2.2))
        low = ImageChops.subtract(inner, shift(inner, -e * 0.45, -e))
        low = ImageChops.multiply(blur(low, e * 0.5), inner)
        rgb.paste(Image.new("RGB", (S, S), hexc(m["shade"])), (0, 0),
                  low.point(lambda v: round(v * opts.get("shade", 0.32))))
        if gloss:
            # The gloss: a wide ellipse over the upper part, bright at the top, gone at its foot.
            w, h = x1 - x0, y1 - y0
            gm = Image.new("L", (S, S), 0)
            gy1 = y0 + h * opts.get("gloss_h", 0.52)
            ImageDraw.Draw(gm).ellipse((x0 + w * 0.06 - w * 0.25, y0 - h * 0.7,
                                        x1 - w * 0.06 + w * 0.05, gy1), fill=255)
            gm = blur(gm, k * 0.6)
            gm = ImageChops.multiply(gm, erode(inner, rw * 0.6) if m["rim"] else inner)
            a0 = (0.80 if not self.small else 0.6) * gloss
            gm = ImageChops.multiply(gm, ramp(S, y0, gy1, round(255 * a0), round(255 * 0.06 * gloss)))
            rgb.paste(Image.new("RGB", (S, S), (255, 255, 255)), (0, 0), gm)
        if m["rim"]:
            rim = ImageChops.subtract(mask, inner)
            rgb.paste(Image.new("RGB", (S, S), hexc(m["rim"])), (0, 0), rim)
            # A light line just inside the rim at the top: the edge catching the light.
            if not self.small:
                lit = ImageChops.subtract(inner, shift(inner, 0, rw))
                lit = ImageChops.multiply(lit, ramp(S, y0, y0 + (y1 - y0) * 0.5, 150, 0))
                rgb.paste(Image.new("RGB", (S, S), (255, 255, 255)), (0, 0), lit)
        layer = rgb.convert("RGBA")
        layer.putalpha(alpha)
        if opts.get("sunk"):
            # A symbol sunk into its button: a dark edge under it inside the button, drawn first.
            under = shift(mask, 0, max(k * 0.6, self.px(1.0)))
            under = ImageChops.subtract(under, mask)
            under = ImageChops.multiply(blur(under, k * 0.3), opts["sunk"])
            base = Image.new("RGBA", (S, S), hexc("#000000") + (0,))
            base.putalpha(under.point(lambda v: round(v * 0.45)))
            layer = Image.alpha_composite(base, layer)
        return layer


# ---- mask helpers ----------------------------------------------------------------------------

def shift(m, dx, dy):
    out = Image.new(m.mode, m.size, 0)
    out.paste(m, (round(dx), round(dy)))
    return out


def blur(m, r):
    return m.filter(ImageFilter.GaussianBlur(r)) if r > 0.3 else m


def erode(m, r):
    """The mask shrunk by about r sub-pixels (blurred, then cut high)."""
    if r < 0.5:
        return m
    return m.filter(ImageFilter.GaussianBlur(r / 1.75)).point(lambda v: 255 if v >= 246 else 0)


def dilate(m, r):
    """The mask grown by about r sub-pixels (blurred, then cut low)."""
    if r < 0.5:
        return m
    return m.filter(ImageFilter.GaussianBlur(r / 1.75)).point(lambda v: 255 if v >= 10 else 0)


def ramp(S, y0, y1, a0, a1):
    col = Image.new("L", (1, S))
    col.putdata([round(a0 + (a1 - a0) * max(0.0, min(1.0, (y - y0) / max(1, y1 - y0))))
                 for y in range(S)])
    return col.resize((S, S), Image.NEAREST)


def vgrad(S, top, foot, y0, y1, tilt=0.0, x0=0, x1=None):
    """From `top` at row y0 to `foot` at row y1, tilted so its left is a little lighter."""
    x1 = S if x1 is None else x1
    v = ramp(S, y0, y1, 0, 255)
    if tilt:
        row = Image.new("L", (S, 1))
        row.putdata([round(255 * max(0.0, min(1.0, (x - x0) / max(1, x1 - x0)))) for x in range(S)])
        h = row.resize((S, S), Image.NEAREST)
        v = Image.blend(v, h, tilt)
    return Image.merge("RGB", [v.point([round(top[c] + (foot[c] - top[c]) * i / 255)
                                        for i in range(256)]) for c in range(3)])


# ---- the icons -------------------------------------------------------------------------------

def button(c, cx, cy, r, mat):
    return c.add(c.circle(cx, cy, r), mat, gloss_h=0.55)


def arrow_down(c, cx, top, bottom, shaft, head):
    """A down arrow: its shaft from `top`, its head `head` wide, its tip at `bottom`."""
    hy = bottom - head / 2
    return c.poly([(cx - shaft / 2, top), (cx + shaft / 2, top), (cx + shaft / 2, hy),
                   (cx + head / 2, hy), (cx, bottom), (cx - head / 2, hy), (cx - shaft / 2, hy)])


def add_url(c):
    t = 7.5  # the bars' half-thickness
    v = c.rrect(24 - t, 4, 24 + t, 44, 3.5)
    h = c.rrect(4, 24 - t, 44, 24 + t, 3.5)
    c.add(ImageChops.lighter(v, h), "green", gloss_h=0.5)


def add_torrent(c):
    # A horseshoe magnet, its poles down: an arch of purple, steel at the ends.
    cx, cy, ro, ri = 24, 21, 19, 8.5
    arch = c.circle(cx, cy, ro)
    arch = ImageChops.subtract(arch, c.circle(cx, cy, ri))
    below = c.rrect(0, cy, 48, 48, 0, snap=False)
    legs = ImageChops.lighter(c.rrect(cx - ro, cy, cx - ri, 37, 0), c.rrect(cx + ri, cy, cx + ro, 37, 0))
    body = ImageChops.lighter(ImageChops.subtract(arch, below), legs)
    c.add(body, "purple", gloss_h=0.45)
    poles = ImageChops.lighter(c.rrect(cx - ro, 37, cx - ri, 45, 1.2), c.rrect(cx + ri, 37, cx + ro, 45, 1.2))
    c.add(poles, "steel", gloss_h=0.6)


def resume(c):
    b = button(c, 24, 24, 20.5, "blue")
    c.add(arrow_down(c, 24, 11, 36, 8.5 if not c.small else 9, 23 if not c.small else 24), "glyph",
          gloss=0, sunk=b)


def stop(c):
    b = button(c, 24, 24, 20.5, "red")
    s = 7.5 if not c.small else 8
    c.add(c.rrect(24 - s, 24 - s, 24 + s, 24 + s, 2), "glyph", gloss=0, sunk=b)


def stop_all(c):
    c.add(c.circle(30.5, 17.5, 14.5), "red", gloss_h=0.5, shade=0.45)
    f = c.add(c.circle(19.5, 28.5, 16.5), "red", gloss_h=0.55, gap=1.6)
    s = 6 if not c.small else 6.5
    c.add(c.rrect(19.5 - s, 28.5 - s, 19.5 + s, 28.5 + s, 1.6), "glyph", gloss=0, sunk=f)


def bin_(c, x=0.0, y=0.0, s=1.0, ribs=True):
    """The bin, scaled by s about its top left and moved by (x, y)."""
    def X(v):
        return x + v * s

    def Y(v):
        return y + v * s
    handle = c.rrect(X(18), Y(3), X(30), Y(10), 2.5 * s)
    handle = ImageChops.subtract(handle, c.rrect(X(21), Y(6), X(27), Y(10), 1 * s))
    lid = c.rrect(X(7), Y(8), X(41), Y(14), 2.4 * s)
    body = c.poly([(X(10), Y(15)), (X(38), Y(15)), (X(35.5), Y(42)), (X(33), Y(45)),
                   (X(15), Y(45)), (X(12.5), Y(42))])
    b = c.add(body, "steel", gloss_h=0.4, tilt=0.35)
    if ribs and not c.small:
        rib = c.new()
        for rx in (18.5, 24, 29.5):
            rib = ImageChops.lighter(rib, c.capsule(X(rx), Y(21), X(rx + (rx - 24) * 0.04), Y(38),
                                                    2.6 * s))
        c.add(ImageChops.multiply(rib, erode(b, c.line_w() * 1.5)), "ink", gloss=0, shade=0)
    c.add(ImageChops.lighter(lid, handle), "steel", gloss_h=0.6)


def delete(c):
    bin_(c)


def badge(c, cx, cy, r, mat):
    return c.add(c.circle(cx, cy, r), mat, gap=1.8, gloss_h=0.55)


def check(c, cx, cy, r, b):
    w = 3.4 if not c.small else 4.2
    m = ImageChops.lighter(c.capsule(cx - r * 0.48, cy + r * 0.02, cx - r * 0.12, cy + r * 0.38, w),
                           c.capsule(cx - r * 0.12, cy + r * 0.38, cx + r * 0.5, cy - r * 0.32, w))
    c.add(m, "glyph", gloss=0, sunk=b)


def delete_completed(c):
    bin_(c, x=-3, y=-1, s=0.9, ribs=True)
    b = badge(c, 35, 35, 11.5, "green")
    check(c, 35, 35, 11.5, b)


def options(c):
    cx, cy = 24, 24
    teeth, ro, rr = 8, 21, 16
    pts = []
    for i in range(teeth):
        a = 2 * math.pi * i / teeth - math.pi / 2
        half = math.pi / teeth
        for da, r in ((-half * 0.92, rr), (-half * 0.48, ro), (half * 0.48, ro), (half * 0.92, rr)):
            pts.append((cx + r * math.cos(a + da), cy + r * math.sin(a + da)))
    gear = c.poly(pts)
    gear = ImageChops.lighter(gear, c.circle(cx, cy, rr + 0.3))
    hole = c.circle(cx, cy, 6.5 if not c.small else 7)
    c.add(ImageChops.subtract(gear, hole), "gearsteel", gloss_h=0.5)


def scheduler(c):
    cx, cy, r = 24, 26.5, 17.5
    # Feet and bells behind the clock.
    feet = ImageChops.lighter(c.capsule(13, 40, 9.5, 45, 4), c.capsule(35, 40, 38.5, 45, 4))
    c.add(feet, "amber", gloss=0)
    bells = ImageChops.lighter(c.circle(10.5, 11.5, 7.5), c.circle(37.5, 11.5, 7.5))
    bells = ImageChops.subtract(bells, c.circle(cx, cy, r + 1.5))
    c.add(bells, "amber", gloss_h=0.7)
    c.add(c.circle(cx, cy, r), "amber", gloss_h=0.5)
    face = c.add(c.circle(cx, cy, r - 4.5), "paper", gloss=0.5, shade=0.2)
    if not c.small:
        ticks = c.new()
        for i in range(12):
            a = 2 * math.pi * i / 12
            r0, r1 = (r - 7.5, r - 6.2) if i % 3 else (r - 8.4, r - 6.2)
            ticks = ImageChops.lighter(ticks, c.capsule(cx + r0 * math.sin(a), cy - r0 * math.cos(a),
                                                        cx + r1 * math.sin(a), cy - r1 * math.cos(a),
                                                        1.4 if i % 3 else 1.9))
        c.add(ticks, "ink", gloss=0, shade=0)
    w = 2.8 if not c.small else 3.4
    hands = ImageChops.lighter(c.capsule(cx, cy, cx, cy - 8.5, w), c.capsule(cx, cy, cx + 6.5, cy + 2.5, w))
    hands = ImageChops.lighter(hands, c.circle(cx, cy, w * 0.9))
    c.add(hands, "ink", gloss=0, shade=0, sunk=face)


def stack(c):
    """Three cards, one in front of the other (a queue of downloads)."""
    cards = [(15, 4, 43, 26), (10, 10, 38, 32), (5, 16, 33, 38)]
    for i, (x0, y0, x1, y1) in enumerate(cards):
        m = c.rrect(x0, y0, x1, y1, 3)
        c.add(m, "card", gloss_h=0.5, gap=1.4 if i else None, shade=0.25)
    if not c.small:
        x0, y0, x1, y1 = cards[-1]
        lines = c.new()
        for ly in (y0 + 7, y0 + 12.5):
            lines = ImageChops.lighter(lines, c.capsule(x0 + 6, ly, x1 - 9, ly, 2.4))
        c.add(lines, "glyph", gloss=0, shade=0)


def start_queue(c):
    stack(c)
    b = badge(c, 35, 35, 12, "green")
    tri = c.poly([(31.6, 28.2), (31.6, 41.8), (42.4, 35)])
    c.add(tri, "glyph", gloss=0, sunk=b)


def stop_queue(c):
    stack(c)
    b = badge(c, 35, 35, 12, "red")
    s = 5
    c.add(c.rrect(35 - s, 35 - s, 35 + s, 35 + s, 1.3), "glyph", gloss=0, sunk=b)


def ring(c, cx, cy, rx, ry, w):
    """An ellipse's outline, w units wide, centred on its path."""
    m = c.new()
    p = c.px
    h = w / 2
    ImageDraw.Draw(m).ellipse((p(cx - rx - h), p(cy - ry - h), p(cx + rx + h), p(cy + ry + h)),
                              outline=255, width=max(1, round(p(w))))
    return m


def grabber(c):
    # A sea-blue globe, its meridians and parallels in white, with a green badge carrying a white
    # arrow down: a web site's files coming down.
    cx, cy, r = 21, 21, 18
    g = c.add(c.circle(cx, cy, r), "globe", gloss_h=0.5)
    w = 2.2 if not c.small else 3.4
    lines = ImageChops.lighter(c.capsule(cx - r, cy, cx + r, cy, w), ring(c, cx, cy, r * 0.42, r, w))
    if not c.small:
        for dy in (-9.5, 9.5):
            half = math.sqrt(r * r - dy * dy)
            lines = ImageChops.lighter(lines, c.capsule(cx - half, cy + dy, cx + half, cy + dy, w))
        lines = ImageChops.lighter(lines, ring(c, cx, cy, r * 0.82, r, w))
    lines = ImageChops.multiply(lines, erode(g, c.line_w() * 1.5))
    c.add(lines, "glyph", gloss=0, shade=0, sunk=g)
    b = badge(c, 35, 35, 12, "green")
    c.add(arrow_down(c, 35, 27.6, 42.2, 5 if not c.small else 6, 14 if not c.small else 15),
          "glyph", gloss=0, sunk=b)


ICONS = {
    "tb-add-url": add_url,
    "tb-add-torrent": add_torrent,
    "tb-resume": resume,
    "tb-stop": stop,
    "tb-stop-all": stop_all,
    "tb-delete": delete,
    "tb-delete-completed": delete_completed,
    "tb-options": options,
    "tb-scheduler": scheduler,
    "tb-start-queue": start_queue,
    "tb-stop-queue": stop_queue,
    "tb-grabber": grabber,
}


def draw(name, size):
    c = Canvas(size)
    ICONS[name](c)
    return c.render()


def greyed(im):
    """A disabled button's icon, as the app makes it (icons.rs `greyed`): grey by its lightness,
    its contrast halved toward a middle grey, and half as opaque."""
    px = im.load()
    out = im.copy()
    po = out.load()
    for y in range(im.height):
        for x in range(im.width):
            r, g, b, a = px[x, y]
            lum = (r * 54 + g * 183 + b * 19) >> 8
            v = (lum + 150) // 2
            po[x, y] = (v, v, v, a * 55 // 100)
    return out


def preview(icons):
    font = ImageFont.load_default(13)
    names = list(ICONS)
    grounds = [("light toolbar", (240, 240, 240), (60, 64, 72)),
               ("dark toolbar", (32, 34, 40), (190, 196, 206))]
    gap = 14
    col = 64 + gap
    width = 150 + len(names) * col
    rows = [(s, False) for s in SIZES] + [(32, True), (16, True)]
    height = sum(max(s, 20) + gap for s, _ in rows) + 40
    sheet = Image.new("RGB", (width, height * len(grounds)), (255, 255, 255))
    d = ImageDraw.Draw(sheet)
    y = 0
    for gname, bg, ink in grounds:
        d.rectangle((0, y, width, y + height), fill=bg)
        d.text((10, y + 10), gname, font=font, fill=ink)
        yy = y + 34
        for s, grey in rows:
            d.text((10, yy + max(0, s // 2 - 7)), f"{s} px" + (" greyed" if grey else ""), font=font,
                   fill=ink)
            for i, n in enumerate(names):
                im = icons[(n, s)]
                if grey:
                    im = greyed(im)
                sheet.paste(im, (150 + i * col + (64 - s) // 2, yy), im)
            yy += max(s, 20) + gap
        y += height
    return sheet


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--preview", action="store_true", help="also write art/toolbar-3d-preview.png")
    ap.add_argument("--only", help="draw one icon (its name, e.g. tb-stop): its files at every "
                    "size, or its 64-pixel one into --out")
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()
    if args.only and args.only not in ICONS:
        ap.error(f"no icon named {args.only}")
    names = [args.only] if args.only else list(ICONS)
    icons = {}
    # The preview shows every icon, so it draws them all; only `names` are written.
    for name in ICONS if args.preview else names:
        for s in SIZES:
            icons[(name, s)] = draw(name, s)
    written = 0
    if args.only and args.out:
        icons[(args.only, 64)].save(args.out)
        written = 1
    else:
        for name in names:
            for s in SIZES:
                d = OUT / str(s)
                d.mkdir(parents=True, exist_ok=True)
                icons[(name, s)].save(d / f"{name}.png", optimize=True)
                written += 1
    if args.preview:
        preview(icons).save(PREVIEW, optimize=True)
    print(f"wrote {written} icons" + (" and the preview" if args.preview else ""))


if __name__ == "__main__":
    main()
