#!/usr/bin/env bash
# Install the Claude Code desktop pet for the current user.
#   ./install.sh            install / update (safe to re-run)
#   ./install.sh --no-start install without launching the pet
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_DIR="$HOME/.claude"
PET_DIR="$CLAUDE_DIR/pet"
SETTINGS="$CLAUDE_DIR/settings.json"
BIN_DIR="$HOME/.local/bin"
START=1
[ "${1:-}" = "--no-start" ] && START=0

say() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m  %s\n' "$*"; }

# 1. dependencies ------------------------------------------------------------
say "Checking dependencies"
command -v python3 >/dev/null || { warn "python3 is required"; exit 1; }
if ! python3 - <<'PY' 2>/dev/null
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("PangoCairo", "1.0")
from gi.repository import Gtk, PangoCairo  # noqa: F401
import cairo  # noqa: F401
PY
then
  warn "GTK 3 / PyGObject / pycairo not found. On Debian/Ubuntu install them with:"
  echo "      sudo apt install python3-gi python3-gi-cairo python3-cairo gir1.2-gtk-3.0"
  echo "    Fedora: sudo dnf install python3-gobject python3-cairo gtk3"
  echo "    Arch:   sudo pacman -S python-gobject python-cairo gtk3"
  exit 1
fi

# 2. files -------------------------------------------------------------------
say "Copying pet to $PET_DIR"
mkdir -p "$PET_DIR/sessions" "$CLAUDE_DIR/commands" "$BIN_DIR"
install -m 755 "$REPO/pet/pet.py" "$REPO/pet/hook.py" "$REPO/pet/statusline.py" "$REPO/pet/pet-ctl" "$PET_DIR/"
install -m 644 "$REPO/pet/vector_pets.py" "$REPO/pet/custom.py" "$PET_DIR/"
rm -rf "$PET_DIR/__pycache__"

say "Adding /pet command and 'claude-pet' CLI"
install -m 644 "$REPO/commands/pet.md" "$CLAUDE_DIR/commands/pet.md"
ln -sf "$PET_DIR/pet-ctl" "$BIN_DIR/claude-pet"
case ":$PATH:" in *":$BIN_DIR:"*) ;; *) warn "$BIN_DIR is not on your PATH; add it to use 'claude-pet'";; esac

# 3. hooks -------------------------------------------------------------------
say "Registering Claude Code hooks in $SETTINGS"
[ -f "$SETTINGS" ] && cp "$SETTINGS" "$SETTINGS.bak-claude-pet"
python3 - "$SETTINGS" <<'PY'
import json, os, sys
path = sys.argv[1]
try:
    with open(path) as f:
        settings = json.load(f)
except FileNotFoundError:
    settings = {}

HOOK = "python3 ~/.claude/pet/hook.py "

def entry(state, matcher=None):
    # async: Claude never waits on the pet; hook.py keeps events in order itself
    e = {"hooks": [{"type": "command", "command": HOOK + state, "async": True, "timeout": 5}]}
    if matcher:
        e["matcher"] = matcher
    return e

wanted = {
    "SessionStart": [entry("start")],
    "UserPromptSubmit": [entry("working")],
    "PreToolUse": [entry("tool", "*")],
    "PostToolUse": [entry("working", "*")],
    "Notification": [entry("waiting", "permission_prompt|elicitation_dialog"),
                     entry("done", "idle_prompt")],
    "Stop": [entry("done")],
    "SessionEnd": [entry("end")],
}
hooks = settings.setdefault("hooks", {})
for event, entries in wanted.items():
    existing = [e for e in hooks.get(event, []) if "pet/hook.py" not in json.dumps(e)]
    hooks[event] = existing + entries  # re-running replaces our entries, keeps yours

# status line: the only place Claude Code shares plan usage (rate_limits).
# An existing status line keeps working: statusline.py runs it and shows its output.
STATUS = "python3 ~/.claude/pet/statusline.py"
current = settings.get("statusLine")
ours = current and ("pet/statusline.py" in json.dumps(current) or "claude-pet statusline" in json.dumps(current))
if current and not ours and current.get("command"):
    cfg_path = os.path.expanduser("~/.claude/pet/config.json")
    try:
        with open(cfg_path) as f:
            cfg = json.load(f)
    except Exception:
        cfg = {}
    cfg["statusline_passthrough"] = current["command"]
    with open(cfg_path, "w") as f:
        json.dump(cfg, f)
settings["statusLine"] = {"type": "command", "command": STATUS, "padding": 0}

tmp = path + ".tmp"
with open(tmp, "w") as f:
    json.dump(settings, f, indent=2)
    f.write("\n")
os.replace(tmp, path)
PY

# 4. launch ------------------------------------------------------------------
if [ "$START" = 1 ] && [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
  say "Starting the pet"
  "$PET_DIR/pet-ctl" stop >/dev/null 2>&1 || true
  "$PET_DIR/pet-ctl" start
fi

say "Done! The pet appears whenever you start Claude Code."
echo "    Try:  claude-pet species crimson    claude-pet size small    /pet (inside Claude Code)"
