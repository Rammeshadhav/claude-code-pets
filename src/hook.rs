//! `claude-pet hook <state>`: Claude Code hook -> pet state bridge.
//!
//! state: start | working | tool | waiting | done | end
//! Reads the hook JSON on stdin and writes ~/.claude/pet/sessions/<session_id>.json.
//!
//! Hooks are registered with "async": true, so Claude never waits for this.
//! Async hooks can finish out of order; every record carries the time its hook
//! *started*, and an older event never overwrites a newer one.
//! Never fails the hook: all errors are swallowed.

use crate::state::{now, pet_dir, sess_dir, write_atomic, Session};
use serde_json::Value;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;

/// Titles are re-appended often; the transcript tail holds the latest one.
const TITLE_TAIL: u64 = 512 * 1024;

pub fn run(arg: &str) {
    let started = now(); // event order = hook start order
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let data: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
    let get = |k: &str| data.get(k).and_then(Value::as_str).unwrap_or("").to_string();

    let sid = match get("session_id") {
        s if s.is_empty() => "unknown".to_string(),
        s => s.replace('/', "_"),
    };
    let _ = fs::create_dir_all(sess_dir());
    let path = sess_dir().join(format!("{sid}.json"));

    if arg == "end" {
        let _ = fs::remove_file(&path);
        return;
    }

    let (state, detail) = match arg {
        "tool" => ("working", get("tool_name")),
        "working" => ("working", "thinking".to_string()),
        "waiting" => ("waiting", "needs you".to_string()),
        "done" => ("done", "done".to_string()),
        "start" => ("idle", "ready".to_string()),
        other => (other, String::new()),
    };

    {
        // serialize concurrent (async) hooks; released when `lock` drops
        let lock = OpenOptions::new().create(true).truncate(false).write(true).open(pet_dir().join("hook.lock"));
        if let Ok(f) = &lock {
            // SAFETY: flock on a file descriptor we own.
            unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) };
        }

        let old: Session = fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        if old.ts > started {
            return; // a newer event already landed; don't roll the state back
        }

        let mut task = old.task.clone();
        let prompt = get("prompt");
        if !prompt.trim().is_empty() {
            task = prompt.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect();
        }

        let mut title = old.title.clone();
        let session_title = get("session_title");
        if !session_title.is_empty() {
            title = session_title; // set via /rename or --name
        } else if title.is_empty() || matches!(detail.as_str(), "thinking" | "done" | "ready") {
            // not on every tool call: titles only change between turns
            if let Some(t) = transcript_title(&get("transcript_path")) {
                title = t;
            }
        }

        let cwd = get("cwd");
        let cwd = if cwd.is_empty() {
            std::env::current_dir().unwrap_or_default()
        } else {
            cwd.into()
        };
        let project = cwd.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "~".into());

        let rec = Session {
            state: state.to_string(),
            detail,
            title,
            project,
            task,
            pid: old.pid.or_else(claude_pid),
            ts: started,
            path: Default::default(),
        };
        if let Ok(json) = serde_json::to_vec(&rec) {
            let _ = write_atomic(&path, &json);
        }
    }

    if state == "idle" {
        launch_pet();
    }
}

/// The session's name: a /rename title if set, else Claude's automatic title.
fn transcript_title(transcript: &str) -> Option<String> {
    if transcript.is_empty() {
        return None;
    }
    let mut f = File::open(transcript).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(TITLE_TAIL))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    last_string_value(&buf, b"\"customTitle\"").or_else(|| last_string_value(&buf, b"\"aiTitle\""))
}

/// The last `"key": "value"` string in a chunk of JSON lines.
fn last_string_value(buf: &[u8], key: &[u8]) -> Option<String> {
    let mut found = None;
    let mut i = 0;
    while let Some(pos) = find(&buf[i..], key) {
        let mut j = i + pos + key.len();
        while j < buf.len() && matches!(buf[j], b' ' | b':') {
            j += 1;
        }
        if j < buf.len() && buf[j] == b'"' {
            let mut k = j + 1;
            while k < buf.len() && buf[k] != b'"' {
                k += if buf[k] == b'\\' { 2 } else { 1 };
            }
            if k < buf.len() {
                if let Ok(v) = serde_json::from_slice::<String>(&buf[j..=k]) {
                    if !v.is_empty() {
                        found = Some(v);
                    }
                }
            }
            i = k.min(buf.len());
        } else {
            i = j;
        }
    }
    found
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// PID of the Claude Code process that ran this hook, so the pet can drop
/// sessions whose terminal was closed or killed.
fn claude_pid() -> Option<i64> {
    // SAFETY: getppid never fails.
    let mut pid = unsafe { libc::getppid() } as i64;
    for _ in 0..6 {
        let comm = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
        let exe = args
            .first()
            .and_then(|a| Path::new(OsStr::from_bytes(a)).file_name())
            .map(|n| n.as_bytes() == b"claude")
            .unwrap_or(false);
        let npm = args.get(1).is_some_and(|a| find(a, b"@anthropic-ai/claude-code").is_some());
        if comm.trim() == "claude" || exe || npm {
            return Some(pid);
        }
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        pid = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()?;
        if pid <= 1 {
            return None;
        }
    }
    None
}

/// Start the overlay if a desktop is available and it isn't already running.
fn launch_pet() {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return;
    }
    if std::env::var_os("CLAUDE_PET_DISABLE").is_some() || pet_dir().join("disabled").exists() {
        return;
    }
    if !crate::ctl::running() {
        let _ = crate::ctl::spawn_pet();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_last_title() {
        let buf = br#"{"type":"ai-title","aiTitle":"Old"}
{"type":"ai-title","aiTitle":"New \"quoted\" title"}"#;
        assert_eq!(last_string_value(buf, b"\"aiTitle\"").as_deref(), Some("New \"quoted\" title"));
        assert_eq!(last_string_value(buf, b"\"customTitle\""), None);
    }

    #[test]
    fn status_phrases() {
        assert_eq!(crate::state::status_phrase("working", "Read", false, None), "reading...");
        assert_eq!(crate::state::status_phrase("working", "mcp__srv__navigate", false, None), "using navigate...");
        assert_eq!(crate::state::status_phrase("idle", "", true, None), "sleeping");
    }
}
