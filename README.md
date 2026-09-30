# claude-code-pets

A tiny animated desktop pet for [Claude Code](https://claude.com/claude-code) on Linux. It floats on top of your windows and shows, at a glance, whether Claude is **working**, **needs you**, or is **done**, so you never miss an approval prompt while you work in other windows.

> **Branches:** `main` is the **Rust version** (recommended: one self-contained binary). The [`python`](https://github.com/Rammeshadhav/claude-code-pets/tree/python) branch has the same features in Python.

## Installation

**You need:** a Linux desktop (X11, or GNOME/Wayland) and [Claude Code](https://claude.com/claude-code).

### Build and install

```bash
# 1. build tools and GTK 3 headers (Fedora: sudo dnf install gcc gtk3-devel · Arch: sudo pacman -S base-devel gtk3)
sudo apt install build-essential libgtk-3-dev

# 2. Rust (skip if you already have it)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 3. build and install
git clone https://github.com/Rammeshadhav/claude-code-pets.git
cd claude-code-pets
cargo build --release
./target/release/claude-pet install
```

That's it. Start Claude Code and the pet appears. To use the `claude-pet` command, make sure `~/.local/bin` is on your `PATH`.

### Or: download a prebuilt binary

If a release is available on the [Releases page](https://github.com/Rammeshadhav/claude-code-pets/releases), you don't need Rust or the headers, only the GTK 3 runtime (preinstalled on GNOME, KDE, Xfce, Cinnamon and MATE):

```bash
curl -LO https://github.com/Rammeshadhav/claude-code-pets/releases/latest/download/claude-pet-x86_64-linux
chmod +x claude-pet-x86_64-linux
./claude-pet-x86_64-linux install
rm claude-pet-x86_64-linux        # it copied itself to ~/.local/bin/claude-pet
```

### What `claude-pet install` does

It's safe to re-run (to update):

| What | Where |
|---|---|
| The binary | `~/.local/bin/claude-pet` |
| Hooks (merged in; your existing hooks are kept, a backup is saved as `settings.json.bak-claude-pet`, and hooks from the Python version are replaced) | `~/.claude/settings.json` |
| Status line for plan usage (an existing status line keeps working through it) | `~/.claude/settings.json` |
| `/pet` slash command | `~/.claude/commands/pet.md` |
| Settings and session state | `~/.claude/pet/` |

### Uninstall

```bash
claude-pet uninstall
```

This stops the pet and removes its hooks (keeping yours), gives back any status line you had before, and removes the `/pet` command, `~/.claude/pet/` and the binary itself.

## What it looks like

![Pet states](docs/preview.png)

> Unofficial community project. Not affiliated with Anthropic or OpenAI.

## Features

- **Live status** from Claude Code hooks: thinking, which tool is running (`reading…`, `editing…`, `running…`), needs your approval, finished, idle and asleep.
- **One pet for all terminals.** With 2+ sessions, a **session list** under the pet shows each one by its session title, with its status. Sessions that need you come first and are highlighted.
- **10 built-in pets:** chibi ninjas **Flash** and **Lavender**; the **Crimson Eye**, **Ripple Eye**, **Spiral Orb** and a little **Robot** (smooth vector art); plus blob, cat, crab and ghost (pixel art).
- **Usage box:** your plan's 5-hour and weekly limits as themed meters (Chakra, Battery or Energy, depending on the pet), showing what's left and when each refills. Turn it on or off whenever you like.
- **Zero tokens.** The hooks print nothing, so nothing is added to Claude's context.
- **Never slows Claude down.** Hooks run async, and the pet updates within milliseconds via inotify (no polling).
- **One file, easy to remove.** `claude-pet install` sets everything up and `claude-pet uninstall` removes everything, including itself.

## Using it

### Reading the pet

| Bubble | Meaning | What to do |
|---|---|---|
| `•••` (animated) | Claude is working. The label shows the tool, e.g. `reading…` | Nothing, carry on |
| 🔴 red clock | **Claude needs you**: a permission prompt or a question | Go to the terminal |
| ✅ green check | Claude finished | Check the result, then click the pet to clear it |
| ♪ / `zzz` | Idle / asleep after 5 minutes | — |
| Blue number | That many sessions are busy at once | — |

Each pet also changes when Claude needs you. For example, the **Crimson Eye** switches from its blade pattern to three spinning tomoe, the **Ripple Eye** turns red with rings racing outward, the **Spiral Orb** grows spinning wind blades, and the **Robot** rocks back and forth with its status light blinking red (it spins its wheels while working).

The chibi ninjas act out each state too. **Flash** raises a throwing knife while working, and crackles with lightning sparks and waves at you when he needs you. **Lavender** gets little sparkles of focus while working, and blushes, pressing her fingers together, when she needs you. Both grin when Claude is done, and close their eyes when asleep.

### Usage limits

```
⚡ Chakra · 5h  ▰▰▰▰▰▰▰▰▱▱  82%   ↻ 1h 18m
▦ Chakra · 7d  ▰▰▰▰▰▱▱▱▱▱  45%   ↻ Sun 15:08
```

The box under the pet shows how much of your plan's **5-hour** and **weekly** limits is left, like a health bar, with a countdown to when each refills. The meter uses the pet's colour and name (Chakra for the eyes, the orb and the ninjas, Battery for the Robot, Energy for the pixel pets). It turns amber below 40% and pulses red below 15%. Turn it on or off from the right-click menu (**Show usage limits**) or with `claude-pet usage on|off`.

Claude Code only shares plan usage with status line commands, so the installer registers a small status line that records it. That status line shows the same numbers at the bottom of Claude Code (`5h 82% left (refills 1h 18m) · week 45% left …`). If you already had a status line, it keeps showing exactly as before, and the pet records usage behind it. Usage appears after Claude's first reply in a session and is only available on Pro and Max plans. With a custom status line set, Claude Code hides most of its footer keyboard hints; that's standard Claude Code behavior.

### Several sessions at once

```
● Fix the login bug             web-app           needs you!   ← highlighted, pulsing
● Add dark mode                 web-app           running...
● Write API docs                api-server        all done
```

Each row is named by the **session title**, the same name Claude Code shows for the session (its automatic title, or the one you set with `/rename` or `--name`). The folder follows in grey, then the status. Click a row to clear its "all done". Closed or killed terminals drop off the list within a few seconds.

### Mouse

- **Drag:** move it (the position is remembered)
- **Click:** clear the green "done" check (all sessions, or just the row you click)
- **Right-click:** choose a pet, change the size, show or hide the session list and the usage box, clear finished sessions, or put the pet away

### Commands

In any terminal (or inside Claude Code as `! claude-pet …`, which uses no tokens):

```bash
claude-pet                  # toggle show/hide
claude-pet start | stop     # stop also disables auto-start until the next 'start'
claude-pet status
claude-pet species          # list pets
claude-pet species ripple
claude-pet size small       # small | medium (default) | large | 0.4–2.0
claude-pet usage off        # hide / show the usage box
```

Inside Claude Code you can also type `/pet`, `/pet species spiral` and so on. This runs through the model, so it costs a few tokens.

## Configuration

`~/.claude/pet/config.json` is written by the menu and the CLI. It uses the same format as the Python version:

```json
{ "species": "crimson", "scale": 0.75, "show_list": true, "show_usage": true, "pos": [1600, 900] }
```

Environment variables:

- `CLAUDE_PET_DISABLE=1` stops hooks from auto-starting the pet in that shell
- `CLAUDE_PET_NATIVE=1` runs as a native Wayland client instead of XWayland (on GNOME it may then not stay on top)
- `CLAUDE_PET_DEBUG=1` logs every state change with a timestamp (run `claude-pet run` in a terminal)

## How it works

```
Claude Code ──hooks (async)──▶ claude-pet hook <state> ──▶ ~/.claude/pet/sessions/<session>.json
                                                                   │ inotify
                                                                   ▼
                                                  claude-pet run (GTK 3 window, cairo)
```

| Hook | Pet state |
|---|---|
| `SessionStart` | idle (and launches the pet if it isn't running) |
| `UserPromptSubmit`, `PostToolUse` | working · thinking |
| `PreToolUse` | working · tool name |
| `Notification` (`permission_prompt`, `elicitation_dialog`) | needs you |
| `Notification` (`idle_prompt`), `Stop` | done |
| `SessionEnd` | session removed |

- **Ordering:** hooks are async, so they can finish out of order. Each event is stamped with the time its hook started, and an older event never overwrites a newer one.
- **Session titles:** on the events between turns, the hook reads the session title from the tail of the transcript (`customTitle` from `/rename`, else `aiTitle`).
- **Closed terminals:** each session records the PID of its Claude Code process, and the pet drops the session as soon as that process is gone.
- **Priority:** with several sessions, the pet shows the most urgent one, in the order needs you > working > done > idle.

## Project layout

| File | What |
|---|---|
| `src/main.rs` | command dispatch |
| `src/ui.rs` | the GTK window, drawing, session list, menu |
| `src/pets.rs` | pet artwork (cairo) |
| `src/chibi.rs` | the chibi characters |
| `src/usage.rs` | status line → plan usage |
| `src/hook.rs` | Claude Code hook → session file |
| `src/state.rs` | session files, config, status text |
| `src/ctl.rs` | CLI, install and uninstall |

**Add your own pet:** write a drawing function in `src/pets.rs`, then add its name to `SPECIES`, `is_vector` and `draw_vector`. For a pixel pet, add a grid to `sprite()` and colors to `palette()`.

GTK 3 is used deliberately, because GTK 4 removed the always-on-top and window-positioning APIs a desktop pet needs.

## Troubleshooting

- **No pet appears:** run `claude-pet status`, then `claude-pet start`. If you'd stopped it with `claude-pet stop`, auto-start stays off until you `start` it again.
- **`error while loading shared libraries: libgtk-3.so.0`:** install the GTK 3 runtime (`sudo apt install libgtk-3-0`).
- **Pet is off-screen** (e.g. after unplugging a monitor): remove `"pos"` from `~/.claude/pet/config.json` and restart the pet.
- **Nothing happens over SSH:** the pet needs a desktop display.
- **Errors:** run `claude-pet run` in a terminal to see them.

## Credits & trademarks

- **Inspiration.** The idea comes from OpenAI's Codex Pets. The Crimson Eye, Ripple Eye and Spiral Orb, and the chibi ninjas Flash and Lavender, are original drawings inspired by the ninja-anime style, especially Masashi Kishimoto's *Naruto*, to whom credit and thanks go for the inspiration. They use their own names and designs, and contain no official artwork, names or symbols. The three-comma tomoe is a traditional Japanese motif.
- **Claude** and **Claude Code** are trademarks of Anthropic, and **Codex** is a trademark of OpenAI. This is an unofficial community project, not affiliated with or endorsed by either.

## License

[MIT](LICENSE)
