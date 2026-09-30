//! Command-line control: start/stop/status/species/size and install/uninstall.

use crate::custom;
use crate::pets::{all_species, SPECIES};
use crate::state::{claude_dir, home, pet_dir, sess_dir, write_atomic, Config};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Shown by /pet inside Claude Code.
const SLASH_COMMAND: &str = include_str!("../commands/pet.md");
/// Hook command prefix; `~` is expanded by the shell Claude Code runs hooks in.
const HOOK_CMD: &str = "~/.local/bin/claude-pet hook ";
/// Status line command: the only place Claude Code shares plan usage.
const STATUS_CMD: &str = "~/.local/bin/claude-pet statusline";

pub fn bin_path() -> PathBuf {
    home().join(".local/bin/claude-pet")
}

fn pid_file() -> PathBuf {
    pet_dir().join("pet.pid")
}

/// True if a pet holds the single-instance lock.
pub fn running() -> bool {
    let Ok(f) = OpenOptions::new().create(true).truncate(false).write(true).open(pet_dir().join("pet.lock")) else {
        return false;
    };
    // SAFETY: flock on a file descriptor we own; unlocked straight away.
    unsafe {
        if libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) != 0 {
            return true;
        }
        libc::flock(f.as_raw_fd(), libc::LOCK_UN);
    }
    false
}

fn pet_pid() -> Option<i32> {
    fs::read_to_string(pid_file()).ok()?.trim().parse().ok()
}

/// Launch `claude-pet run` fully detached from the terminal.
pub fn spawn_pet() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg("run").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().map(|_| ())
}

fn has_display() -> bool {
    std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

fn start() -> i32 {
    let _ = fs::create_dir_all(sess_dir());
    let _ = fs::remove_file(pet_dir().join("disabled"));
    if running() {
        println!("pet is already out");
        return 0;
    }
    if !has_display() {
        println!("no desktop display here (SSH?)");
        return 1;
    }
    match spawn_pet() {
        Ok(()) => {
            println!("pet summoned");
            0
        }
        Err(e) => {
            eprintln!("could not start the pet: {e}");
            1
        }
    }
}

/// Stop a running pet (Python or Rust) and wait for it to exit.
fn kill_pet() -> bool {
    let was_running = running();
    if let Some(pid) = pet_pid() {
        // SAFETY: plain SIGTERM to the recorded pet process.
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    was_running
}

fn stop() -> i32 {
    let _ = fs::create_dir_all(pet_dir());
    let _ = fs::write(pet_dir().join("disabled"), ""); // keep hooks from re-summoning it
    println!("{}", if kill_pet() { "pet put away" } else { "pet is not out" });
    0
}

fn restart_if_running() {
    if running() {
        kill_pet();
        if has_display() {
            let _ = spawn_pet();
        }
    }
}

fn status() -> i32 {
    println!("{}", if running() { "pet is out" } else { "pet is away" });
    let n = fs::read_dir(sess_dir())
        .map(|d| d.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "json")).count())
        .unwrap_or(0);
    println!("tracked sessions: {n}");
    0
}

fn species(name: Option<&str>) -> i32 {
    let all = all_species().join(" ");
    let Some(name) = name else {
        println!("species: {all}");
        return 0;
    };
    if !all_species().iter().any(|s| s == name) {
        println!("choose one of: {all}");
        return 1;
    }
    let mut cfg = Config::load();
    cfg.species = Some(name.to_string());
    cfg.save();
    restart_if_running();
    println!("your pet is now a {name}");
    0
}

fn usage_box(arg: Option<&str>) -> i32 {
    let on = match arg {
        Some("on") => true,
        Some("off") => false,
        _ => {
            println!("usage: claude-pet usage on|off");
            return 1;
        }
    };
    let mut cfg = Config::load();
    cfg.show_usage = Some(on);
    cfg.save();
    restart_if_running();
    println!("usage box turned {}", if on { "on" } else { "off" });
    0
}

/// `claude-pet add NAME IMAGE [--meter WORD]`
fn add(args: &[String]) -> i32 {
    let (Some(name), Some(image)) = (args.first(), args.get(1)) else {
        println!("usage: claude-pet add NAME IMAGE [--meter WORD]");
        return 1;
    };
    if !custom::valid_name(name) {
        println!("pet names use lowercase letters, digits and '-' (up to 32)");
        return 1;
    }
    if SPECIES.contains(&name.as_str()) {
        println!("'{name}' is a built-in pet; pick another name");
        return 1;
    }
    let meter = args
        .iter()
        .position(|a| a == "--meter")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
        .unwrap_or("Energy");
    // GDK must be initialised to decode images; no window is opened
    let _ = gtk::gdk_pixbuf::Pixbuf::formats();
    match custom::add(name, std::path::Path::new(image), meter) {
        Ok(meta) => {
            let how = if meta.mode == "card" {
                "busy background, shown as a portrait card"
            } else {
                "background removed"
            };
            println!("added '{name}' ({how})");
            let mut cfg = Config::load();
            cfg.species = Some(name.clone());
            cfg.save();
            restart_if_running();
            println!("your pet is now {name}");
            0
        }
        Err(e) => {
            println!("could not add '{name}': {e}");
            1
        }
    }
}

fn remove(name: Option<&str>) -> i32 {
    let Some(name) = name.filter(|n| custom::valid_name(n)) else {
        println!("usage: claude-pet remove NAME");
        return 1;
    };
    if !custom::remove(name) {
        println!("no image pet called '{name}'");
        return 1;
    }
    let mut cfg = Config::load();
    if cfg.species.as_deref() == Some(name) {
        cfg.species = None;
        cfg.save();
        restart_if_running();
    }
    println!("removed '{name}'");
    0
}

fn size(arg: Option<&str>) -> i32 {
    let value = match arg {
        Some("small") => Some(0.6),
        Some("medium") => Some(0.75),
        Some("large") => Some(1.0),
        Some(v) => v.parse::<f64>().ok(),
        None => None,
    };
    match value {
        Some(v) if (0.4..=2.0).contains(&v) => {
            let mut cfg = Config::load();
            cfg.scale = Some(v);
            cfg.save();
            restart_if_running();
            println!("pet size set to {v}");
            0
        }
        _ => {
            println!("usage: claude-pet size small|medium|large|<0.4-2.0>");
            1
        }
    }
}

// ---- install / uninstall ------------------------------------------------------

fn settings_path() -> PathBuf {
    claude_dir().join("settings.json")
}

/// Our hook entries, from this version or the Python one.
fn is_ours(entry: &Value) -> bool {
    let s = entry.to_string();
    s.contains("claude-pet hook") || s.contains("pet/hook.py")
}

/// Our status line, from this version or the Python one.
fn is_our_statusline(entry: &Value) -> bool {
    let s = entry.to_string();
    s.contains("claude-pet statusline") || s.contains("pet/statusline.py")
}

fn hook_entry(state: &str, matcher: Option<&str>) -> Value {
    // async: Claude never waits on the pet; the hook keeps events in order itself
    let mut e = json!({"hooks": [{"type": "command", "command": format!("{HOOK_CMD}{state}"),
                                  "async": true, "timeout": 5}]});
    if let Some(m) = matcher {
        e["matcher"] = json!(m);
    }
    e
}

/// Add (or remove) our hooks in ~/.claude/settings.json, keeping everything else.
fn update_settings(add: bool) -> Result<(), String> {
    let path = settings_path();
    let mut settings: Value = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?,
        Err(_) if add => json!({}),
        Err(_) => return Ok(()),
    };
    if !settings.is_object() {
        return Err(format!("{} is not a JSON object", path.display()));
    }
    if path.exists() {
        let _ = fs::copy(&path, path.with_extension("json.bak-claude-pet"));
    }

    let wanted: Vec<(&str, Vec<Value>)> = vec![
        ("SessionStart", vec![hook_entry("start", None)]),
        ("UserPromptSubmit", vec![hook_entry("working", None)]),
        ("PreToolUse", vec![hook_entry("tool", Some("*"))]),
        ("PostToolUse", vec![hook_entry("working", Some("*"))]),
        ("Notification", vec![
            hook_entry("waiting", Some("permission_prompt|elicitation_dialog")),
            hook_entry("done", Some("idle_prompt")),
        ]),
        ("Stop", vec![hook_entry("done", None)]),
        ("SessionEnd", vec![hook_entry("end", None)]),
    ];

    let obj = settings.as_object_mut().expect("checked above");
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return Err("\"hooks\" in settings.json is not an object".into());
    };
    // drop our old entries everywhere, keep the user's own hooks
    for list in hooks.values_mut() {
        if let Some(arr) = list.as_array_mut() {
            arr.retain(|e| !is_ours(e));
        }
    }
    if add {
        for (event, entries) in wanted {
            let list = hooks.entry(event).or_insert_with(|| json!([]));
            if let Some(arr) = list.as_array_mut() {
                arr.extend(entries);
            }
        }
    }
    hooks.retain(|_, v| v.as_array().map_or(true, |a| !a.is_empty()));
    if hooks.is_empty() {
        obj.remove("hooks");
    }

    // status line: wrap an existing one (it keeps showing), or give it back on uninstall
    let current = obj.get("statusLine").cloned();
    let ours = current.as_ref().is_some_and(is_our_statusline);
    let mut cfg = Config::load();
    if add {
        if let (Some(cur), false) = (&current, ours) {
            if let Some(cmd) = cur.get("command").and_then(Value::as_str) {
                cfg.statusline_passthrough = Some(cmd.to_string());
                cfg.save();
            }
        }
        obj.insert("statusLine".into(), json!({"type": "command", "command": STATUS_CMD, "padding": 0}));
    } else if ours {
        match cfg.statusline_passthrough.take() {
            Some(cmd) => {
                obj.insert("statusLine".into(), json!({"type": "command", "command": cmd}));
            }
            None => {
                obj.remove("statusLine");
            }
        }
    }

    let mut text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    text.push('\n');
    fs::create_dir_all(claude_dir()).map_err(|e| e.to_string())?;
    write_atomic(&path, text.as_bytes()).map_err(|e| e.to_string())
}

fn install(args: &[String]) -> i32 {
    let no_start = args.iter().any(|a| a == "--no-start");
    let step = |msg: &str| println!("\x1b[1;36m==>\x1b[0m {msg}");

    // 1. binary -> ~/.local/bin/claude-pet
    let dest = bin_path();
    let src = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cannot find this program: {e}");
            return 1;
        }
    };
    let same = fs::canonicalize(&src).ok() == fs::canonicalize(&dest).ok();
    if !same {
        step(&format!("Installing binary to {}", dest.display()));
        kill_pet(); // a running pet may be the old binary
        let _ = fs::create_dir_all(dest.parent().expect("has parent"));
        let _ = fs::remove_file(&dest); // may be the Python version's symlink
        if let Err(e) = fs::copy(&src, &dest) {
            eprintln!("could not copy to {}: {e}", dest.display());
            return 1;
        }
        let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(0o755));
    }

    // 2. state dir, /pet command
    let _ = fs::create_dir_all(sess_dir());
    step("Adding the /pet command");
    let commands = claude_dir().join("commands");
    let _ = fs::create_dir_all(&commands);
    if let Err(e) = fs::write(commands.join("pet.md"), SLASH_COMMAND) {
        eprintln!("could not write /pet command: {e}");
    }
    // files left over from the Python version
    for old in ["pet.py", "vector_pets.py", "hook.py", "statusline.py", "pet-ctl", "__pycache__"] {
        let p = pet_dir().join(old);
        let _ = if p.is_dir() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) };
    }

    // 3. hooks
    step(&format!("Registering Claude Code hooks in {}", settings_path().display()));
    if let Err(e) = update_settings(true) {
        eprintln!("could not update settings: {e}");
        return 1;
    }

    let on_path = std::env::var("PATH")
        .map(|p| p.split(':').any(|d| std::path::Path::new(d) == dest.parent().unwrap_or(&dest)))
        .unwrap_or(false);
    if !on_path {
        println!("\x1b[1;33m!!\x1b[0m  ~/.local/bin is not on your PATH; add it to use 'claude-pet' directly");
    }

    // 4. launch
    if !no_start && has_display() {
        step("Starting the pet");
        kill_pet();
        let _ = fs::remove_file(pet_dir().join("disabled"));
        if Command::new(&dest).arg("start").status().is_err() {
            let _ = spawn_pet();
        }
    }
    step("Done! The pet appears whenever you start Claude Code.");
    println!("    Try:  claude-pet species crimson    claude-pet size small    /pet (inside Claude Code)");
    0
}

fn uninstall() -> i32 {
    if kill_pet() {
        println!("stopped the pet");
    }
    match update_settings(false) {
        Ok(()) => println!("removed hooks from {} (backup: settings.json.bak-claude-pet)", settings_path().display()),
        Err(e) => eprintln!("could not update settings: {e}"),
    }
    let _ = fs::remove_file(claude_dir().join("commands/pet.md"));
    let _ = fs::remove_dir_all(pet_dir());
    let _ = fs::remove_file(bin_path());
    println!("claude-pet uninstalled");
    0
}

pub fn main(cmd: &str, args: &[String]) -> i32 {
    let arg = args.first().map(String::as_str);
    match cmd {
        "start" => start(),
        "stop" => stop(),
        "toggle" => {
            if running() {
                stop()
            } else {
                start()
            }
        }
        "status" => status(),
        "species" => species(arg),
        "size" => size(arg),
        "usage" => usage_box(arg),
        "add" => add(args),
        "remove" => remove(arg),
        "install" => install(args),
        "uninstall" => uninstall(),
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_our_hooks() {
        assert!(is_ours(&hook_entry("done", None)));
        assert!(is_ours(&json!({"hooks": [{"command": "python3 ~/.claude/pet/hook.py done"}]})));
        assert!(!is_ours(&json!({"hooks": [{"command": "echo mine"}]})));
        assert!(is_our_statusline(&json!({"command": STATUS_CMD})));
        assert!(is_our_statusline(&json!({"command": "python3 ~/.claude/pet/statusline.py"})));
        assert!(!is_our_statusline(&json!({"command": "echo mine"})));
    }
}
