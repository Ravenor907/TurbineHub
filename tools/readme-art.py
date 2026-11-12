#!/usr/bin/env python3
"""Draws the pictures on Turbine Hub's GitHub page into docs/art: the banner at the top of the README (1600x480)
and the picture GitHub shows when the page is shared (1280x640). Made from the emblem and the jade palette with
tools/steam-art.py's drawing, so they match the Steam library art:

    python3 tools/readme-art.py          needs Pillow and the fonts (tools/get-fonts.sh)

Also copies the README's screenshots out of a picture run (tools/ci-shots.sh) into docs/screenshots, with nothing
in the files but the picture:

    python3 tools/readme-art.py --shots <ci-shots out dir>"""
import os
import sys

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import importlib.util

spec = importlib.util.spec_from_file_location("steam_art", os.path.join(os.path.dirname(os.path.abspath(__file__)), "steam-art.py"))
art = importlib.util.module_from_spec(spec)
spec.loader.exec_module(art)

ROOT = art.ROOT
OUT = os.path.join(ROOT, "docs", "art")
SHOTS = os.path.join(ROOT, "docs", "screenshots")
BODY = os.path.join(ROOT, "fonts", "Barlow-Medium.ttf")
SOFT = (186, 204, 196)

# The README's screenshots: the name in docs/screenshots, the picture run's size and name.
README_SHOTS = [
    ("pick", "1280x800", "pick"),
    ("home-guided", "1280x800", "home-guided"),
    ("home-veteran", "1280x800", "home-veteran"),
    ("games", "1280x800", "games-switches"),
    ("store", "1280x800", "store-launchers-nsl"),
    ("machine", "1280x800", "machine"),
    ("lighting", "1280x800", "lighting"),
    ("quick-setup", "1280x800", "quick-setup"),
    ("tuneup", "1280x800", "tuneup"),
    ("dialog", "1280x800", "dialog"),
    ("tv-home", "1920x1080", "tv-home"),
    ("desktop-window", "1600x900", "pc-home"),
]


TAGLINE = "HARNESS THE POWER OF STEAMOS"


def line(im, text, middle, height, fill=SOFT, face=BODY):
    font = ImageFont.truetype(face, height)
    ImageDraw.Draw(im).text(middle, text, font=font, fill=fill, anchor="mm")


def banner():
    w, h = 1600, 480
    im = art.ground(w, h, (330, h // 2), 360)
    art.put_turbine(im, (330, h // 2), 640, faint=40)
    art.put_turbine(im, (330, h // 2), 300)
    art.words(im, "TURBINE", (1000, 150), 104)
    art.rule(im, (1000, 222), 520)
    art.words(im, "HUB", (1000, 292), 104, spacing=0.5)
    art.words(im, TAGLINE, (1000, 385), 30, spacing=0.08)
    return im


def social():
    w, h = 1280, 640
    im = art.ground(w, h, (w // 2, 250), 330)
    art.put_turbine(im, (w // 2, 250), 560, faint=40)
    art.put_turbine(im, (w // 2, 250), 290)
    art.words(im, "TURBINE HUB", (w // 2, 485), 70, spacing=0.1)
    art.words(im, TAGLINE, (w // 2, 565), 30, spacing=0.08)
    return im


def save(im, folder, name):
    os.makedirs(folder, exist_ok=True)
    clean = Image.frombytes(im.mode, im.size, im.tobytes())
    clean.save(os.path.join(folder, name), "PNG", optimize=True)
    print(name, im.size)


def shots(run):
    for name, size, shot in README_SHOTS:
        src = os.path.join(run, size, shot + ".png")
        if not os.path.isfile(src):
            sys.exit(f"{src} is missing: run sh tools/ci-shots.sh all <dir> first")
        save(Image.open(src).convert("RGB"), SHOTS, name + ".png")


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--shots":
        shots(sys.argv[2])
        return
    for f in (art.FONT, BODY):
        if not os.path.isfile(f):
            sys.exit(f"{f} is missing: run sh tools/get-fonts.sh first")
    save(banner(), OUT, "banner.png")
    save(social(), OUT, "social.png")


if __name__ == "__main__":
    main()
