"""The browser stores' pictures for the Dannex Download Manager extension, from the app's own windows.

    python tools/extension/store/make_images.py [<shots folder> [<dark shots folder>]]

The shots are tools/lab/docs-shots.ps1's PNGs of DDM's windows (main, progress, sitegrab,
statistics; main again in dark mode). Writes, next to this script, in images/:
  screenshot-1..5.png  1280x800  (Chrome Web Store, Microsoft Edge Add-ons, Firefox add-ons;
                                  only with a shots folder)
  promo-small.png      440x280   (Chrome Web Store's small promo tile, required)
  promo-marquee.png    1400x560  (Chrome Web Store's marquee, optional)
  icon-128.png         128x128   (the store icon: the extension's own)
Only DDM's own windows and logo appear, never another program's. The listings' texts are in
PUBLISHING.md ("Store listing texts").
"""
import shutil
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
OUT = HERE / "images"
LOGO = ROOT / "art" / "Dannex Download Manager.png"
# The icon's own colours (tools/icon/comet_logo.py): its night sky, from the light top left to
# the deep lower right, and the haze its words are set in.
TOP = (35, 47, 112)
BOTTOM = (7, 10, 30)
HAZE = (195, 205, 224)
FONT_BOLD = "C:/Windows/Fonts/seguisb.ttf"
FONT = "C:/Windows/Fonts/segoeui.ttf"


def font(size, bold=False):
    try:
        return ImageFont.truetype(FONT_BOLD if bold else FONT, size)
    except OSError:
        return ImageFont.load_default()


def backdrop(w, h):
    """The icon's night sky, diagonally."""
    img = Image.new("RGB", (w, h))
    px = img.load()
    for y in range(h):
        for x in range(w):
            t = (x / w * 0.35 + y / h * 0.65)
            px[x, y] = tuple(int(a + (b - a) * t) for a, b in zip(TOP, BOTTOM))
    return img


def place(canvas, window, box):
    """`window` scaled into `box` (x, y, w, h), centred, with a soft shadow."""
    x, y, w, h = box
    scale = min(w / window.width, h / window.height, 1.0)
    win = window.resize((int(window.width * scale), int(window.height * scale)), Image.LANCZOS)
    px = x + (w - win.width) // 2
    py = y + (h - win.height) // 2
    shadow = Image.new("RGBA", (win.width + 60, win.height + 60), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rectangle((30, 36, 30 + win.width, 36 + win.height), fill=(0, 0, 0, 110))
    shadow = shadow.filter(ImageFilter.GaussianBlur(16))
    canvas.paste(shadow, (px - 30, py - 30), shadow)
    canvas.paste(win.convert("RGB"), (px, py))


def screenshot(window, title, subtitle, out):
    canvas = backdrop(1280, 800).convert("RGB")
    d = ImageDraw.Draw(canvas)
    d.text((64, 44), title, font=font(40, True), fill="white")
    d.text((64, 98), subtitle, font=font(22), fill=HAZE)
    place(canvas, window, (64, 160, 1152, 600))
    canvas.save(out, optimize=True)


def logo(size):
    return Image.open(LOGO).convert("RGBA").resize((size, size), Image.LANCZOS)


def promo(w, h, out, big):
    canvas = backdrop(w, h).convert("RGBA")
    side = int(h * (0.5 if big else 0.42))
    canvas.alpha_composite(logo(side), (int(w * 0.06), (h - side) // 2))
    d = ImageDraw.Draw(canvas)
    x = int(w * 0.06) + side + int(w * 0.04)
    d.text((x, h // 2 - (78 if big else 44)), "Dannex", font=font(64 if big else 34, True), fill="white")
    d.text((x, h // 2 - (2 if big else 6)), "Download Manager", font=font(40 if big else 22, True), fill="white")
    d.text((x, h // 2 + (52 if big else 24)), "Browser downloads, saved by the app on your computer",
           font=font(24 if big else 13), fill=HAZE)
    canvas.convert("RGB").save(out, optimize=True)


def main():
    # Without shots, only the tiles and the icon (the screenshots need the app's windows).
    shots = Path(sys.argv[1]) if len(sys.argv) > 1 else None
    dark = Path(sys.argv[2]) if len(sys.argv) > 2 else None
    OUT.mkdir(exist_ok=True)
    shot = lambda folder, name: Image.open(folder / f"{name}.png")
    if shots:
        screenshot(shot(shots, "main"), "Your downloads, in DDM",
                   "Links you click in the browser go to Dannex Download Manager, with its full speed.",
                   OUT / "screenshot-1.png")
        screenshot(shot(shots, "progress"), "Up to 8 connections per file",
                   "Pause, resume after a restart, limit the speed, see each connection.",
                   OUT / "screenshot-2.png")
        screenshot(shot(shots, "sitegrab"), "Grab a whole site's files",
                   "Pictures, documents or videos from a site, found and downloaded into a queue.",
                   OUT / "screenshot-3.png")
        screenshot(shot(shots, "statistics"), "Statistics and a live speed graph",
                   "What you downloaded today, this week and this month, and from where.",
                   OUT / "screenshot-4.png")
    if dark:
        screenshot(shot(dark, "main"), "Light or dark, in 86 languages",
                   "Free and open source (GPL-3.0). No accounts, no ads, no tracking.",
                   OUT / "screenshot-5.png")
    promo(440, 280, OUT / "promo-small.png", big=False)
    promo(1400, 560, OUT / "promo-marquee.png", big=True)
    shutil.copy(ROOT / "extension" / "icons" / "icon-128.png", OUT / "icon-128.png")
    for p in sorted(OUT.glob("*.png")):
        im = Image.open(p)
        print(f"{p.name}: {im.size[0]}x{im.size[1]}, {p.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
