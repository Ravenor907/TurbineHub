#!/usr/bin/env python3
"""The build's pictures against the last green build's: which pages changed, and how.

    python3 tools/shot-diff.py <before> <after> <out> [--summary FILE] [--before-name TEXT] [--fuzz 2] [--min-pixels 8]

<before> and <after> hold PNGs (in subfolders too, the way `gh run download` and actions/download-artifact unpack
artifacts); pictures pair up by file name. A pixel differs when one of its channels moves by more than --fuzz percent;
a picture changed when more than --min-pixels differ (or its size changed). <out> gets index.html with before | after |
diff for every changed page (the diff is the new picture dimmed, with the pixels that differ in red), the new and the
removed ones, and summary.md (also appended to --summary, the job's step summary). Layout reports beside the pictures
(layout-*.txt from tools/ci-shots.sh) are summed into it too.

Standard library only: PNGs are read with zlib (Pillow is used when it is there, as it is faster). Never fails the
build: anything it can't read is listed as such.
"""
import argparse
import html
import os
import re
import shutil
import struct
import sys
import zlib
from itertools import accumulate

try:
    from PIL import Image  # optional, only for speed
except Exception:  # any import problem means no Pillow
    Image = None

PNG_SIG = b"\x89PNG\r\n\x1a\n"
LOW = (255).__and__


def read_png_plain(path):
    """(width, height, rows) with rows as RGB bytes, from a non-interlaced PNG (8 or 16 bits)."""
    data = open(path, "rb").read()
    if not data.startswith(PNG_SIG):
        raise ValueError("not a PNG")
    pos, idat, plte = 8, [], b""
    width = height = depth = ctype = interlace = None
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        pos += 12 + n
        if kind == b"IHDR":
            width, height, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
        elif kind == b"PLTE":
            plte = body
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if interlace:
        raise ValueError("interlaced PNG")
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[ctype]
    if depth not in (8, 16):
        raise ValueError("%d-bit PNG" % depth)
    bpp = channels * depth // 8
    stride = width * bpp
    raw = zlib.decompress(b"".join(idat))
    rows, prev = [], bytearray(stride)
    for y in range(height):
        f = raw[y * (stride + 1)]
        cur = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        if f == 1:  # Sub: a running sum per channel
            for c in range(bpp):
                cur[c::bpp] = bytes(map(LOW, accumulate(cur[c::bpp])))
        elif f == 2:  # Up
            cur = bytearray(map(LOW, map(int.__add__, cur, prev)))
        elif f == 3:  # Average
            for i in range(stride):
                left = cur[i - bpp] if i >= bpp else 0
                cur[i] = (cur[i] + ((left + prev[i]) >> 1)) & 255
        elif f == 4:  # Paeth
            for i in range(stride):
                a = cur[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                cur[i] = (cur[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        prev = cur
        px = bytes(cur)
        if depth == 16:
            px = px[0::2]
        if ctype == 2:
            rows.append(px)
        elif ctype == 6:
            rows.append(bytes(b for i, b in enumerate(px) if i % 4 != 3))
        elif ctype == 0:
            rows.append(bytes(v for g in px for v in (g, g, g)))
        elif ctype == 4:
            rows.append(bytes(v for g in px[0::2] for v in (g, g, g)))
        else:  # a palette
            rows.append(b"".join(plte[i * 3:i * 3 + 3] for i in px))
    return width, height, rows


def read_png(path):
    if Image is not None:
        with Image.open(path) as im:
            im = im.convert("RGB")
            w, h = im.size
            raw = im.tobytes()
        return w, h, [raw[y * w * 3:(y + 1) * w * 3] for y in range(h)]
    return read_png_plain(path)


def write_png(path, width, rows):
    """An RGB PNG, every row unfiltered."""
    raw = b"".join(b"\x00" + r for r in rows)

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
    with open(path, "wb") as f:
        f.write(PNG_SIG + chunk(b"IHDR", struct.pack(">IIBBBBB", width, len(rows), 8, 2, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))


DIM = bytes(int(v * 0.3) for v in range(256))


def compare(a, b, fuzz):
    """Pixels that differ, their bounding box, and the diff picture's rows (None when the sizes differ)."""
    wa, ha, ra = a
    wb, hb, rb = b
    if (wa, ha) != (wb, hb):
        return wa * ha, None, None
    limit = int(round(fuzz * 255 / 100))
    count, box, out = 0, None, []
    for y in range(hb):
        x0, x1 = ra[y], rb[y]
        if x0 == x1:
            out.append(x1.translate(DIM))
            continue
        row = bytearray(x1.translate(DIM))
        hit = False
        for i in range(0, len(x1), 3):
            if x0[i:i + 3] == x1[i:i + 3]:
                continue
            if max(abs(x0[i] - x1[i]), abs(x0[i + 1] - x1[i + 1]), abs(x0[i + 2] - x1[i + 2])) > limit:
                row[i:i + 3] = b"\xff\x20\x40"
                count += 1
                hit = True
                x = i // 3
                box = [x, y, x, y] if box is None else [min(box[0], x), min(box[1], y), max(box[2], x), y]
        out.append(bytes(row) if hit else x1.translate(DIM))
    return count, box, out


def pictures(folder):
    found = {}
    for root, _, files in sorted(os.walk(folder)):
        for f in sorted(files):
            if f.lower().endswith(".png") and f not in found:
                found[f] = os.path.join(root, f)
    return found


def layout_lines(folder):
    """The first line of each layout-<part>-<shard>.txt (tools/ci-shots.sh's layout checks)."""
    out = []
    for root, _, files in sorted(os.walk(folder)):
        for f in sorted(files):
            if re.match(r"layout-.*\.txt$", f):
                with open(os.path.join(root, f), encoding="utf-8", errors="replace") as fh:
                    out.append(fh.readline().strip())
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("before")
    ap.add_argument("after")
    ap.add_argument("out")
    ap.add_argument("--summary", default="")
    ap.add_argument("--before-name", default="the last green build")
    ap.add_argument("--fuzz", type=float, default=2.0)
    ap.add_argument("--min-pixels", type=int, default=8)
    o = ap.parse_args()

    before = pictures(o.before) if os.path.isdir(o.before) else {}
    after = pictures(o.after) if os.path.isdir(o.after) else {}
    os.makedirs(o.out, exist_ok=True)
    for sub in ("before", "after", "diff"):
        os.makedirs(os.path.join(o.out, sub), exist_ok=True)
    unchanged, changed, unreadable = [], [], []
    for name in sorted(set(before) & set(after)):
        pb, pa = before[name], after[name]
        if open(pb, "rb").read() == open(pa, "rb").read():
            unchanged.append(name)
            continue
        try:
            a, b = read_png(pb), read_png(pa)
        except Exception as e:  # a picture it can't read is listed, never a failure
            unreadable.append((name, str(e)))
            shutil.copy(pb, os.path.join(o.out, "before", name))
            shutil.copy(pa, os.path.join(o.out, "after", name))
            continue
        count, box, rows = compare(a, b, o.fuzz)
        if rows is not None and count <= o.min_pixels:
            unchanged.append(name)
            continue
        shutil.copy(pb, os.path.join(o.out, "before", name))
        shutil.copy(pa, os.path.join(o.out, "after", name))
        if rows is not None:
            write_png(os.path.join(o.out, "diff", name), b[0], rows)
        total = b[0] * b[1]
        how = ("%d pixels (%.2f%%), in %d,%d to %d,%d" % (count, 100.0 * count / total, *box) if rows is not None
               else "size %dx%d to %dx%d" % (a[0], a[1], b[0], b[1]))
        changed.append((name, count / total if rows is not None else 1.0, how, rows is not None))
    added = sorted(set(after) - set(before))
    removed = sorted(set(before) - set(after))
    for name in added:
        shutil.copy(after[name], os.path.join(o.out, "after", name))
    for name in removed:
        shutil.copy(before[name], os.path.join(o.out, "before", name))
    changed.sort(key=lambda c: -c[1])

    # The page: before | after | diff for each changed picture, then the new and the removed ones.
    esc = html.escape
    cards = []
    for name, _, how, has_diff in changed:
        cells = ['<figure><figcaption>Before</figcaption><img loading="lazy" src="before/%s"></figure>' % esc(name),
                 '<figure><figcaption>After</figcaption><img loading="lazy" src="after/%s"></figure>' % esc(name)]
        if has_diff:
            cells.append('<figure><figcaption>Diff (red: changed)</figcaption><img loading="lazy" src="diff/%s"></figure>' % esc(name))
        cards.append('<section id="%s"><h2>%s</h2><p>%s</p><div class="row">%s</div></section>' % (esc(name), esc(name), esc(how), "".join(cells)))
    for name, why in unreadable:
        cards.append('<section><h2>%s</h2><p>Not compared: %s</p><div class="row"><figure><figcaption>Before</figcaption><img loading="lazy" src="before/%s"></figure>'
                     '<figure><figcaption>After</figcaption><img loading="lazy" src="after/%s"></figure></div></section>' % (esc(name), esc(why), esc(name), esc(name)))
    for name in added:
        cards.append('<section><h2>New: %s</h2><div class="row"><figure><figcaption>After</figcaption><img loading="lazy" src="after/%s"></figure></div></section>' % (esc(name), esc(name)))
    for name in removed:
        cards.append('<section><h2>Removed: %s</h2><div class="row"><figure><figcaption>Before</figcaption><img loading="lazy" src="before/%s"></figure></div></section>' % (esc(name), esc(name)))
    counts = "%d unchanged, %d changed, %d new, %d removed%s" % (len(unchanged), len(changed), len(added), len(removed),
                                                                ", %d not compared" % len(unreadable) if unreadable else "")
    index = ["<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
             "<title>Screenshot diff</title><style>",
             ":root{--bg:#14110d;--fg:#eee4d3;--muted:#a89a84;--line:#3a3225}",
             "body{margin:0;padding:16px;background:var(--bg);color:var(--fg);font:15px/1.4 system-ui,sans-serif}",
             "h1{font-size:20px}h2{font-size:16px;margin:24px 0 4px}p{color:var(--muted);margin:0 0 8px}",
             ".row{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:8px}",
             "figure{margin:0}figcaption{color:var(--muted);font-size:13px}img{width:100%;height:auto;border:1px solid var(--line)}",
             "a{color:#e7b35a}ul{columns:3 240px}</style></head><body>",
             "<h1>Pictures against %s</h1><p>%s (a pixel differs past %g%%; a picture changed past %d such pixels).</p>" % (esc(o.before_name), counts, o.fuzz, o.min_pixels)]
    if changed:
        index.append("<ul>" + "".join('<li><a href="#%s">%s</a> %s</li>' % (esc(n), esc(n), esc(h)) for n, _, h, _ in changed) + "</ul>")
    index.append("".join(cards) or "<p>No changes.</p>")
    index.append("</body></html>")
    with open(os.path.join(o.out, "index.html"), "w", encoding="utf-8") as f:
        f.write("\n".join(index))

    md = ["## Pictures against %s" % o.before_name, "",
          "| Unchanged | Changed | New | Removed |", "|---|---|---|---|",
          "| %d | %d | %d | %d |" % (len(unchanged), len(changed), len(added), len(removed)), ""]
    if changed:
        md.append("Changed (most first; before, after and diff in the screenshot-diff artifact):")
        md += ["- `%s`: %s" % (n, h) for n, _, h, _ in changed[:60]]
        if len(changed) > 60:
            md.append("- and %d more" % (len(changed) - 60))
        md.append("")
    if added:
        md += ["New: " + ", ".join("`%s`" % n for n in added), ""]
    if removed:
        md += ["Removed: " + ", ".join("`%s`" % n for n in removed), ""]
    if unreadable:
        md += ["Not compared: " + ", ".join("`%s` (%s)" % u for u in unreadable), ""]
    lay = layout_lines(o.after)
    if lay:
        md += ["### Layout checks (report only)", ""] + ["- " + line for line in lay] + [""]
    text = "\n".join(md) + "\n"
    with open(os.path.join(o.out, "summary.md"), "w", encoding="utf-8") as f:
        f.write(text)
    if o.summary:
        with open(o.summary, "a", encoding="utf-8") as f:
            f.write(text)
    print(counts)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:  # the comparison never fails the build
        print("::warning title=Screenshot diff::%s" % e)
        sys.exit(0)
