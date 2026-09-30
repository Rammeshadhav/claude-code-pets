//! claude-pet: a tiny animated desktop companion for Claude Code.
//!
//! One binary does everything: the pet window (`run`), the Claude Code hook
//! (`hook <state>`), the control CLI, and install/uninstall.

mod chibi;
mod ctl;
mod hook;
mod pets;
mod state;
mod ui;
mod usage;

const USAGE: &str = "\
claude-pet: a desktop pet that shows what Claude Code is doing

usage:
  claude-pet install [--no-start]   set up hooks, /pet command and ~/.local/bin/claude-pet
  claude-pet uninstall              remove everything again
  claude-pet [toggle]               show / hide the pet
  claude-pet start | stop           stop also disables auto-start until the next start
  claude-pet status
  claude-pet species [NAME]         list pets, or switch to one
  claude-pet size SIZE              small | medium | large | 0.4-2.0
  claude-pet usage on|off           show / hide the plan usage box

internal:
  claude-pet run                    run the pet window in the foreground
  claude-pet hook STATE             Claude Code hook (start|working|tool|waiting|done|end)
  claude-pet statusline             Claude Code status line (records plan usage)";

fn main() {
    // exit quietly when output goes to a closed pipe (e.g. `| head`), like other CLI tools
    // SAFETY: restoring the default SIGPIPE action before any other code runs.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("toggle");
    let rest = args.get(1..).unwrap_or(&[]);
    let code = match cmd {
        "run" => ui::run(),
        "hook" => {
            // never fail a Claude Code hook
            hook::run(rest.first().map(String::as_str).unwrap_or("working"));
            0
        }
        "statusline" => {
            usage::statusline();
            0
        }
        "start" | "stop" | "toggle" | "status" | "species" | "size" | "usage" | "install" | "uninstall" => {
            ctl::main(cmd, rest)
        }
        "-V" | "--version" | "version" => {
            println!("claude-pet {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            0
        }
        _ => {
            eprintln!("{USAGE}");
            1
        }
    };
    std::process::exit(code);
}
