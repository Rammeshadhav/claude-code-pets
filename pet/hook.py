#!/usr/bin/env python3
"""Claude Code hook -> pet state bridge.

Usage (from settings.json hooks):  hook.py <state>
  state: start | working | tool | waiting | done | end
Reads the hook JSON on stdin and writes ~/.claude/pet/sessions/<session_id>.json.

Hooks are registered with "async": true, so Claude never waits for this script.
Async hooks can finish out of order; every record carries the time its hook
*started*, and an older event never overwrites a newer one.
Never blocks or fails the hook.
"""
import fcntl
import json
import os
import re
import sys
import time

STARTED = time.time()  # event order = hook start order
PET_DIR = os.path.expanduser("~/.claude/pet")
SESS_DIR = os.path.join(PET_DIR, "sessions")
TITLE_TAIL = 512 * 1024  # titles are re-appended often; the transcript tail has the latest
TITLE_RE = re.compile(rb'"(customTitle|aiTitle)"\s*:\s*"((?:[^"\\]|\\.)*)"')


def main():
    state = sys.argv[1] if len(sys.argv) > 1 else "working"
    try:
        data = json.load(sys.stdin)
    except Exception:
        data = {}
    sid = str(data.get("session_id") or "unknown").replace("/", "_")
    path = os.path.join(SESS_DIR, sid + ".json")
    os.makedirs(SESS_DIR, exist_ok=True)

    if state == "end":
        try:
            os.remove(path)
        except FileNotFoundError:
            pass
        return

    detail = ""
    if state == "tool":
        state, detail = "working", data.get("tool_name", "")
    elif state == "working":
        detail = "thinking"
    elif state == "waiting":
        detail = "needs you"
    elif state == "done":
        detail = "done"
    elif state == "start":
        state, detail = "idle", "ready"

    with open(os.path.join(PET_DIR, "hook.lock"), "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            with open(path) as f:
                old = json.load(f)
        except Exception:
            old = {}
        if old.get("ts", 0) > STARTED:
            return  # a newer event already landed; don't roll the state back

        task = old.get("task", "")
        prompt = data.get("prompt")
        if isinstance(prompt, str) and prompt.strip():
            task = " ".join(prompt.split())[:80]  # latest request, one line

        title = old.get("title", "")
        if data.get("session_title"):
            title = data["session_title"]
        elif not title or detail in ("thinking", "done", "ready"):
            # not on every tool call: titles only change between turns
            title = session_title(data.get("transcript_path")) or title

        rec = {
            "state": state,
            "detail": detail,
            "title": title,
            "project": os.path.basename(data.get("cwd") or os.getcwd()) or "~",
            "task": task,
            "pid": old.get("pid") or claude_pid(),
            "ts": STARTED,
        }
        tmp = path + ".tmp"
        with open(tmp, "w") as f:
            json.dump(rec, f)
        os.replace(tmp, path)

    if state == "idle":
        launch_pet()


def session_title(transcript):
    """The session's name: a /rename title if set, else Claude's auto title."""
    if not transcript:
        return ""
    try:
        with open(transcript, "rb") as f:
            f.seek(0, os.SEEK_END)
            f.seek(max(0, f.tell() - TITLE_TAIL))
            tail = f.read()
    except OSError:
        return ""
    found = {}
    for m in TITLE_RE.finditer(tail):
        found[m.group(1)] = m.group(2)  # keep the last of each kind
    raw = found.get(b"customTitle") or found.get(b"aiTitle")
    if not raw:
        return ""
    try:
        return json.loads(b'"' + raw + b'"')
    except ValueError:
        return raw.decode("utf-8", "replace")


def claude_pid():
    """PID of the Claude Code process that ran this hook, so the pet can drop
    sessions whose terminal was closed or killed."""
    pid = os.getppid()
    for _ in range(6):
        try:
            with open(f"/proc/{pid}/comm") as f:
                comm = f.read().strip()
            with open(f"/proc/{pid}/cmdline", "rb") as f:
                cmd = f.read().split(b"\0")
            exe = os.path.basename(cmd[0]) if cmd else b""
            npm = len(cmd) > 1 and b"@anthropic-ai/claude-code" in cmd[1]  # node-based install
            if comm == "claude" or exe == b"claude" or npm:
                return pid
            with open(f"/proc/{pid}/stat") as f:
                pid = int(f.read().rsplit(")", 1)[1].split()[1])
        except (OSError, ValueError, IndexError):
            break
        if pid <= 1:
            break
    return None


def launch_pet():
    """Start the overlay if a desktop is available and it isn't already running."""
    if not (os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")):
        return
    if os.environ.get("CLAUDE_PET_DISABLE") or os.path.exists(os.path.join(PET_DIR, "disabled")):
        return
    try:
        fd = os.open(os.path.join(PET_DIR, "pet.lock"), os.O_RDWR | os.O_CREAT)
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(fd, fcntl.LOCK_UN)
        os.close(fd)
    except OSError:
        return  # already running
    import subprocess
    subprocess.Popen(
        [sys.executable, os.path.join(PET_DIR, "pet.py")],
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        start_new_session=True,
    )


if __name__ == "__main__":
    try:
        main()
    except Exception:
        pass
    sys.exit(0)
