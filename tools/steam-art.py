#!/usr/bin/env python3
"""Draws Turbine Hub's Steam library art into packaging/steam-art: the cover (600x900), the wide capsule (920x430),
the hero (1920x620), the logo (see-through) and the icon. Everything is drawn here from the emblem
(packaging/emblem.png) and the brass palette, so the pictures can be made again exactly:

    python3 tools/steam-art.py          needs Pillow and fonts/Cinzel.ttf (tools/get-fonts.sh)

The files carry no text chunks or other extras, only the picture."""
import os
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "packaging", "steam-art")
FONT = os.path.join(ROOT, "fonts", "Cinzel.ttf")

GROUND = (11, 10, 8)
PANEL = (20, 17, 9)
BRASS = (201, 158, 82)
BRASS_HI = (234, 205, 140)
BRASS_LO = (110, 84, 40)
SS = 3  # every picture is drawn this many times larger, then scaled down, for smooth edges

EMBLEM = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "packaging", "emblem.png")


def emblem(size):
    """The emblem (packaging/emblem.png: the gold turbine on a see-through ground) at this many pixels."""
    return Image.open(EMBLEM).convert("RGBA").resize((size, size), Image.LANCZOS)


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
    """The turbine in brass, its middle at `middle`. `faint`: 0 for the full one, else how see-through (0 to 255)."""
    mask = turbine_mask(size)
    if faint:
        mask = mask.point(lambda v: v * faint // 255)
    box = (round(middle[0] - size / 2), round(middle[1] - size / 2))
    im.paste(emblem(size).convert("RGB") if not faint else Image.new("RGB", (size, size), BRASS), box, mask)


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
    alpha.paste(turbine_mask(300), (40, 50))
    text = Image.new("L", (w, h), 0)
    words(text, "TURBINE", (680, 150), 96, fill=255)
    rule(text, (680, 216), 400, color=200)
    words(text, "HUB", (680, 280), 96, spacing=0.5, fill=255)
    alpha = ImageChops.lighter(alpha, text)
    layer.paste(emblem(300).convert("RGB"), (40, 50))
    out = layer.convert("RGBA")
    out.putalpha(alpha)
    return out


def icon():
    """The emblem alone on a see-through ground."""
    return emblem(256)


def app_icons():
    """The app's icon at each size the desktop asks for: the emblem on a see-through ground."""
    for n in (32, 48, 64, 128, 256, 512):
        out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "packaging", f"io.github.ravenor907.TurbineHub-{n}.png")
        emblem(n).save(out, optimize=True)


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
