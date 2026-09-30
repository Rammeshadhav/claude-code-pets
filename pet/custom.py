#!/usr/bin/env python3
"""Image pets: your own artwork as a pet.

  custom.py add NAME IMAGE [--meter WORD]
  custom.py remove NAME
  custom.py list

The image is prepared once: shrunk, its plain background removed (flood fill
from the edges, with softened edges) or, if the background is busy, kept whole
to be shown as a rounded portrait card. The result is saved to
~/.claude/pet/custom/NAME.png with NAME.json alongside. Images stay on the
user's machine; nothing is bundled with the pet.
"""
import json
import os
import re
import sys
from collections import deque

import gi

gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf, GLib  # noqa: E402

DIR = os.path.expanduser("~/.claude/pet/custom")
MAX_H = 480            # prepared images are at most this tall: crisp at every pet size
TOLERANCE = 60         # how far (sum of RGB differences) from the background a pixel may be and still be cleared
PLAIN_SHARE = 0.9      # a background is plain when this share of the border is within
PLAIN_TOLERANCE = 30   # PLAIN_TOLERANCE of its average (white backdrop ~95%, painted scene ~40%)
BUILTIN = {"blob", "cat", "crab", "ghost", "crimson", "ripple", "spiral", "miti", "flash", "lavender"}


def valid_name(name):
    return bool(re.fullmatch(r"[a-z0-9-]{1,32}", name or ""))


def names():
    try:
        files = os.listdir(DIR)
    except FileNotFoundError:
        return []
    return sorted(f[:-5] for f in files
                  if f.endswith(".json") and os.path.exists(os.path.join(DIR, f[:-5] + ".png")))


def load(name):
    """(Pixbuf, meta) for an image pet, or None."""
    if not valid_name(name):
        return None
    try:
        with open(os.path.join(DIR, name + ".json")) as f:
            meta = json.load(f)
        return GdkPixbuf.Pixbuf.new_from_file(os.path.join(DIR, name + ".png")), meta
    except (OSError, ValueError, GLib.Error):
        return None


class Pixels:
    """Editable RGBA pixels of a pixbuf."""

    def __init__(self, pb):
        pb = pb.add_alpha(False, 0, 0, 0)
        self.w, self.h = pb.get_width(), pb.get_height()
        rs, raw = pb.get_rowstride(), pb.get_pixels()
        self.data = bytearray()
        for y in range(self.h):
            self.data += raw[y * rs:y * rs + self.w * 4]

    def px(self, x, y):
        i = (y * self.w + x) * 4
        return self.data[i:i + 4]

    def border(self):
        w, h = self.w, self.h
        return ([(x, 0) for x in range(w)] + [(x, h - 1) for x in range(w)]
                + [(0, y) for y in range(1, h - 1)] + [(w - 1, y) for y in range(1, h - 1)])

    def to_pixbuf(self):
        return GdkPixbuf.Pixbuf.new_from_bytes(GLib.Bytes.new(bytes(self.data)), GdkPixbuf.Colorspace.RGB,
                                               True, 8, self.w, self.h, self.w * 4)


def _dist(c, bg):
    return abs(c[0] - bg[0]) + abs(c[1] - bg[1]) + abs(c[2] - bg[2])


def prepare(src):
    """Shrink, then remove a plain background (or decide on a card), then trim."""
    w, h = src.get_width(), src.get_height()
    if w < 8 or h < 8:
        raise ValueError("image is too small")
    if h > MAX_H:
        src = src.scale_simple(max(1, round(w * MAX_H / h)), MAX_H, GdkPixbuf.InterpType.HYPER)
    p = Pixels(src)
    border = p.border()
    n = len(border)
    transparent = sum(p.px(x, y)[3] < 30 for x, y in border) / n
    if transparent < 0.6:
        bg = [sum(p.px(x, y)[k] for x, y in border) / n for k in range(3)]
        plain = sum(_dist(p.px(x, y), bg) < PLAIN_TOLERANCE for x, y in border) / n
        if plain < PLAIN_SHARE:
            return src, "card"  # busy background: show the whole picture
        _flood_clear(p, border, bg)
    return _trim(p), "cutout"


def _flood_clear(p, border, bg):
    """Clear background pixels connected to the edges, then soften the cut edge."""
    w, h, data = p.w, p.h, p.data
    cleared = bytearray(w * h)
    queue = deque()
    for x, y in border:
        if not cleared[y * w + x] and _dist(p.px(x, y), bg) < TOLERANCE:
            cleared[y * w + x] = 1
            queue.append((x, y))
    while queue:
        x, y = queue.popleft()
        data[(y * w + x) * 4 + 3] = 0
        for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
            if 0 <= nx < w and 0 <= ny < h:
                i = ny * w + nx
                if not cleared[i] and _dist(data[i * 4:i * 4 + 3], bg) < TOLERANCE:
                    cleared[i] = 1
                    queue.append((nx, ny))
    # anti-aliased edge: pixels next to the cleared area fade by how close they are to the background
    for y in range(h):
        for x in range(w):
            i = y * w + x
            if cleared[i]:
                continue
            if ((x > 0 and cleared[i - 1]) or (x + 1 < w and cleared[i + 1])
                    or (y > 0 and cleared[i - w]) or (y + 1 < h and cleared[i + w])):
                keep = min(1.0, max(0.35, (_dist(data[i * 4:i * 4 + 3], bg) - TOLERANCE) / 90))
                data[i * 4 + 3] = int(data[i * 4 + 3] * keep)


def _trim(p):
    """Crop to the visible pixels, with a small margin."""
    xs, ys = [], []
    for y in range(p.h):
        for x in range(p.w):
            if p.data[(y * p.w + x) * 4 + 3] > 16:
                xs.append(x)
                ys.append(y)
    pb = p.to_pixbuf()
    if not xs:
        return pb
    x0, y0 = max(0, min(xs) - 2), max(0, min(ys) - 2)
    x1, y1 = min(p.w - 1, max(xs) + 2), min(p.h - 1, max(ys) + 2)
    return pb.new_subpixbuf(x0, y0, x1 - x0 + 1, y1 - y0 + 1).copy()


def accent(pb):
    """The artwork's most characteristic colour: an average weighted toward
    saturated pixels, brightened so it reads on the dark usage box."""
    p = Pixels(pb)
    acc, total = [0.0, 0.0, 0.0], 0.0
    for y in range(0, p.h, 2):
        for x in range(0, p.w, 2):
            c = p.px(x, y)
            if c[3] < 200:
                continue
            mx, mn = max(c[:3]), min(c[:3])
            sat = (mx - mn) / mx if mx else 0
            weight = sat * sat * (mx / 255)
            for k in range(3):
                acc[k] += c[k] * weight
            total += weight
    if total < 1e-6:
        return [0.55, 0.85, 0.45]
    col = [v / total / 255 for v in acc]
    peak = max(max(col), 1e-6)
    return [min(1.0, max(0.2, v / peak * 0.95)) for v in col]


def add(name, image, meter="Energy"):
    pb, mode = prepare(GdkPixbuf.Pixbuf.new_from_file(image))
    meta = {"mode": mode, "accent": accent(pb), "meter": meter}
    os.makedirs(DIR, exist_ok=True)
    pb.savev(os.path.join(DIR, name + ".png"), "png", [], [])
    with open(os.path.join(DIR, name + ".json"), "w") as f:
        json.dump(meta, f)
    return meta


def main(argv):
    if len(argv) >= 3 and argv[0] == "add":
        name, image = argv[1], argv[2]
        meter = argv[argv.index("--meter") + 1] if "--meter" in argv[:-1] else "Energy"
        if not valid_name(name):
            print("pet names use lowercase letters, digits and '-' (up to 32)")
            return 1
        if name in BUILTIN:
            print(f"'{name}' is a built-in pet; pick another name")
            return 1
        try:
            meta = add(name, image, meter)
        except (GLib.Error, ValueError, OSError) as e:
            print(f"could not add '{name}': {e}")
            return 1
        how = "busy background, shown as a portrait card" if meta["mode"] == "card" else "background removed"
        print(f"added '{name}' ({how})")
        return 0
    if len(argv) == 2 and argv[0] == "remove":
        gone = False
        for ext in (".png", ".json"):
            try:
                os.remove(os.path.join(DIR, argv[1] + ext))
                gone = True
            except OSError:
                pass
        print(f"removed '{argv[1]}'" if gone else f"no image pet called '{argv[1]}'")
        return 0 if gone else 1
    if argv[:1] == ["list"]:
        print(" ".join(names()))
        return 0
    print("usage: custom.py add NAME IMAGE [--meter WORD] | remove NAME | list")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
