//! Shared state: paths, the per-session files written by the hook, the user
//! config, and the human-readable status phrases.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A "working" session silent this long is treated as idle.
pub const STALE_WORKING: f64 = 15.0 * 60.0;
/// Session files older than this are forgotten.
pub const STALE_SESSION: f64 = 12.0 * 3600.0;
/// Idle this long and the pet falls asleep.
pub const SLEEP_AFTER: f64 = 5.0 * 60.0;

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

pub fn claude_dir() -> PathBuf {
    home().join(".claude")
}

pub fn pet_dir() -> PathBuf {
    claude_dir().join("pet")
}

pub fn sess_dir() -> PathBuf {
    pet_dir().join("sessions")
}

pub fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Write via a temp file + rename so readers never see a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

// ---- sessions ---------------------------------------------------------------

/// One Claude Code session, as recorded by `claude-pet hook`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub pid: Option<i64>,
    #[serde(default)]
    pub ts: f64,
    #[serde(skip)]
    pub path: PathBuf,
}

impl Session {
    /// What to call a session: its title, else the latest request, else its folder.
    pub fn display_name(&self) -> &str {
        [&self.title, &self.task, &self.project]
            .into_iter()
            .find(|s| !s.is_empty())
            .map(|s| s.as_str())
            .unwrap_or("claude")
    }
}

pub fn priority(state: &str) -> i32 {
    match state {
        "waiting" => 3,
        "working" => 2,
        "done" => 1,
        _ => 0,
    }
}

pub fn pid_alive(pid: i64) -> bool {
    if pid <= 0 || pid > i32::MAX as i64 {
        return true;
    }
    // SAFETY: signal 0 only checks that the process exists.
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// All live sessions, most urgent first. Removes files of dead or stale sessions.
pub fn read_sessions() -> Vec<Session> {
    let now = now();
    let mut out = Vec::new();
    let Ok(dir) = fs::read_dir(sess_dir()) else { return out };
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Ok(mut s) = serde_json::from_str::<Session>(&text) else { continue };
        let age = now - s.ts;
        // terminal closed or Claude killed without a SessionEnd hook
        if age > STALE_SESSION || s.pid.is_some_and(|p| !pid_alive(p)) {
            let _ = fs::remove_file(&path);
            continue;
        }
        if s.state == "working" && age > STALE_WORKING {
            s.state = "idle".into();
        }
        s.path = path;
        out.push(s);
    }
    out.sort_by(|a, b| {
        priority(&b.state)
            .cmp(&priority(&a.state))
            .then(b.ts.partial_cmp(&a.ts).unwrap_or(Ordering::Equal))
    });
    out
}

/// Clear "done" sessions back to idle: all of them, or just one.
pub fn acknowledge(only: Option<&Path>) {
    for mut s in read_sessions() {
        if s.state == "done" && only.map_or(true, |p| p == s.path) {
            s.state = "idle".into();
            s.detail = "ready".into();
            if let Ok(json) = serde_json::to_vec(&s) {
                let _ = write_atomic(&s.path, &json);
            }
        }
    }
}

// ---- config -----------------------------------------------------------------

/// `~/.claude/pet/config.json`, shared with the Python version.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub species: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_list: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_usage: Option<bool>,
    /// The user's own status line command from before install; still shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statusline_passthrough: Option<String>,
    /// Keys this version doesn't know about are kept as-is.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Config {
    pub fn path() -> PathBuf {
        pet_dir().join("config.json")
    }

    pub fn load() -> Self {
        let mut cfg: Self = fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        // pets renamed in 0.2 and 0.3
        let renamed = match cfg.species.as_deref() {
            Some("sharingan") => Some("crimson"),
            Some("rinnegan") => Some("ripple"),
            Some("rasengan") => Some("spiral"),
            Some("minato") => Some("flash"),
            Some("hinata") => Some("lavender"),
            Some("pokeball") => Some("blob"),
            Some("miti") => Some("robot"),
            _ => None,
        };
        if let Some(new) = renamed {
            cfg.species = Some(new.into());
        }
        cfg
    }

    pub fn save(&self) {
        let _ = fs::create_dir_all(pet_dir());
        if let Ok(json) = serde_json::to_vec(self) {
            let _ = write_atomic(&Self::path(), &json);
        }
    }
}

// ---- status text ------------------------------------------------------------

/// Short human-readable status. `t = None` gives the widest form (three dots)
/// for measuring, so labels don't jitter while the dots animate.
pub fn status_phrase(state: &str, detail: &str, asleep: bool, t: Option<f64>) -> String {
    let dots = match t {
        None => "...".to_string(),
        Some(t) => ".".repeat(1 + ((t / 4.0).floor() as i64).rem_euclid(3) as usize),
    };
    match state {
        "working" => {
            if detail.is_empty() || detail == "thinking" {
                return format!("thinking{dots}");
            }
            let verb = match detail {
                "Bash" => Some("running"),
                "Read" => Some("reading"),
                "Edit" | "MultiEdit" | "NotebookEdit" => Some("editing"),
                "Write" => Some("writing"),
                "Grep" | "Glob" | "WebSearch" => Some("searching"),
                "WebFetch" => Some("browsing"),
                "Agent" | "Task" => Some("delegating"),
                "TodoWrite" => Some("planning"),
                "Skill" => Some("using a skill"),
                _ => None,
            };
            match verb {
                Some(v) => format!("{v}{dots}"),
                None => {
                    let tool = detail.rsplit("__").next().unwrap_or(detail); // mcp__server__tool
                    let tool: String = if tool.chars().count() > 12 {
                        tool.chars().take(11).chain(std::iter::once('…')).collect()
                    } else {
                        tool.to_string()
                    };
                    format!("using {tool}{dots}")
                }
            }
        }
        "waiting" => "needs you!".into(),
        "done" => "all done".into(),
        _ if asleep => "sleeping".into(),
        _ => "ready".into(),
    }
}
