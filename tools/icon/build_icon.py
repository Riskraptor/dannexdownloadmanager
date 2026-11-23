"""Builds every size of Dannex Download Manager's icon ("Comet", tools/icon/comet_logo.py), each
drawn at its own size from its SVG (never scaled down from a bigger picture):

- 16, 20, 24 and 32 px: the pixel masters (the tile filling the square, the shapes placed for
  whole pixels);
- 40 px and up: the full drawing, with its light, halo and the tile's shadow.

Writes:
- art/Dannex Download Manager.png, the 1024 px master (the Linux packages are made from it:
  tools/installer/build-linux.sh; so are the README's social card and the browser stores'
  tiles), and art/Dannex Download Manager (macOS).png, the same on macOS's icon grid (the tile
  824 px of 1024) for the macOS package (build-macos.sh);
- crates/app/assets/icons/app/mdm-<size>.png and mdm.ico (every size), the app's own (the
  window's title bar, taskbar and Alt+Tab icons and the tray's 3D look are picked from them by
  size; the phone remote serves the 256 px one);
- crates/app/assets/icons/app/mark-<size>.png, the mark alone in white on nothing (the
  notification area's classic icon, inked white or dark for the taskbar by the app);
- crates/app/assets/icons/app/drop-target.svg and drop-target-hot.svg, Tasks → Show drop
  target's face, and its look while something droppable is over it (drawn by the app at its size);
- extension/icons/icon-<size>.png, and the copy of the 48 px one that extension/video.js
  carries as a data: URL (tools/extension/test.mjs checks the two are the same), and
  icon-off-<size>.png, the toolbar button's look while it takes no downloads (grey, a dark
  badge with a white X);
- the website's favicon.ico, favicon-32.png, apple-touch-icon.png (a full square: iOS rounds it
  itself) and logo-64/256/640, and the README's docs/images/logo.png;
- tools/installer/wizard/: the Inno Setup wizard's panels (side-<w>x<h>.png for the Welcome and
  Finished pages, small-<n>.png for the header), one per screen scale from 100 to 250%;
- art/icon-preview.png, a sheet to check them by eye: every size at 1:1 on light, dark and night;
  the small ones magnified; the tray, classic and 3D, with its progress bar; the off look in a
  light and a dark toolbar; the installer's side panel.

Run from anywhere: python tools/icon/build_icon.py (plain python, not python -I, if Pillow is
installed in the user site; resvg as comet_logo.py says). Then python tools/readme/social.py for
the social preview card, python tools/readme/icons.py for the README's block icons, and
python tools/extension/store/make_images.py for the stores' tiles.
"""

import base64
import re
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).resolve().parent))  # also under python -I (no script dir)
import comet_logo  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / "crates" / "app" / "assets" / "icons" / "app"
EXT = ROOT / "extension" / "icons"
VIDEO_JS = ROOT / "extension" / "video.js"
SITE = ROOT / "website"
WIZARD = ROOT / "tools" / "installer" / "wizard"
# The master on macOS's icon grid, for the .app's AppIcon.icns (tools/installer/build-macos.sh).
MACOS_MASTER = ROOT / "art" / "Dannex Download Manager (macOS).png"
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]
MARKS = [16, 20, 24, 32, 40, 48]
# The extension's toolbar button sizes (its manifest's default_icon, background.js ICONS).
TOOLBAR = (16, 24, 32)
# The tray's 3D look while downloads run (crates/app/src/tray.rs, picture_rgba): a dim indigo
# groove along the icon's foot, filled in amber.
TRAY_TRACK = (0x4A, 0x58, 0xA8, 0xFF)
TRAY_FILL = (0xFF, 0xB2, 0x3E, 0xFF)


def save_ico(path: Path, icons: dict) -> None:
    sizes = sorted(icons)
    big = icons[sizes[-1]]
    big.save(path, sizes=[(s, s) for s in sizes], append_images=[icons[s] for s in sizes[:-1]])


def tray_bar(icon: Image.Image, done: float) -> Image.Image:
    """The icon with the tray's progress bar along its foot, as tray.rs draws it (bar_rect)."""
    side = icon.width
    h = max(2, side // 10)
    x0 = max(2, side // 8)
    y1 = side - max(1, side // 16)
    out = icon.copy()
    px = out.load()
    for y in range(y1 - h, y1):
        for x in range(x0, side - x0):
            px[x, y] = TRAY_FILL if x - x0 < round(done * (side - 2 * x0)) else TRAY_TRACK
    return out


def preview(icons: dict, marks: dict, off: dict, side: Image.Image) -> Image.Image:
    font = ImageFont.load_default(14)
    sizes = sorted(icons, reverse=True)
    grounds = [("light", (246, 247, 249), (60, 64, 72)), ("dark", (26, 28, 34), (190, 196, 206)),
               ("night", (17, 24, 66), (220, 226, 240))]
    gap, row_h = 24, 256 + 56
    zoom = [16, 20, 24, 32]
    width = max(gap + 70 + sum(s + gap for s in sizes), 1180)
    zoom_h = 128 + 44
    tray_h = 30 + 24 * 4 + 16
    sheet = Image.new("RGB", (width, row_h * len(grounds) + zoom_h * 2 + tray_h * 3 + 90 + 220), (255, 255, 255))
    draw = ImageDraw.Draw(sheet)
    y = 0
    for name, bg, ink in grounds:
        draw.rectangle((0, y, width, y + row_h), fill=bg)
        draw.text((gap, y + 10), f"Comet, {name}", font=font, fill=ink)
        x = gap + 70
        for s in sizes:
            sheet.paste(icons[s], (x, y + 32 + (256 - s) // 2), icons[s])
            draw.text((x, y + 32 + 256 + 4), f"{s}", font=font, fill=ink)
            x += s + gap
        y += row_h
    for name, bg, ink in grounds[:2]:
        draw.rectangle((0, y, width, y + zoom_h), fill=bg)
        draw.text((gap, y + 8), f"{name}, magnified", font=font, fill=ink)
        x = gap
        for s in zoom:
            f_ = 128 // s
            tile = Image.new("RGB", (s, s), bg)
            tile.paste(icons[s], (0, 0), icons[s])
            sheet.paste(tile.resize((s * f_, s * f_), Image.NEAREST), (x, y + 30))
            sheet.paste(tile, (x + s * f_ + 8, y + 30))
            x += s * f_ + s + 24
        y += zoom_h
    # The tray: classic (the mark, inked as the app inks it) on a dark and a light taskbar, then
    # the 3D look with downloads running (the bar a third and two thirds done).
    rows = (("tray, classic: dark taskbar", (32, 32, 32), (200, 200, 200), (255, 255, 255)),
            ("tray, classic: light taskbar", (238, 238, 238), (60, 60, 60), (27, 27, 31)))
    for name, bg, ink, mark_ink in rows:
        draw.rectangle((0, y, width, y + tray_h), fill=bg)
        draw.text((gap, y + 8), name, font=font, fill=ink)
        x = gap
        for s in sorted(marks):
            m = marks[s]
            inked = Image.new("RGBA", m.size, mark_ink + (0,))
            inked.putalpha(m.getchannel("A"))
            sheet.paste(inked, (x, y + 30), inked)
            if s <= 24:
                t = Image.new("RGB", (s, s), bg)
                t.paste(inked, (0, 0), inked)
                sheet.paste(t.resize((s * 4, s * 4), Image.NEAREST), (x + s + 8, y + 30))
                x += s + 8 + s * 4 + 24
            else:
                x += s + 24
        y += tray_h
    draw.rectangle((0, y, width, y + tray_h), fill=(32, 32, 32))
    draw.text((gap, y + 8), "tray, 3D, downloads running (a third, two thirds)", font=font, fill=(200, 200, 200))
    x = gap
    for s in (16, 24, 32):
        for done in (1 / 3, 2 / 3):
            im = tray_bar(icons[s], done)
            sheet.paste(im, (x, y + 30), im)
            t = Image.new("RGB", (s, s), (32, 32, 32))
            t.paste(im, (0, 0), im)
            sheet.paste(t.resize((s * 3, s * 3), Image.NEAREST), (x + s + 6, y + 30))
            x += s + 6 + s * 3 + 22
    y += tray_h
    # The extension's off look in a light and a dark toolbar, beside the on look.
    for i, (bg, ink) in enumerate((((255, 255, 255), (60, 60, 60)), ((53, 54, 58), (200, 200, 200)))):
        x0 = i * (width // 2)
        draw.rectangle((x0, y, x0 + width // 2, y + 90), fill=bg)
        draw.text((x0 + gap, y + 8), "toolbar: on, off (16, 24, 32), off magnified", font=font, fill=ink)
        x = x0 + gap
        for s in TOOLBAR:
            sheet.paste(icons[s], (x, y + 34), icons[s])
            sheet.paste(off[s], (x + s + 6, y + 34), off[s])
            x += 2 * s + 20
        t = Image.new("RGB", (16, 16), bg)
        t.paste(off[16], (0, 0), off[16])
        sheet.paste(t.resize((48, 48), Image.NEAREST), (x + 10, y + 30))
    y += 90
    draw.rectangle((0, y, width, y + 220), fill=(240, 240, 240))
    draw.text((gap, y + 8), "installer: the Welcome page's panel (100%) and the header image", font=font, fill=(60, 60, 60))
    thumb = side.resize((side.width * 180 // side.height, 180), Image.LANCZOS)
    sheet.paste(thumb, (gap, y + 30), thumb)
    small = Image.open(WIZARD / "small-58.png").convert("RGBA")
    sheet.paste(small, (gap + thumb.width + 30, y + 30), small)
    return sheet


def main() -> None:
    master = comet_logo.render(comet_logo.MASTER_SIZE)
    master.save(comet_logo.MASTER, optimize=True)
    comet_logo.render_macos(comet_logo.MASTER_SIZE).save(MACOS_MASTER, optimize=True)

    icons = {size: comet_logo.render(size) for size in SIZES}
    for size, im in icons.items():
        im.save(APP / f"mdm-{size}.png", optimize=True)
    save_ico(APP / "mdm.ico", icons)

    marks = {size: comet_logo.render_mark(size) for size in MARKS}
    for size, im in marks.items():
        im.save(APP / f"mark-{size}.png", optimize=True)
    comet_logo.write_drop_target(APP)

    for size in (16, 24, 32, 48, 128):
        icons[size].save(EXT / f"icon-{size}.png", optimize=True)
    # The toolbar button's look while it takes no downloads (off, DDM not reached).
    off = {size: comet_logo.off_icon(size) for size in TOOLBAR}
    for size, im in off.items():
        im.save(EXT / f"icon-off-{size}.png", optimize=True)
    # video.js's LOGO: the 48 px icon's very bytes, as written above.
    data = base64.b64encode((EXT / "icon-48.png").read_bytes())
    js, n = re.subn(rb'const LOGO = "data:image/png;base64,[A-Za-z0-9+/=]*";',
                    b'const LOGO = "data:image/png;base64,' + data + b'";', VIDEO_JS.read_bytes())
    if n != 1:
        raise SystemExit('extension/video.js: no `const LOGO = "data:image/png;base64,...";` line')
    VIDEO_JS.write_bytes(js)

    save_ico(SITE / "favicon.ico", {s: icons[s] for s in (16, 32, 48, 64)})
    icons[32].save(SITE / "assets" / "favicon-32.png", optimize=True)
    icons[64].save(SITE / "assets" / "logo-64.png", optimize=True)
    icons[256].save(SITE / "assets" / "logo-256.webp", lossless=True, quality=100, method=6)
    comet_logo.render(640).save(SITE / "assets" / "logo-640.png", optimize=True)
    touch = comet_logo.render_full(180, square=True).convert("RGB")
    touch.save(SITE / "assets" / "apple-touch-icon.png", optimize=True)
    comet_logo.render(320).save(ROOT / "docs" / "images" / "logo.png", optimize=True)

    # The installer's panels: Inno Setup picks the one nearest the screen's scale.
    WIZARD.mkdir(parents=True, exist_ok=True)
    for old in WIZARD.glob("*.png"):
        old.unlink()
    sides = {}
    for w, h in comet_logo.WIZARD_SIDE:
        sides[w] = comet_logo.wizard_side(w, h)
        sides[w].convert("RGB").save(WIZARD / f"side-{w}x{h}.png", optimize=True)
    for n in comet_logo.WIZARD_SMALL:
        comet_logo.render(n).save(WIZARD / f"small-{n}.png", optimize=True)

    preview(icons, marks, off, sides[comet_logo.WIZARD_SIDE[0][0]]).save(ROOT / "art" / "icon-preview.png",
                                                                           optimize=True)

    print("wrote the 1024 px master,", ", ".join(str(s) for s in SIZES),
          "+ mdm.ico, the tray's marks, the extension's icons (and video.js's copy), the website's, the "
          "README's and the installer's panels")


if __name__ == "__main__":
    main()
