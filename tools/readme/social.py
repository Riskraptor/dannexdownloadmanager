"""The repository's social preview (GitHub → Settings → Social preview; 1280×640), also the
website's og:image (tools/website/build.py copies it to assets/social.png): the app icon, the
name, the one-line promise and the main window, on the icon's night sky. Built from the repo's
own files:

    python tools/readme/social.py        # → docs/images/social-preview.png

Fonts are Windows' own Segoe UI (rendered into the picture, not shipped).
"""

import random
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[2]
FONTS = Path("C:/Windows/Fonts")
W, H = 1280, 640
# The icon's palette (tools/icon/comet_logo.py): night, its deep end, amber, the haze for text.
NIGHT, NIGHT_DEEP, AMBER = (35, 47, 112), (7, 10, 30), (255, 178, 62)
WHITE, HAZE = (246, 248, 252), (195, 205, 224)


def font(name, size):
    return ImageFont.truetype(str(FONTS / name), size)


def sky():
    """The night: deep at the lower right, a warm glow under the window, faint stars."""
    t = Image.linear_gradient("L").resize((W, H))  # 0 at the top, 255 at the bottom
    h = Image.linear_gradient("L").rotate(90).resize((W, H))  # 0 at the left
    t = Image.blend(t, h, 0.4)
    card = Image.merge("RGB", [t.point(lambda v, c=c: round(NIGHT[c] + (NIGHT_DEEP[c] - NIGHT[c]) * v / 255))
                               for c in range(3)])
    glow = Image.radial_gradient("L").resize((1100, 700))
    glow = glow.point(lambda v: round(70 * max(0.0, 1 - v / 190) ** 1.6))
    layer = Image.new("L", (W, H), 0)
    layer.paste(glow, (W - 760, H - 330))
    card.paste((255, 106, 69), (0, 0), layer)
    stars = Image.new("L", (W, H), 0)
    d = ImageDraw.Draw(stars)
    rng = random.Random(7)
    for _ in range(70):
        x, y, r = rng.randrange(W), rng.randrange(H), rng.choice((1, 1, 1, 1.5, 2))
        d.ellipse((x - r, y - r, x + r, y + r), fill=rng.randrange(60, 150))
    card.paste((255, 255, 255), (0, 0), stars.filter(ImageFilter.GaussianBlur(0.6)))
    return card


def main():
    card = sky()
    draw = ImageDraw.Draw(card)

    # The main window, large, lower right, with a soft shadow and rounded corners.
    shot = Image.open(ROOT / "docs/images/main-light.webp").convert("RGB")
    shot = shot.resize((900, round(900 * shot.height / shot.width)), Image.LANCZOS)
    x0, y0 = W - shot.width + 120, 250
    shadow = Image.new("L", (shot.width + 80, shot.height + 80), 0)
    ImageDraw.Draw(shadow).rounded_rectangle((40, 40, 40 + shot.width, 40 + shot.height), 16, fill=170)
    shadow = shadow.filter(ImageFilter.GaussianBlur(22))
    card.paste((2, 4, 15), (x0 - 40, y0 - 26), shadow)
    mask = Image.new("L", shot.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, shot.width, shot.height), 14, fill=255)
    card.paste(shot, (x0, y0), mask)

    # The words, upper left.
    icon = Image.open(ROOT / "art/Dannex Download Manager.png").convert("RGBA").resize((120, 120), Image.LANCZOS)
    card.paste(icon, (60, 50), icon)
    draw.text((196, 58), "Dannex Download Manager", font=font("segoeuib.ttf", 56), fill=WHITE)
    draw.text((198, 132), "Downloads faster than IDM. Free, forever.", font=font("segoeui.ttf", 34), fill=AMBER)
    draw.text((66, 196), "Windows, macOS and Linux  ·  86 languages  ·  no ads, no trial, no sign-up",
              font=font("segoeui.ttf", 24), fill=HAZE)

    out = ROOT / "docs/images/social-preview.png"
    card.save(out, optimize=True)
    print(f"wrote {out.relative_to(ROOT)} ({W}x{H})")


if __name__ == "__main__":
    main()
