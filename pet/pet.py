#!/usr/bin/env python3
"""Claude Code desktop pet — a floating companion that shows what Claude is doing.

  thought bubble:  ... = working   red clock = needs your approval/input
                   green check = finished   zzz = idle
  left-drag: move   left-click: acknowledge "done"   right-click: menu
State comes from ~/.claude/pet/sessions/*.json, written by hook.py.

Efficiency: session files are watched with inotify (Gio.FileMonitor), so state
changes show up instantly without polling, and the frame rate adapts to the
state — smooth while Claude works, nearly idle when the pet is asleep.
"""
import fcntl
import json
import math
import os
import sys
import time

# Wayland/GNOME ignores keep-above and window positioning for native clients;
# XWayland honors both.
if os.environ.get("DISPLAY") and not os.environ.get("CLAUDE_PET_NATIVE"):
    os.environ["GDK_BACKEND"] = "x11"

import gi  # noqa: E402

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("Pango", "1.0")
gi.require_version("PangoCairo", "1.0")
from gi.repository import Gdk, Gio, GLib, Gtk, Pango, PangoCairo  # noqa: E402

import cairo  # noqa: E402

import custom  # noqa: E402
import vector_pets  # noqa: E402

PET_DIR = os.path.expanduser("~/.claude/pet")
SESS_DIR = os.path.join(PET_DIR, "sessions")
CONFIG = os.path.join(PET_DIR, "config.json")

PX = 7            # size of one sprite pixel
W, H = 190, 180   # drawing canvas; the window is this times the scale
DEFAULT_SCALE = 0.75
SIZES = {"Small": 0.6, "Medium": 0.75, "Large": 1.0}
TICKS_PER_SEC = 1000 / 120  # animation speeds below are tuned in these ticks
FPS = {"working": 24, "waiting": 24, "done": 12, "idle": 12, "asleep": 2}
DEBUG = bool(os.environ.get("CLAUDE_PET_DEBUG"))
FALLBACK_POLL_S = 5         # catches closed terminals/stale sessions; inotify handles the rest

# session list shown under the pet when several Claude sessions are open
LIST_W = 270                # canvas width while the list is shown
LIST_TOP = H - 26           # list starts where the single label would be
ROW_H = 21
LIST_PAD = 6
MAX_ROWS = 6

# usage box: plan limits recorded by statusline.py
USAGE = os.path.join(PET_DIR, "usage.json")
USAGE_ROW = 19
USAGE_PAD = 6
USAGE_WINDOWS = (("five_hour", "5h"), ("seven_day", "7d"))
# what the meter is called, and its colour, per pet
USAGE_THEME = {
    "crimson": ("Chakra", (0.95, 0.25, 0.25)), "ripple": ("Chakra", (0.68, 0.5, 0.98)),
    "spiral": ("Chakra", (0.35, 0.68, 1.0)), "flash": ("Chakra", (1.0, 0.85, 0.3)),
    "lavender": ("Chakra", (0.78, 0.68, 0.98)), "miti": ("Battery", (0.36, 0.72, 1.0)),
    "blob": ("Energy", (0.55, 0.88, 0.45)), "cat": ("Energy", (0.98, 0.7, 0.36)),
    "crab": ("Energy", (0.95, 0.5, 0.4)), "ghost": ("Energy", (0.82, 0.78, 0.98)),
}
STALE_WORKING = 15 * 60   # a "working" session silent this long is treated as idle
STALE_SESSION = 12 * 3600  # forget session files older than this
SLEEP_AFTER = 5 * 60       # idle this long -> pet falls asleep

# D = outline, B = body, E = eye, P = blush/accent, . = transparent
SPRITES = {
    "blob": [
        "....DDDD....",
        "..DDBBBBDD..",
        ".DBBBBBBBBD.",
        "DBBEBBBBEBBD",
        "DBBEBBBBEBBD",
        "DBPBBBBBBPBD",
        "DBBBBDDBBBBD",
        "DBBBBBBBBBBD",
        ".DBBBBBBBBD.",
        "..DDDDDDDD..",
    ],
    "cat": [
        ".D........D.",
        "DBD......DBD",
        "DBBDDDDDDBBD",
        "DBBBBBBBBBBD",
        "DBEBBBBBBEBD",
        "DBEBBPPBBEBD",
        "DBBBBDDBBBBD",
        ".DBBBBBBBBD.",
        ".DBDBBBBDBD.",
        ".DD.DDDD.DD.",
    ],
    "crab": [
        "DD........DD",
        "DBD......DBD",
        ".DBD.DD.DBD.",
        "..DDBBBBDD..",
        ".DBBEBBEBBD.",
        "DBBBEBBEBBBD",
        "DBPBBBBBBPBD",
        ".DBBBBBBBBD.",
        "..DBD..DBD..",
        "..DD....DD..",
    ],
    "ghost": [
        "...DDDDDD...",
        "..DBBBBBBD..",
        ".DBBBBBBBBD.",
        ".DBEBBBBEBD.",
        ".DBEBBBBEBD.",
        ".DBPBDDBPBD.",
        ".DBBBBBBBBD.",
        ".DBBBBBBBBD.",
        ".DBBDBBDBBD.",
        "..D..D..D...",
    ],
}

PALETTES = {  # body, outline, accent
    "blob": ((0.55, 0.85, 0.45), (0.13, 0.28, 0.12), (1.0, 0.60, 0.65)),
    "cat": ((0.98, 0.70, 0.36), (0.33, 0.18, 0.06), (1.0, 0.55, 0.60)),
    "crab": ((0.90, 0.47, 0.36), (0.35, 0.11, 0.07), (1.0, 0.78, 0.60)),
    "ghost": ((0.82, 0.78, 0.98), (0.25, 0.20, 0.42), (1.0, 0.62, 0.78)),
}

VECTOR_PETS = list(vector_pets.PETS)
SPECIES = list(SPRITES) + VECTOR_PETS


def all_species():
    """Built-in pets plus the user's image pets."""
    return SPECIES + [n for n in custom.names() if n not in SPECIES]

FONT = "Ubuntu Sans, Lato, Open Sans, Noto Sans, Sans"

# state -> (status-dot / accent colour, text colour)
STATE_COLORS = {
    "working": ((0.36, 0.62, 1.00), (0.62, 0.78, 1.00)),
    "waiting": ((1.00, 0.33, 0.33), (1.00, 0.62, 0.60)),
    "done": ((0.25, 0.82, 0.45), (0.58, 0.92, 0.66)),
    "idle": ((0.62, 0.62, 0.70), (0.78, 0.78, 0.84)),
}

# friendly verbs for what a tool does
TOOL_VERBS = {
    "Bash": "running", "Read": "reading", "Edit": "editing",
    "MultiEdit": "editing", "Write": "writing", "NotebookEdit": "editing",
    "Grep": "searching", "Glob": "searching", "WebFetch": "browsing",
    "WebSearch": "searching", "Agent": "delegating", "Task": "delegating",
    "TodoWrite": "planning", "Skill": "using a skill",
}


def status_phrase(state, detail, asleep, t):
    """Short human-readable status. Pass t=None to get the widest form
    (three dots) for measuring, so the label doesn't jitter as dots animate."""
    dots = "..." if t is None else "." * (1 + int(t // 4) % 3)
    if state == "working":
        if detail in ("", "thinking"):
            return "thinking" + dots
        tool = detail.split("__")[-1]  # mcp__server__tool -> tool
        if len(tool) > 12:
            tool = tool[:11] + "…"
        return TOOL_VERBS.get(detail, f"using {tool}") + dots
    if state == "waiting":
        return "needs you!"
    if state == "done":
        return "all done"
    return "sleeping" if asleep else "ready"


_FONTS = {}


def layout(cr, markup, size, weight="normal"):
    desc = _FONTS.get((size, weight))
    if desc is None:
        desc = Pango.FontDescription.from_string(FONT)
        desc.set_absolute_size(size * Pango.SCALE)
        desc.set_weight(getattr(Pango.Weight, weight.upper()))
        _FONTS[(size, weight)] = desc
    lay = PangoCairo.create_layout(cr)
    lay.set_font_description(desc)
    lay.set_markup(markup, -1)
    return lay


def hexcolor(rgb):
    return "#%02x%02x%02x" % tuple(int(c * 255) for c in rgb)


def display_name(s):
    """What to call a session: its title, else the latest request, else its folder."""
    return s.get("title") or s.get("task") or s.get("project") or "claude"


def rounded_rect(cr, x, y, w, h, r):
    cr.new_sub_path()
    cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
    cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
    cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
    cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
    cr.close_path()


PRIORITY = {"waiting": 3, "working": 2, "done": 1, "idle": 0}


RENAMED = {"sharingan": "crimson", "rinnegan": "ripple", "rasengan": "spiral",  # pets renamed in 0.2
           "minato": "flash", "hinata": "lavender", "pokeball": "blob"}


def load_config():
    try:
        with open(CONFIG) as f:
            cfg = json.load(f)
    except Exception:
        return {}
    if cfg.get("species") in RENAMED:
        cfg["species"] = RENAMED[cfg["species"]]
    return cfg


def save_config(cfg):
    tmp = CONFIG + ".tmp"
    with open(tmp, "w") as f:
        json.dump(cfg, f)
    os.replace(tmp, CONFIG)


def read_usage():
    """Remaining % and reset time per window; a window past its reset is full again."""
    try:
        with open(USAGE) as f:
            data = json.load(f)
    except Exception:
        return []
    now = time.time()
    rows = []
    for key, label in USAGE_WINDOWS:
        win = data.get(key)
        if not isinstance(win, dict):
            continue
        resets = float(win.get("resets_at") or 0)
        used = 0.0 if resets and now >= resets else float(win.get("used_percentage") or 0)
        rows.append((label, max(0.0, min(100.0, 100 - used)), resets))
    return rows


def until(ts):
    """'1h 20m', '12m', or a weekday + time for resets more than a day away."""
    secs = int(ts - time.time())
    if secs <= 0:
        return "now"
    if secs >= 86400:
        return time.strftime("%a %H:%M", time.localtime(ts))
    h, m = divmod(secs // 60, 60)
    return f"{h}h {m:02d}m" if h else f"{max(m, 1)}m"


def pid_alive(pid):
    try:
        os.kill(int(pid), 0)
    except ProcessLookupError:
        return False
    except (PermissionError, ValueError, TypeError, OverflowError):
        return True
    return True


def read_sessions():
    """All live sessions, most urgent first."""
    now = time.time()
    out = []
    try:
        names = os.listdir(SESS_DIR)
    except FileNotFoundError:
        return out
    for name in names:
        if not name.endswith(".json"):
            continue
        path = os.path.join(SESS_DIR, name)
        try:
            with open(path) as f:
                rec = json.load(f)
        except Exception:
            continue
        age = now - rec.get("ts", 0)
        # terminal closed or Claude killed without a SessionEnd hook
        if age > STALE_SESSION or (rec.get("pid") and not pid_alive(rec["pid"])):
            try:
                os.remove(path)
            except OSError:
                pass
            continue
        if rec.get("state") == "working" and age > STALE_WORKING:
            rec["state"] = "idle"
        rec["path"] = path
        out.append(rec)
    out.sort(key=lambda r: (-PRIORITY.get(r.get("state"), 0), -r.get("ts", 0)))
    return out


class Pet(Gtk.Window):
    def __init__(self):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.cfg = load_config()
        self.species = self.cfg.get("species", "blob")
        if self.species not in all_species():
            self.species = "blob"
        try:
            self.scale = float(self.cfg.get("scale", DEFAULT_SCALE))
        except (TypeError, ValueError):
            self.scale = DEFAULT_SCALE
        self.t0 = time.monotonic()
        self.t = 0.0
        self.next_blink = 37
        self.poll_pending = False
        self.state, self.detail, self.project, self.count = "idle", "", "", 0
        self.focus_path = None
        self.sessions = []
        self.list_mode = False
        self.row_rects = []
        self.layers = {}
        self.usage = []
        self.usage_top = H
        self.custom = None  # (name, pixbuf, meta) of the current image pet
        self.name = ""
        self.cw, self.ch = W, H  # canvas size (unscaled)
        self.last_active = time.time()
        self.blink_until = 0
        self.press_pos = None

        self.set_title("Claude Pet")
        self.set_decorated(False)
        self.set_resizable(False)
        self.set_keep_above(True)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)
        self.set_accept_focus(False)
        self.set_type_hint(Gdk.WindowTypeHint.UTILITY)
        self.stick()
        self.set_app_paintable(True)
        visual = self.get_screen().get_rgba_visual()
        if visual is not None:
            self.set_visual(visual)

        area = Gtk.DrawingArea()
        area.connect("draw", self.on_draw)
        self.add(area)
        self.area = area
        self.apply_size()

        self.add_events(Gdk.EventMask.BUTTON_PRESS_MASK | Gdk.EventMask.BUTTON_RELEASE_MASK
                        | Gdk.EventMask.POINTER_MOTION_MASK)
        self.connect("button-press-event", self.on_press)
        self.connect("motion-notify-event", self.on_motion)
        self.connect("button-release-event", self.on_release)
        self.connect("configure-event", self.on_configure)
        self.connect("destroy", Gtk.main_quit)

        pos = self.cfg.get("pos")
        if pos:
            self.move(*pos)
        else:
            geo = Gdk.Display.get_default().get_monitor(0).get_workarea()
            w, h = self.win_size()
            self.move(geo.x + geo.width - w - 24, geo.y + geo.height - h - 24)

        # react to hook writes immediately instead of polling
        self.monitor = Gio.File.new_for_path(SESS_DIR).monitor_directory(Gio.FileMonitorFlags.NONE, None)
        self.monitor.connect("changed", self.on_sessions_changed)
        GLib.timeout_add_seconds(FALLBACK_POLL_S, self.poll)
        self.poll()
        self.schedule_frame()

    def win_size(self):
        return int(self.cw * self.scale + 0.5), int(self.ch * self.scale + 0.5)

    def apply_size(self):
        """Resize the window to the canvas, keeping the pet where it was and on screen."""
        w, h = self.win_size()
        win = self.get_window()
        if win is not None and self.get_visible():
            x, y = self.get_position()
            old_w, _ = self.get_size()
            x += (old_w - w) // 2  # keep the pet horizontally in place
            display = Gdk.Display.get_default()
            geo = display.get_monitor_at_window(win).get_workarea()
            x = max(geo.x, min(x, geo.x + geo.width - w))
            y = max(geo.y, min(y, geo.y + geo.height - h))
            self.move(x, y)
        self.area.set_size_request(w, h)
        self.resize(w, h)

    def show_usage(self):
        return self.cfg.get("show_usage", True)

    def update_layout(self):
        self.list_mode = self.cfg.get("show_list", True) and len(self.sessions) >= 2
        if self.list_mode:
            rows = min(len(self.sessions), MAX_ROWS)
            more = 16 if len(self.sessions) > MAX_ROWS else 0
            size = (LIST_W, LIST_TOP + LIST_PAD * 2 + rows * ROW_H + more + 4)
        else:
            size = (W, H)
        if self.show_usage():
            self.usage_top = size[1] + 2
            rows = max(1, len(self.usage))  # one hint row until data arrives
            size = (LIST_W, self.usage_top + USAGE_PAD * 2 + rows * USAGE_ROW + 2)
        if size != (self.cw, self.ch):
            self.cw, self.ch = size
            self.apply_size()

    # ---- state -------------------------------------------------------
    def on_sessions_changed(self, *_):
        # a hook write fires several events; coalesce them into one read
        if not self.poll_pending:
            self.poll_pending = True
            GLib.idle_add(self.poll_once)

    def poll_once(self):
        self.poll()
        return False  # one-shot: returning True would re-run it forever

    def poll(self):
        self.poll_pending = False
        sessions = self.sessions = read_sessions()
        self.usage = read_usage()
        active = [s for s in sessions if s.get("state") in ("working", "waiting")]
        self.count = len(active)
        self.update_layout()
        if sessions:
            top = sessions[0]
            self.state = top.get("state", "idle")
            self.detail = top.get("detail", "")
            self.project = top.get("project", "")
            self.name = display_name(top)
            self.focus_path = top["path"]
        else:
            self.state, self.detail, self.project, self.focus_path = "idle", "", "", None
            self.name = ""
        if self.state != "idle":
            self.last_active = time.time()
        if DEBUG:
            print(f"{time.time():.3f} poll: {self.state} {self.detail} ({len(sessions)} sessions)",
                  file=sys.stderr, flush=True)
        self.area.queue_draw()
        return True

    def acknowledge(self, only_path=None):
        """Clear 'done' sessions back to idle: all of them, or just one row's."""
        for s in read_sessions():
            if s.get("state") == "done" and only_path in (None, s["path"]):
                rec = {k: v for k, v in s.items() if k != "path"}
                rec.update(state="idle", detail="ready")
                try:
                    with open(s["path"], "w") as f:
                        json.dump(rec, f)
                except OSError:
                    pass
        self.poll()

    def asleep(self):
        return self.state == "idle" and time.time() - self.last_active > SLEEP_AFTER

    def schedule_frame(self):
        fps = FPS["asleep"] if self.asleep() else FPS.get(self.state, 12)
        GLib.timeout_add(int(1000 / fps), self.tick)

    def tick(self):
        # time-based animation: speed is the same whatever the frame rate
        self.t = (time.monotonic() - self.t0) * TICKS_PER_SEC
        if self.t >= self.next_blink:
            self.next_blink = self.t + 37
            if self.state != "working":
                self.blink_until = self.t + 2
        self.area.queue_draw()
        self.schedule_frame()
        return False

    # ---- input -------------------------------------------------------
    def on_press(self, _w, ev):
        if ev.button == 3:
            self.show_menu(ev)
            return True
        if ev.button == 1:
            self.press_pos = (ev.x_root, ev.y_root)
            return True
        return False

    def on_motion(self, _w, ev):
        if self.press_pos and ev.state & Gdk.ModifierType.BUTTON1_MASK:
            dist = abs(ev.x_root - self.press_pos[0]) + abs(ev.y_root - self.press_pos[1])
            if dist > 4:
                self.press_pos = None
                self.begin_move_drag(1, int(ev.x_root), int(ev.y_root), ev.time)
        return False

    def on_release(self, _w, ev):
        if ev.button == 1 and self.press_pos:
            x, y = ev.x / self.scale, ev.y / self.scale
            row = next((r for r in self.row_rects
                        if r[0] <= x <= r[0] + r[2] and r[1] <= y <= r[1] + r[3]), None)
            self.acknowledge(row[4] if row else None)
        self.press_pos = None
        return False

    def on_configure(self, _w, ev):
        pos = [ev.x, ev.y]
        if self.cfg.get("pos") != pos:
            self.cfg["pos"] = pos
            GLib.timeout_add(800, self.persist)
        return False

    def persist(self):
        save_config(self.cfg)
        return False

    def show_menu(self, ev):
        menu = Gtk.Menu()
        group = None
        for name in all_species():
            item = Gtk.RadioMenuItem.new_with_label_from_widget(group, name.capitalize())
            group = group or item
            item.set_active(name == self.species)
            item.connect("toggled", self.on_species, name)
            menu.append(item)
        menu.append(Gtk.SeparatorMenuItem())
        size = Gtk.MenuItem(label="Size")
        sub = Gtk.Menu()
        group = None
        for label, value in SIZES.items():
            item = Gtk.RadioMenuItem.new_with_label_from_widget(group, label)
            group = group or item
            item.set_active(abs(value - self.scale) < 0.01)
            item.connect("toggled", self.on_size, value)
            sub.append(item)
        size.set_submenu(sub)
        menu.append(size)
        show = Gtk.CheckMenuItem(label="Show all sessions")
        show.set_active(self.cfg.get("show_list", True))
        show.connect("toggled", self.on_show_list)
        menu.append(show)
        usage = Gtk.CheckMenuItem(label="Show usage limits")
        usage.set_active(self.show_usage())
        usage.connect("toggled", self.on_show_usage)
        menu.append(usage)
        clear = Gtk.MenuItem(label="Clear finished")
        clear.connect("activate", lambda *_: self.acknowledge())
        menu.append(clear)
        quit_ = Gtk.MenuItem(label="Put pet away")
        quit_.connect("activate", lambda *_: Gtk.main_quit())
        menu.append(quit_)
        menu.show_all()
        menu.popup_at_pointer(ev)

    def on_size(self, item, value):
        if item.get_active():
            self.scale = value
            self.cfg["scale"] = value
            save_config(self.cfg)
            self.apply_size()

    def on_show_list(self, item):
        self.cfg["show_list"] = item.get_active()
        save_config(self.cfg)
        self.update_layout()

    def on_show_usage(self, item):
        self.cfg["show_usage"] = item.get_active()
        save_config(self.cfg)
        self.update_layout()

    def on_species(self, item, name):
        if item.get_active():
            self.species = name
            self.cfg["species"] = name
            save_config(self.cfg)

    # ---- drawing -----------------------------------------------------
    def on_draw(self, _w, cr):
        cr.set_operator(0)  # CLEAR
        cr.paint()
        cr.set_operator(2)  # OVER
        cr.scale(self.scale, self.scale)
        cr.save()
        cr.translate((self.cw - W) / 2, 0)  # pet stays centred when the list widens the window
        self.draw_pet(cr)
        cr.restore()
        if self.list_mode:
            self.draw_list(cr, LIST_TOP)
        else:
            self.row_rects = []
        if self.show_usage():
            self.draw_usage(cr, self.usage_top)
        return False

    def draw_pet(self, cr):
        asleep = self.asleep()
        t = self.t
        if self.state == "working":
            bob = -abs(math.sin(t * 0.45)) * 6
        elif self.state == "waiting":
            bob = -abs(math.sin(t * 0.25)) * 14
        elif self.state == "done":
            bob = -abs(math.sin(t * 0.3)) * 4
        else:
            bob = math.sin(t * 0.08) * 2

        if self.custom_pet():
            self.draw_custom(cr, bob, asleep)
            if not self.list_mode:
                self.draw_label(cr)
            return

        if self.species in vector_pets.PETS:
            R = 44
            cx, cy = W / 2 - 14, H - R - 36 + bob * 0.5
            cr.save()
            cr.translate(cx, H - 28)
            cr.scale(1, 0.22)
            cr.arc(0, 0, R * 0.85, 0, 2 * math.pi)
            cr.restore()
            cr.set_source_rgba(0, 0, 0, 0.18)
            cr.fill()
            vector_pets.PETS[self.species](cr, cx, cy, R, t, self.state, asleep)
            self.draw_bubble(cr, cx + R * 0.8, cy - R * 0.85, asleep)
            if not self.list_mode:
                self.draw_label(cr)
            return

        sprite = SPRITES[self.species]
        sw, sh = len(sprite[0]) * PX, len(sprite) * PX
        ox = (W - sw) / 2 - 18
        oy = H - sh - 34 + bob

        # shadow
        cr.save()
        cr.translate(ox + sw / 2, H - 30)
        cr.scale(1, 0.25)
        cr.arc(0, 0, sw / 2.4 * (1 + bob / 60), 0, 2 * math.pi)
        cr.restore()
        cr.set_source_rgba(0, 0, 0, 0.18)
        cr.fill()

        self.draw_sprite(cr, sprite, ox, oy, eyes_closed=asleep or t < self.blink_until)
        self.draw_bubble(cr, ox + sw - 6, oy - 6, asleep)
        if not self.list_mode:
            self.draw_label(cr)

    def draw_sprite(self, cr, sprite, ox, oy, eyes_closed):
        body, dark, accent = PALETTES[self.species]
        for y, row in enumerate(sprite):
            for x, ch in enumerate(row):
                if ch == ".":
                    continue
                px, py = ox + x * PX, oy + y * PX
                if ch == "D":
                    cr.set_source_rgb(*dark)
                elif ch == "P":
                    cr.set_source_rgb(*accent)
                elif ch == "E":
                    if eyes_closed:
                        cr.set_source_rgb(*body)
                        cr.rectangle(px, py, PX, PX)
                        cr.fill()
                        below = y + 1 < len(sprite) and sprite[y + 1][x] == "E"
                        if not below:
                            cr.set_source_rgb(*dark)
                            cr.rectangle(px, py + PX / 3, PX, PX / 3)
                            cr.fill()
                        continue
                    cr.set_source_rgb(0.08, 0.08, 0.1)
                else:
                    cr.set_source_rgb(*body)
                cr.rectangle(px, py, PX + 0.5, PX + 0.5)
                cr.fill()

    def draw_bubble(self, cr, x, y, asleep):
        r = 20
        cx, cy = min(x + r, W - r - 4), max(y - r, r + 4)
        # tail puffs
        cr.set_source_rgba(1, 1, 1, 0.95)
        cr.arc(x + 2, y + 2, 3, 0, 2 * math.pi)
        cr.fill()
        cr.arc(x + 8, y - 6, 5, 0, 2 * math.pi)
        cr.fill()
        # bubble
        cr.arc(cx, cy, r, 0, 2 * math.pi)
        cr.set_source_rgba(1, 1, 1, 0.97)
        cr.fill_preserve()
        cr.set_source_rgba(0, 0, 0, 0.25)
        cr.set_line_width(1.5)
        cr.stroke()

        s = self.state
        if s == "working":
            for i in range(3):
                phase = (self.t * 0.35 - i * 0.8)
                lift = max(0, math.sin(phase)) * 4
                cr.arc(cx - 9 + i * 9, cy - lift, 3.2, 0, 2 * math.pi)
                cr.set_source_rgb(0.35, 0.35, 0.40)
                cr.fill()
        elif s == "waiting":
            pulse = 0.85 + 0.15 * math.sin(self.t * 0.5)
            cr.arc(cx, cy, 12 * pulse, 0, 2 * math.pi)
            cr.set_source_rgb(0.90, 0.22, 0.22)
            cr.fill()
            cr.set_source_rgb(1, 1, 1)
            cr.set_line_width(2.2)
            cr.set_line_cap(1)
            cr.move_to(cx, cy)
            cr.line_to(cx, cy - 7)
            cr.move_to(cx, cy)
            cr.line_to(cx + 5, cy + 2)
            cr.stroke()
        elif s == "done":
            cr.arc(cx, cy, 12, 0, 2 * math.pi)
            cr.set_source_rgb(0.20, 0.72, 0.35)
            cr.fill()
            cr.set_source_rgb(1, 1, 1)
            cr.set_line_width(2.8)
            cr.set_line_cap(1)
            cr.set_line_join(1)
            cr.move_to(cx - 6, cy)
            cr.line_to(cx - 2, cy + 5)
            cr.line_to(cx + 6, cy - 5)
            cr.stroke()
        else:
            cr.set_source_rgb(0.40, 0.40, 0.55)
            if asleep:
                for i, size in enumerate((9, 12, 15)):
                    lay = layout(cr, "z", size, "heavy")
                    dy = math.sin(self.t * 0.1 + i) * 1.5
                    cr.move_to(cx - 13 + i * 7, cy - 4 - i * 6 + dy)
                    PangoCairo.show_layout(cr, lay)
            else:
                # little music note, drawn so it never depends on fonts
                dy = math.sin(self.t * 0.12) * 2
                cr.save()
                cr.translate(cx - 2, cy + dy)
                cr.save()
                cr.translate(-3, 6)
                cr.scale(1.3, 1)
                cr.arc(0, 0, 3.4, 0, 2 * math.pi)
                cr.restore()
                cr.fill()
                cr.rectangle(0.8, -8, 1.8, 14)
                cr.fill()
                cr.move_to(2.6, -8)
                cr.curve_to(6, -6, 8, -3, 6, 1)
                cr.set_line_width(1.8)
                cr.stroke()
                cr.restore()

        if self.count > 1:
            bx, by = cx + r - 4, cy - r + 4
            cr.arc(bx, by, 8, 0, 2 * math.pi)
            cr.set_source_rgb(0.25, 0.45, 0.95)
            cr.fill()
            cr.set_source_rgb(1, 1, 1)
            lay = layout(cr, str(self.count), 10, "bold")
            _, log = lay.get_pixel_extents()
            cr.move_to(bx - log.width / 2, by - log.height / 2)
            PangoCairo.show_layout(cr, lay)

    # ---- text layers (cached) ----------------------------------------
    # Pango layout is the costly part of a frame, so text is rendered once into
    # an offscreen surface and re-used until the text itself changes. Only the
    # cheap animated shapes (status dots, glow) are drawn every frame.
    def cached_layer(self, name, key, top, height, render):
        cache = self.layers.get(name)
        if cache is None or cache[0] != key:
            s = self.scale
            oy = round(top * s)
            surf = cairo.ImageSurface(cairo.FORMAT_ARGB32,
                                      max(1, int(self.cw * s + 1)), max(1, int(height * s + 2)))
            c = cairo.Context(surf)
            c.translate(0, -oy)
            c.scale(s, s)
            info = render(c)
            cache = (key, surf, oy, info)
            self.layers[name] = cache
        return cache

    def blit(self, cr, cache):
        _, surf, oy, _ = cache
        cr.save()
        cr.identity_matrix()  # device pixels: keeps text crisp
        cr.set_source_surface(surf, 0, oy)
        cr.paint()
        cr.restore()

    def status_dot(self, cr, x, y, state, r=3.5):
        dot_rgb = STATE_COLORS.get(state, STATE_COLORS["idle"])[0]
        if state in ("working", "waiting"):
            halo = 0.5 + 0.5 * math.sin(self.t * (0.5 if state == "waiting" else 0.25))
            cr.arc(x, y, r + 2.4 * halo, 0, 2 * math.pi)
            cr.set_source_rgba(*dot_rgb, 0.35 * (1 - halo) + 0.1)
            cr.fill()
        cr.arc(x, y, r, 0, 2 * math.pi)
        cr.set_source_rgb(*dot_rgb)
        cr.fill()

    def draw_label(self, cr):
        """Glassy pill: pulsing status dot, session name, coloured status phrase."""
        if self.state == "idle" and not self.name:
            return
        asleep = self.asleep()
        phrase = status_phrase(self.state, self.detail, asleep, self.t)
        widest = status_phrase(self.state, self.detail, asleep, None)
        key = (self.scale, self.name, phrase, widest, self.state)
        ph = 22
        py = H - ph - 3
        ox = (self.cw - W) / 2

        def render(c):
            text_rgb = STATE_COLORS.get(self.state, STATE_COLORS["idle"])[1]
            esc = GLib.markup_escape_text
            max_text = W - 38
            status = layout(c, f"<span foreground='#ffffff' fgalpha='40%'>  ·  </span>"
                               f"<span weight='medium' foreground='{hexcolor(text_rgb)}'>{esc(phrase)}</span>", 11.5)
            status_w = layout(c, "  ·  " + esc(widest), 11.5, "medium").get_pixel_extents()[1].width
            name = layout(c, f"<span weight='bold' foreground='#ffffff'>{esc(self.name or 'claude')}</span>", 11.5)
            name.set_ellipsize(Pango.EllipsizeMode.END)
            name.set_width(max(30, max_text - status_w) * Pango.SCALE)
            name_w = name.get_pixel_extents()[1].width
            text_h = name.get_pixel_extents()[1].height
            pw = name_w + status_w + 30
            px = ox + (W - pw) / 2
            rounded_rect(c, px, py + 1.5, pw, ph, ph / 2)
            c.set_source_rgba(0, 0, 0, 0.25)
            c.fill()
            rounded_rect(c, px, py, pw, ph, ph / 2)
            c.set_source_rgba(0.09, 0.09, 0.12, 0.88)
            c.fill_preserve()
            c.set_source_rgba(1, 1, 1, 0.12)
            c.set_line_width(1)
            c.stroke()
            ty = py + (ph - text_h) / 2
            c.move_to(px + 21, ty)
            PangoCairo.show_layout(c, name)
            c.move_to(px + 21 + name_w, ty)
            PangoCairo.show_layout(c, status)
            return px

        cache = self.cached_layer("label", key, py - 2, ph + 6, render)
        self.blit(cr, cache)
        # draw_pet is translated by ox; the cache is in window coordinates
        self.status_dot(cr, cache[3] + 12 - ox, py + ph / 2, self.state)

    def draw_list(self, cr, top):
        """One row per Claude session: status dot, session name, folder, status."""
        rows = self.sessions[:MAX_ROWS]
        extra = len(self.sessions) - len(rows)
        x0, w = 6, self.cw - 12
        h = LIST_PAD * 2 + len(rows) * ROW_H + (16 if extra else 0)
        right = x0 + w - 9
        phrases = [status_phrase(s.get("state", "idle"), s.get("detail", ""), False, self.t) for s in rows]
        key = (self.scale, self.cw, extra, tuple(
            (display_name(s), s.get("project", ""), s.get("state"), p, s["path"]) for s, p in zip(rows, phrases)))

        def render(c):
            esc = GLib.markup_escape_text
            rounded_rect(c, x0, top + 1.5, w, h, 10)
            c.set_source_rgba(0, 0, 0, 0.25)
            c.fill()
            rounded_rect(c, x0, top, w, h, 10)
            c.set_source_rgba(0.09, 0.09, 0.12, 0.9)
            c.fill_preserve()
            c.set_source_rgba(1, 1, 1, 0.12)
            c.set_line_width(1)
            c.stroke()
            rects = []
            y = top + LIST_PAD
            for s, phrase in zip(rows, phrases):
                state = s.get("state", "idle")
                text_rgb = STATE_COLORS.get(state, STATE_COLORS["idle"])[1]
                mid = y + ROW_H / 2
                # status, right-aligned in a fixed slot so it doesn't jitter
                widest = status_phrase(state, s.get("detail", ""), False, None)
                slot = layout(c, esc(widest), 11, "medium").get_pixel_extents()[1].width
                status = layout(c, f"<span weight='medium' foreground='{hexcolor(text_rgb)}'>{esc(phrase)}</span>", 11)
                # session name, then its folder in grey if there's room
                room = right - slot - 8 - (x0 + 21)
                name = layout(c, f"<span weight='bold' foreground='#ffffff'>{esc(display_name(s))}</span>", 11)
                name.set_ellipsize(Pango.EllipsizeMode.END)
                name.set_width(int(room * 0.75) * Pango.SCALE)
                name_w = name.get_pixel_extents()[1].width
                text_h = name.get_pixel_extents()[1].height
                ty = mid - text_h / 2
                c.move_to(x0 + 21, ty)
                PangoCairo.show_layout(c, name)
                folder_w = room - name_w - 6
                if s.get("project") and folder_w > 24:
                    folder = layout(c, f"<span foreground='#ffffff' fgalpha='45%'>{esc(s['project'])}</span>", 10)
                    folder.set_ellipsize(Pango.EllipsizeMode.END)
                    folder.set_width(int(folder_w) * Pango.SCALE)
                    c.move_to(x0 + 21 + name_w + 6, mid - folder.get_pixel_extents()[1].height / 2)
                    PangoCairo.show_layout(c, folder)
                c.move_to(right - slot, ty)
                PangoCairo.show_layout(c, status)
                rects.append((x0, y, w, ROW_H, s["path"]))
                y += ROW_H
            if extra:
                more = layout(c, f"<span foreground='#ffffff' fgalpha='50%'>+{extra} more</span>", 10)
                mw = more.get_pixel_extents()[1].width
                c.move_to(x0 + (w - mw) / 2, y)
                PangoCairo.show_layout(c, more)
            return rects

        cache = self.cached_layer("list", key, top - 1, h + 4, render)
        self.row_rects = cache[3]
        self.blit(cr, cache)
        # animated parts on top: pulsing highlight for rows that need you, status dots
        for (rx, ry, rw, rh, _), s in zip(self.row_rects, rows):
            state = s.get("state", "idle")
            if state == "waiting":
                glow = 0.5 + 0.5 * math.sin(self.t * 0.5)
                rounded_rect(cr, rx + 3, ry + 1, rw - 6, rh - 2, 6)
                cr.set_source_rgba(1.0, 0.3, 0.3, 0.08 + 0.08 * glow)
                cr.fill()
            self.status_dot(cr, rx + 12, ry + rh / 2, state, 3.2)

    # ---- image pets ------------------------------------------------------
    def custom_pet(self):
        """(name, pixbuf, meta) if the current species is an image pet; loaded once."""
        if not self.custom or self.custom[0] != self.species:
            loaded = custom.load(self.species)
            self.custom = (self.species, *loaded) if loaded else None
        return self.custom

    def draw_custom(self, cr, bob, asleep):
        """Your own artwork, untouched; the state shows in its aura, motion and bubble."""
        _, pb, meta = self.custom
        t, state = self.t, self.state
        card = meta.get("mode") == "card"
        iw, ih = pb.get_width(), pb.get_height()
        s = (112 if card else 124) / ih
        if iw * s > 150:
            s = 150 / iw
        w, h = iw * s, ih * s
        waiting = state == "waiting" and not asleep
        shake = math.sin(t * 1.3) * 2.5 if waiting else 0
        breathe = 1 + 0.012 * math.sin(t * 0.08) if state == "idle" and not asleep else 1
        cx = W / 2 - 14 + shake
        x, y = cx - w / 2, H - 30 - h + bob

        cr.save()  # shadow
        cr.translate(W / 2 - 14, H - 28)
        cr.scale(1, 0.22)
        cr.arc(0, 0, w * 0.42, 0, 2 * math.pi)
        cr.restore()
        cr.set_source_rgba(0, 0, 0, 0.18)
        cr.fill()

        if not asleep:  # aura in the state's colour (the pet's own colour when idle)
            rgb, alpha = {
                "working": ((0.36, 0.62, 1.0), 0.28 + 0.12 * math.sin(t * 0.3)),
                "waiting": ((1.0, 0.3, 0.3), 0.45 + 0.2 * math.sin(t * 0.6)),
                "done": ((0.3, 0.9, 0.5), 0.4),
            }.get(state, (tuple(meta.get("accent", (0.6, 0.6, 0.7))), 0.16))
            gx, gy, gr = cx, y + h * 0.5, max(w, h) * 0.62
            g = cairo.RadialGradient(gx, gy, gr * 0.15, gx, gy, gr)
            g.add_color_stop_rgba(0, *rgb, alpha)
            g.add_color_stop_rgba(1, *rgb, 0)
            cr.set_source(g)
            cr.save()
            cr.translate(gx, gy)
            cr.scale(1, h / max(w, h) * 1.05)
            cr.arc(0, 0, gr, 0, 2 * math.pi)
            cr.restore()
            cr.fill()

        cr.save()  # the artwork, breathing gently around its feet
        cr.translate(x + w / 2, y + h)
        cr.scale(breathe, breathe)
        cr.translate(-w / 2, -h)
        if card:
            rounded_rect(cr, 0, 0, w, h, 10)
            cr.clip()
        cr.scale(s, s)
        Gdk.cairo_set_source_pixbuf(cr, pb, 0, 0)
        cr.get_source().set_filter(cairo.FILTER_GOOD)
        cr.paint_with_alpha(0.5 if asleep else 1.0)
        cr.restore()
        if card:
            bc, ba = {
                "waiting": ((1.0, 0.35, 0.35), 0.6 + 0.4 * math.sin(t * 0.6)),
                "working": ((0.45, 0.7, 1.0), 0.9),
                "done": ((0.35, 0.9, 0.5), 0.9),
            }.get(state if not asleep else "", ((1, 1, 1), 0.85))
            rounded_rect(cr, x, y, w, h, 10)
            cr.set_source_rgba(*bc, ba)
            cr.set_line_width(2.5)
            cr.stroke()

        if state == "done" and not asleep:  # finished: a few twinkling stars
            for k, (fx, fy) in enumerate(((0.1, 0.15), (0.9, 0.3), (0.2, 0.55))):
                ph = (t * 0.08 + k / 3) % 1
                vector_pets.star(cr, x + w * fx, y + h * fy - ph * 8, 3.5 * (1 - ph) + 1, 0)
                cr.set_source_rgba(1.0, 0.9, 0.35, 1 - ph)
                cr.fill()
        self.draw_bubble(cr, x + w - 6, y + 10, asleep)

    def draw_usage(self, cr, top):
        """Plan usage as themed meters: what's left in the 5-hour and weekly windows."""
        img = self.custom_pet()
        if img:
            term, accent = img[2].get("meter", "Energy"), tuple(img[2].get("accent", (0.55, 0.85, 0.45)))
        else:
            term, accent = USAGE_THEME.get(self.species, ("Energy", (0.55, 0.85, 0.45)))
        rows = self.usage
        x0, w = 6, self.cw - 12
        h = USAGE_PAD * 2 + max(1, len(rows)) * USAGE_ROW
        bar_x, bar_w = x0 + 94, 56
        texts = [(label, f"{left:.0f}%", until(resets) if resets else "") for label, left, resets in rows]
        key = (self.scale, self.cw, top, term, tuple(texts))

        def render(c):
            esc = GLib.markup_escape_text
            rounded_rect(c, x0, top + 1.5, w, h, 10)
            c.set_source_rgba(0, 0, 0, 0.25)
            c.fill()
            rounded_rect(c, x0, top, w, h, 10)
            c.set_source_rgba(0.09, 0.09, 0.12, 0.9)
            c.fill_preserve()
            c.set_source_rgba(1, 1, 1, 0.12)
            c.set_line_width(1)
            c.stroke()
            if not texts:
                hint = layout(c, "<span foreground='#ffffff' fgalpha='55%'>usage shows after Claude's next reply</span>", 10)
                hw, hh = hint.get_pixel_extents()[1].width, hint.get_pixel_extents()[1].height
                c.move_to(x0 + (w - hw) / 2, top + (h - hh) / 2)
                PangoCairo.show_layout(c, hint)
                return None
            y = top + USAGE_PAD
            for (label, pct, reset), (_, left, _) in zip(texts, rows):
                mid = y + USAGE_ROW / 2
                name = layout(c, f"<span weight='bold' foreground='#ffffff'>{esc(term)}</span>"
                                 f"<span foreground='#ffffff' fgalpha='55%'> · {label}</span>", 10.5)
                c.move_to(x0 + 24, mid - name.get_pixel_extents()[1].height / 2)
                PangoCairo.show_layout(c, name)
                col = hexcolor(self.usage_color(accent, left))
                p = layout(c, f"<span weight='bold' foreground='{col}'>{pct}</span>", 10.5)
                c.move_to(bar_x + bar_w + 6, mid - p.get_pixel_extents()[1].height / 2)
                PangoCairo.show_layout(c, p)
                if reset:
                    r = layout(c, f"<span foreground='#ffffff' fgalpha='60%'>{esc(reset)}</span>", 10)
                    rw, rh = r.get_pixel_extents()[1].width, r.get_pixel_extents()[1].height
                    c.move_to(x0 + w - 9 - rw, mid - rh / 2)
                    PangoCairo.show_layout(c, r)
                    self.refill_icon(c, x0 + w - 9 - rw - 7, mid)
                y += USAGE_ROW
            return None

        cache = self.cached_layer("usage", key, top - 1, h + 4, render)
        self.blit(cr, cache)
        # live parts: icons and meters (the meter pulses when nearly empty)
        y = top + USAGE_PAD
        for label, left, _ in rows:
            mid = y + USAGE_ROW / 2
            if label == "5h":
                self.bolt_icon(cr, x0 + 13, mid, accent)
            else:
                self.calendar_icon(cr, x0 + 13, mid, accent)
            self.meter(cr, bar_x, mid - 4, bar_w, 8, left, self.usage_color(accent, left))
            y += USAGE_ROW

    @staticmethod
    def usage_color(accent, left):
        if left < 15:
            return (1.0, 0.32, 0.32)
        if left < 40:
            return (1.0, 0.75, 0.25)
        return accent

    def meter(self, cr, x, y, w, h, left, rgb):
        """A segmented, glossy health-bar style meter showing what's left."""
        rounded_rect(cr, x, y, w, h, h / 2)
        cr.set_source_rgba(1, 1, 1, 0.12)
        cr.fill()
        fw = w * left / 100
        if fw > 0.5:
            alpha = 1.0
            if left < 15:  # nearly empty: pulse
                alpha = 0.55 + 0.45 * (0.5 + 0.5 * math.sin(self.t * 0.6))
            cr.save()
            rounded_rect(cr, x, y, w, h, h / 2)
            cr.clip()
            cr.rectangle(x, y, fw, h)
            cr.set_source_rgba(*rgb, alpha)
            cr.fill()
            cr.rectangle(x, y, fw, h * 0.45)  # gloss
            cr.set_source_rgba(1, 1, 1, 0.28 * alpha)
            cr.fill()
            cr.restore()
        cr.set_source_rgba(0.09, 0.09, 0.12, 0.9)  # segment ticks, like an HP bar
        for i in range(1, 10):
            cr.rectangle(x + w * i / 10 - 0.5, y, 1, h)
        cr.fill()

    @staticmethod
    def bolt_icon(cr, x, y, rgb):
        cr.move_to(x + 1.5, y - 6)
        cr.line_to(x - 3.5, y + 1)
        cr.line_to(x - 0.2, y + 1)
        cr.line_to(x - 1.5, y + 6)
        cr.line_to(x + 3.5, y - 1)
        cr.line_to(x + 0.2, y - 1)
        cr.close_path()
        cr.set_source_rgb(*rgb)
        cr.fill()

    @staticmethod
    def calendar_icon(cr, x, y, rgb):
        rounded_rect(cr, x - 5, y - 4.5, 10, 9.5, 1.8)
        cr.set_source_rgb(*rgb)
        cr.set_line_width(1.3)
        cr.stroke()
        cr.rectangle(x - 5, y - 4.5, 10, 3)
        cr.fill()
        for dx in (-2.5, 0, 2.5):
            cr.rectangle(x + dx - 0.6, y + 0.5, 1.2, 1.2)
            cr.rectangle(x + dx - 0.6, y + 2.6, 1.2, 1.2)
        cr.fill()

    @staticmethod
    def refill_icon(cr, x, y):
        """A small circular arrow: 'refills in'."""
        cr.set_source_rgba(1, 1, 1, 0.6)
        cr.set_line_width(1.2)
        cr.new_sub_path()
        cr.arc(x, y, 3.2, 0.6, 5.6)
        cr.stroke()
        cr.move_to(x + 3.2 * math.cos(0.6) + 1.8, y + 3.2 * math.sin(0.6) - 0.6)
        cr.line_to(x + 3.2 * math.cos(0.6) - 0.3, y + 3.2 * math.sin(0.6) + 1.9)
        cr.line_to(x + 3.2 * math.cos(0.6) - 0.9, y + 3.2 * math.sin(0.6) - 1.2)
        cr.close_path()
        cr.fill()


def main():
    if "--list-species" in sys.argv:
        print(" ".join(all_species()))
        return
    os.makedirs(SESS_DIR, exist_ok=True)
    lock = open(os.path.join(PET_DIR, "pet.lock"), "w")
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        print("pet is already running", file=sys.stderr)
        return
    with open(os.path.join(PET_DIR, "pet.pid"), "w") as f:
        f.write(str(os.getpid()))
    pet = Pet()
    pet.show_all()
    Gtk.main()


if __name__ == "__main__":
    main()
