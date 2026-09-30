#!/usr/bin/env python3
"""Claude Code status line -> pet usage bridge.

Claude Code only shares plan usage (`rate_limits`) with status line commands, so
this records it to ~/.claude/pet/usage.json for the pet's usage box, then prints
a status line: the user's own one if they had one before (passthrough), else a
compact usage summary. It never fails, so the status line never breaks.
"""
import json
import os
import subprocess
import sys
import time

PET_DIR = os.path.expanduser("~/.claude/pet")
USAGE = os.path.join(PET_DIR, "usage.json")
CONFIG = os.path.join(PET_DIR, "config.json")
WINDOWS = ("five_hour", "seven_day")


def record(data):
    """Merge this session's rate-limit windows into usage.json."""
    limits = data.get("rate_limits") or {}
    if not any(limits.get(w) for w in WINDOWS):
        return
    try:
        with open(USAGE) as f:
            usage = json.load(f)
    except Exception:
        usage = {}
    for w in WINDOWS:
        win = limits.get(w) or {}
        if win.get("used_percentage") is not None:
            usage[w] = {"used_percentage": float(win["used_percentage"]),
                        "resets_at": float(win.get("resets_at") or 0)}
    usage["ts"] = time.time()
    os.makedirs(PET_DIR, exist_ok=True)
    tmp = USAGE + ".tmp"
    with open(tmp, "w") as f:
        json.dump(usage, f)
    os.replace(tmp, USAGE)


def until(ts):
    secs = max(0, int(ts - time.time()))
    if secs >= 86400:
        return time.strftime("%a %H:%M", time.localtime(ts))
    h, m = divmod(secs // 60, 60)
    return f"{h}h {m:02d}m" if h else f"{m}m"


def summary(data):
    limits = data.get("rate_limits") or {}
    parts = []
    for w, label in (("five_hour", "5h"), ("seven_day", "week")):
        win = limits.get(w)
        if win and win.get("used_percentage") is not None:
            left = max(0, 100 - float(win["used_percentage"]))
            parts.append(f"{label} {left:.0f}% left (refills {until(float(win.get('resets_at') or 0))})")
    return " · ".join(parts)


def main():
    raw = sys.stdin.read()
    try:
        data = json.loads(raw)
    except ValueError:
        data = {}
    try:
        record(data)
    except Exception:
        pass
    try:
        with open(CONFIG) as f:
            passthrough = json.load(f).get("statusline_passthrough")
    except Exception:
        passthrough = None
    if passthrough:  # keep the user's own status line exactly as it was
        try:
            out = subprocess.run(passthrough, shell=True, input=raw, capture_output=True, text=True, timeout=5)
            sys.stdout.write(out.stdout)
        except Exception:
            pass
        return
    print(summary(data))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        pass
