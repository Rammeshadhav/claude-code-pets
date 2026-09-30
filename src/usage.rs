//! Plan usage (the 5-hour and weekly limits).
//!
//! Claude Code only shares `rate_limits` with status line commands, so
//! `claude-pet statusline` records them to ~/.claude/pet/usage.json for the
//! pet's usage box, then prints a status line: the user's own one if they had
//! one before (passthrough), else a compact usage summary. It never fails.

use crate::state::{now, pet_dir, write_atomic, Config};
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// (key in rate_limits, short label)
pub const WINDOWS: [(&str, &str); 2] = [("five_hour", "5h"), ("seven_day", "7d")];

fn usage_path() -> PathBuf {
    pet_dir().join("usage.json")
}

/// One window for the usage box.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub label: &'static str,
    /// Percentage still available, 0..=100.
    pub left: f64,
    /// Unix seconds when the window resets (0 if unknown).
    pub resets_at: f64,
}

/// Remaining % and reset time per window; a window past its reset is full again.
pub fn read() -> Vec<Window> {
    let Some(data) = fs::read_to_string(usage_path()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
        return Vec::new();
    };
    let now = now();
    WINDOWS
        .iter()
        .filter_map(|(key, label)| {
            let win = data.get(*key)?;
            let used = win.get("used_percentage")?.as_f64()?;
            let resets_at = win.get("resets_at").and_then(Value::as_f64).unwrap_or(0.0);
            let used = if resets_at > 0.0 && now >= resets_at { 0.0 } else { used };
            Some(Window { label, left: (100.0 - used).clamp(0.0, 100.0), resets_at })
        })
        .collect()
}

/// "1h 20m", "12m", or a weekday + time for resets more than a day away.
pub fn until(ts: f64) -> String {
    let secs = (ts - now()) as i64;
    if secs <= 0 {
        return "now".into();
    }
    if secs >= 86_400 {
        return local_time(ts as i64, "%a %H:%M");
    }
    let (h, m) = (secs / 3600, (secs / 60) % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{}m", m.max(1))
    }
}

fn local_time(ts: i64, fmt: &str) -> String {
    let t = ts as libc::time_t;
    // SAFETY: localtime_r and strftime write only into the buffers we pass.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return String::new();
        }
        let mut buf = [0u8; 64];
        let fmt = std::ffi::CString::new(fmt).unwrap_or_default();
        let n = libc::strftime(buf.as_mut_ptr() as *mut libc::c_char, buf.len(), fmt.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

/// Merge this session's rate-limit windows into usage.json.
fn record(data: &Value) {
    let Some(limits) = data.get("rate_limits") else { return };
    let mut usage: Value = fs::read_to_string(usage_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let mut changed = false;
    for (key, _) in WINDOWS {
        let Some(used) = limits.get(key).and_then(|w| w.get("used_percentage")).and_then(Value::as_f64) else {
            continue;
        };
        let resets_at = limits[key].get("resets_at").and_then(Value::as_f64).unwrap_or(0.0);
        usage[key] = json!({"used_percentage": used, "resets_at": resets_at});
        changed = true;
    }
    if changed {
        usage["ts"] = json!(now());
        let _ = fs::create_dir_all(pet_dir());
        if let Ok(bytes) = serde_json::to_vec(&usage) {
            let _ = write_atomic(&usage_path(), &bytes);
        }
    }
}

fn summary(data: &Value) -> String {
    let mut parts = Vec::new();
    for (key, label) in [("five_hour", "5h"), ("seven_day", "week")] {
        let win = &data["rate_limits"][key];
        if let Some(used) = win["used_percentage"].as_f64() {
            let left = (100.0 - used).max(0.0);
            let resets = win["resets_at"].as_f64().unwrap_or(0.0);
            parts.push(format!("{label} {left:.0}% left (refills {})", until(resets)));
        }
    }
    parts.join(" · ")
}

/// `claude-pet statusline`
pub fn statusline() {
    let mut raw = String::new();
    let _ = std::io::stdin().read_to_string(&mut raw);
    let data: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    record(&data);

    if let Some(cmd) = Config::load().statusline_passthrough.filter(|c| !c.trim().is_empty()) {
        // keep the user's own status line exactly as it was
        let child = Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = child {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(raw.as_bytes());
            }
            let _ = child.wait();
        }
        return;
    }
    println!("{}", summary(&data));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_countdowns() {
        assert_eq!(until(now() + 3.0 * 3600.0 + 125.0), "3h 02m");
        assert_eq!(until(now() + 30.0), "1m");
        assert_eq!(until(now() - 5.0), "now");
        assert!(until(now() + 3.0 * 86_400.0).contains(':'));
    }

    #[test]
    fn summarizes_rate_limits() {
        let data = json!({"rate_limits": {"five_hour": {"used_percentage": 25.0, "resets_at": now() + 600.0}}});
        assert!(summary(&data).starts_with("5h 75% left (refills"));
        assert_eq!(summary(&json!({})), "");
    }
}
