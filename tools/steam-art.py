#!/usr/bin/env python3
"""Draws Turbine Hub's Steam library art into packaging/steam-art: the cover (600x900), the wide capsule (920x430),
the hero (1920x620), the logo (see-through) and the icon. Everything is drawn here from the turbine of the app icon
(packaging/io.github.ravenor907.TurbineHub.svg) and the brass palette, so the pictures can be made again exactly:

    python3 tools/steam-art.py          needs Pillow and fonts/Cinzel.ttf (tools/get-fonts.sh)

The files carry no text chunks or other extras, only the picture."""
import math
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

# One blade of the icon, on its 512 grid around the middle: a closed shape of two curves.
BLADE = [((-20, -50), (-34, -112), (8, -160), (80, -146)), ((80, -146), (44, -118), (34, -80), (38, -36))]


def curve(p0, p1, p2, p3, steps=40):
    for i in range(steps + 1):
        t = i / steps
        a, b, c, d = (1 - t) ** 3, 3 * t * (1 - t) ** 2, 3 * t * t * (1 - t), t ** 3
        yield (a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1])


def turbine_mask(size):
    """The turbine as a mask (white on black), filling a square of this many pixels."""
    big = size * SS
    m = Image.new("L", (big, big), 0)
    d = ImageDraw.Draw(m)
    k = big / 416.0  # the ring's outer edge is 193 from the middle on the icon's grid; a little air around it
    mid = big / 2

    def at(p, turn):
        c, s = math.cos(turn), math.sin(turn)
        return (mid + (p[0] * c - p[1] * s) * k, mid + (p[0] * s + p[1] * c) * k)

    d.ellipse([mid - 193 * k, mid - 193 * k, mid + 193 * k, mid + 193 * k], fill=255)
    d.ellipse([mid - 163 * k, mid - 163 * k, mid + 163 * k, mid + 163 * k], fill=0)
    for quarter in range(4):
        turn = quarter * math.pi / 2
        d.polygon([at(p, turn) for seg in BLADE for p in curve(*seg)], fill=255)
    d.ellipse([mid - 52 * k, mid - 52 * k, mid + 52 * k, mid + 52 * k], fill=255)
    d.ellipse([mid - 20 * k, mid - 20 * k, mid + 20 * k, mid + 20 * k], fill=0)
    return m.resize((size, size), Image.LANCZOS)


def brass(size, top=BRASS_HI, bottom=BRASS):
    """A brass fill: lighter at the top, as on a lit piece of metal."""
    g = Image.new("RGB", (1, size))
    for y in range(size):
        t = y / max(1, size - 1)
        g.putpixel((0, y), tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3)))
    return g.resize((size, size))


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
    im.paste(brass(size) if not faint else Image.new("RGB", (size, size), BRASS), box, mask)


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
    layer.paste(brass(300), (40, 50))
    out = layer.convert("RGBA")
    out.putalpha(alpha)
    return out


def icon():
    size = 256
    big = size * SS
    plate = Image.new("L", (big, big), 0)
    ImageDraw.Draw(plate).rounded_rectangle([10 * SS, 10 * SS, big - 10 * SS, big - 10 * SS], radius=50 * SS, fill=255)
    plate = plate.resize((size, size), Image.LANCZOS)
    im = Image.new("RGB", (size, size), GROUND)
    put_turbine(im, (size // 2, size // 2), 196)
    out = im.convert("RGBA")
    out.putalpha(plate)
    return out


def main():
    if not os.path.isfile(FONT):
        sys.exit("fonts/Cinzel.ttf is missing: run sh tools/get-fonts.sh first")
    save(cover(), "capsule.png")
    save(wide(), "wide.png")
    save(hero(), "hero.png")
    save(logo(), "logo.png")
    save(icon(), "icon.png")


if __name__ == "__main__":
    main()
