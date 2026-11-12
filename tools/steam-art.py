#!/usr/bin/env python3
"""Draws Turbine Hub's Steam library art into packaging/steam-art: the cover (600x900), the wide capsule (920x430),
the hero (1920x620), the logo (see-through) and the icon. Everything is drawn here from the emblem
(packaging/emblem.png: its shape is what counts) and the jade palette on true black, so the pictures can be made
again exactly. Turbine Hub's mark is the turbine in jade inside a hex nut (the hub); Turbine Mod Manager's is the same
turbine in brass, so the two are told apart at a glance:

    python3 tools/steam-art.py          needs Pillow and fonts/Cinzel.ttf (tools/get-fonts.sh)

The files carry no text chunks or other extras, only the picture."""
import os
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "packaging", "steam-art")
FONT = os.path.join(ROOT, "fonts", "Cinzel.ttf")

GROUND = (0, 0, 0)
PANEL = (10, 14, 12)
BRASS = (111, 174, 140)  # the app's jade accent
BRASS_HI = (190, 234, 212)
BRASS_LO = (30, 92, 70)
SS = 3  # every picture is drawn this many times larger, then scaled down, for smooth edges

EMBLEM = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "packaging", "emblem.png")


def metal(size):
    """Brushed jade: light at the top left, deep at the bottom right, with a soft band of light across."""
    import math
    n = 128
    g = Image.new("RGB", (n, n))
    px = g.load()
    ca, sa = math.cos(math.radians(115)), math.sin(math.radians(115))
    for y in range(n):
        for x in range(n):
            t = min(1.0, max(0.0, ((x - n / 2) * ca + (y - n / 2) * sa) / (n * 0.7) + 0.5))
            k = min(1.0, max(0.0, t + 0.10 * math.sin(t * math.pi * 3)))
            px[x, y] = tuple(round(BRASS_HI[i] + (BRASS_LO[i] - BRASS_HI[i]) * k) for i in range(3))
    return g.resize((size, size), Image.BICUBIC)


def emblem(size):
    """The mark (the turbine's shape from packaging/emblem.png, in brushed jade on a see-through ground)."""
    mask = Image.open(EMBLEM).convert("RGBA").resize((size, size), Image.LANCZOS).getchannel("A")
    out = metal(size).convert("RGBA")
    out.putalpha(mask)
    return out


def blades(size):
    """The turbine without its outer ring (the hex takes the ring's place), in brushed jade."""
    full = Image.open(EMBLEM).convert("RGBA").getchannel("A")
    n = full.width
    px = full.load()
    # Walk in from the right edge along the middle row: past the ring, the first gap is where the blades begin.
    r, seen = n // 2 - 2, False
    while r > 0:
        if px[n // 2 + r, n // 2] > 128:
            seen = True
        elif seen:
            break
        r -= 1
    keep = Image.new("L", (n, n), 0)
    ImageDraw.Draw(keep).ellipse([n // 2 - r + 2, n // 2 - r + 2, n // 2 + r - 2, n // 2 + r - 2], fill=255)
    mask = ImageChops.multiply(full, keep).crop((n // 2 - r, n // 2 - r, n // 2 + r, n // 2 + r)).resize((size, size), Image.LANCZOS)
    out = metal(size).convert("RGBA")
    out.putalpha(mask)
    return out


def hexagon(middle, radius):
    """A hexagon standing on a point."""
    import math
    return [(middle + radius * math.cos(math.pi / 6 + math.pi / 3 * i), middle + radius * math.sin(math.pi / 6 + math.pi / 3 * i)) for i in range(6)]


def tile(size):
    """The app icon: a jade hex nut (the hub) with a dark face and the turbine's blades inside."""
    big = size * SS
    rim = Image.new("L", (big, big), 0)
    ImageDraw.Draw(rim).polygon(hexagon(big / 2, big * 0.49), fill=255)
    im = metal(big).convert("RGBA")
    im.putalpha(rim)
    face = Image.new("L", (big, big), 0)
    ImageDraw.Draw(face).polygon(hexagon(big / 2, big * 0.425), fill=255)
    im.paste(Image.new("RGBA", (big, big), (4, 6, 5, 255)), (0, 0), face)
    mark = blades(round(big * 0.60))
    im.alpha_composite(mark, ((big - mark.width) // 2, (big - mark.height) // 2))
    return im.resize((size, size), Image.LANCZOS)


def turbine_mask(size):
    """The emblem as a mask (white where it is), filling a square of this many pixels."""
    return emblem(size).getchannel("A")


def ground(w, h, glow_at, glow_size):
    """The dark ground with a warm glow behind where the turbine sits."""
    im = Image.new("RGB", (w, h), GROUND)
    glow = Image.new("L", (w, h), 0)
    x, y = glow_at
    ImageDraw.Draw(glow).ellipse([x - glow_size, y - glow_size, x + glow_size, y + glow_size], fill=70)
    glow = glow.filter(ImageFilter.GaussianBlur(glow_size * 0.45))
    im.paste(Image.new("RGB", (w, h), BRASS_LO), (0, 0), glow)
    return im


def put_turbine(im, middle, size, faint=0):
    """The mark, its middle at `middle`: the hex icon, or with `faint` (how see-through, 0 to 255) the bare turbine
    as a watermark behind it."""
    box = (round(middle[0] - size / 2), round(middle[1] - size / 2))
    if faint:
        mask = turbine_mask(size).point(lambda v: v * faint // 255)
        im.paste(Image.new("RGB", (size, size), BRASS), box, mask)
    else:
        mark = tile(size)
        im.paste(mark.convert("RGB"), box, mark.getchannel("A"))


def words(im, text, middle, height, spacing=0.12, fill=BRASS_HI):
    """The name in the title font, capitals letter-spaced, its middle at `middle`."""
    font = ImageFont.truetype(FONT, height)
    d = ImageDraw.Draw(im)
    gap = height * spacing
    widths = [d.textlength(c, font=font) for c in text]
    total = sum(widths) + gap * (len(text) - 1)
    x = middle[0] - total / 2
    for c, w in zip(text, widths):
        d.text((x, middle[1]), c, font=font, fill=fill, anchor="lm")
        x += w + gap
    return total


def rule(im, middle, width, color=BRASS_LO):
    d = ImageDraw.Draw(im)
    d.line([middle[0] - width / 2, middle[1], middle[0] + width / 2, middle[1]], fill=color, width=max(1, im.height // 300))


def save(im, name):
    os.makedirs(OUT, exist_ok=True)
    # A fresh image holds the pixels and nothing else.
    clean = Image.frombytes(im.mode, im.size, im.tobytes())
    clean.save(os.path.join(OUT, name), "PNG", optimize=True)
    print(name, im.size, im.mode)


def cover():
    w, h = 600, 900
    im = ground(w, h, (w // 2, 350), 330)
    put_turbine(im, (w // 2, 350), 380)
    words(im, "TURBINE", (w // 2, 640), 74)
    rule(im, (w // 2, 698), 300)
    words(im, "HUB", (w // 2, 752), 74, spacing=0.5)
    return im


def wide():
    w, h = 920, 430
    im = ground(w, h, (215, h // 2), 240)
    put_turbine(im, (215, h // 2), 290)
    words(im, "TURBINE", (630, 172), 80)
    rule(im, (630, 228), 330)
    words(im, "HUB", (630, 282), 80, spacing=0.5)
    return im


def hero():
    w, h = 1920, 620
    im = ground(w, h, (1420, h // 2), 520)
    put_turbine(im, (1420, h // 2), 900, faint=46)
    put_turbine(im, (1420, h // 2), 360)
    return im


def logo():
    w, h = 1000, 400
    layer = Image.new("RGB", (w, h), BRASS)
    alpha = Image.new("L", (w, h), 0)
    alpha.paste(tile(300).getchannel("A"), (40, 50))
    text = Image.new("L", (w, h), 0)
    words(text, "TURBINE", (680, 150), 96, fill=255)
    rule(text, (680, 216), 400, color=200)
    words(text, "HUB", (680, 280), 96, spacing=0.5, fill=255)
    alpha = ImageChops.lighter(alpha, text)
    layer.paste(tile(300).convert("RGB"), (40, 50))
    out = layer.convert("RGBA")
    out.putalpha(alpha)
    return out


def icon():
    """The app icon, as Steam's small icon."""
    return tile(256)


def app_icons():
    """The app's icon at each size the desktop asks for, and the mark itself for the app's own screens."""
    for n in (32, 48, 64, 128, 256, 512):
        out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "packaging", f"io.github.ravenor907.TurbineHub-{n}.png")
        clean = tile(n)
        Image.frombytes(clean.mode, clean.size, clean.tobytes()).save(out, optimize=True)
    mark = emblem(1024)
    Image.frombytes(mark.mode, mark.size, mark.tobytes()).save(EMBLEM, optimize=True)


def main():
    if not os.path.isfile(FONT):
        sys.exit("fonts/Cinzel.ttf is missing: run sh tools/get-fonts.sh first")
    save(cover(), "capsule.png")
    save(wide(), "wide.png")
    save(hero(), "hero.png")
    save(logo(), "logo.png")
    save(icon(), "icon.png")
    app_icons()


if __name__ == "__main__":
    main()
