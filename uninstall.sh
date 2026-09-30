#!/usr/bin/env bash
# Remove the Claude Code desktop pet: stops it, removes its hooks, command, CLI and files.
set -euo pipefail

CLAUDE_DIR="$HOME/.claude"
PET_DIR="$CLAUDE_DIR/pet"
SETTINGS="$CLAUDE_DIR/settings.json"

if [ -f "$PET_DIR/pet.pid" ] && kill -0 "$(cat "$PET_DIR/pet.pid")" 2>/dev/null; then
  kill "$(cat "$PET_DIR/pet.pid")"
  echo "stopped the pet"
fi

if [ -f "$SETTINGS" ]; then
  cp "$SETTINGS" "$SETTINGS.bak-claude-pet"
  python3 - "$SETTINGS" <<'PY'
import json, os, sys
path = sys.argv[1]
with open(path) as f:
    settings = json.load(f)
hooks = settings.get("hooks", {})
for event in list(hooks):
    hooks[event] = [e for e in hooks[event] if "pet/hook.py" not in json.dumps(e)]
    if not hooks[event]:
        del hooks[event]
if not hooks:
    settings.pop("hooks", None)
# give back the status line the user had before, if any
sl = json.dumps(settings.get("statusLine", ""))
if "pet/statusline.py" in sl or "claude-pet statusline" in sl:
    try:
        with open(os.path.expanduser("~/.claude/pet/config.json")) as f:
            previous = json.load(f).get("statusline_passthrough")
    except Exception:
        previous = None
    if previous:
        settings["statusLine"] = {"type": "command", "command": previous}
    else:
        settings.pop("statusLine", None)
tmp = path + ".tmp"
with open(tmp, "w") as f:
    json.dump(settings, f, indent=2)
    f.write("\n")
os.replace(tmp, path)
PY
  echo "removed hooks from $SETTINGS (backup: $SETTINGS.bak-claude-pet)"
fi

rm -f "$CLAUDE_DIR/commands/pet.md" "$HOME/.local/bin/claude-pet"
rm -rf "$PET_DIR"
echo "claude-code-pet uninstalled"
