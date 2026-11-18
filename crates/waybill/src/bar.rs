//! `waybill bar`, the waybar module, and `waybill toggle`, which opens the
//! Desk drawer from the icon that was clicked.

use anyhow::Context;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use waybill_core::clock::{self, Zone};
use waybill_core::config::Config;

/// What the task database and the claims look like on disk right now. A
/// change in any of it means the list may have changed.
fn signature(dirs: &[PathBuf]) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    let mut out = Vec::new();
    for dir in dirs {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        entries.sort();
        for p in entries {
            let meta = std::fs::metadata(&p).ok();
            out.push((
                p,
                meta.as_ref().and_then(|m| m.modified().ok()),
                meta.map(|m| m.len()).unwrap_or(0),
            ));
        }
    }
    out
}

/// Prints one JSON line whenever the lamp or its numbers change. Waybar
/// keeps the process running.
pub fn run(demo: bool) -> anyhow::Result<()> {
    let mut last = String::new();
    loop {
        let (cfg, _) = Config::load();
        let now = clock::now();
        let (snap, dirs) = if demo {
            (waybill_core::demo::snapshot(now, Zone::Local), vec![])
        } else {
            let notes = waybill_core::load::notes_dir(&cfg);
            let snap = waybill_core::load::snapshot(&cfg, notes.as_deref(), now, Zone::Local);
            let mut dirs: Vec<PathBuf> = waybill_core::load::data_dir(&cfg).into_iter().collect();
            dirs.push(waybill_core::claim::dir());
            (snap, dirs)
        };
        let line = serde_json::to_string(&waybill_core::lamp::bar_line(&snap, &cfg.bar))?;
        if line != last {
            let mut out = std::io::stdout().lock();
            // A closed pipe means waybar is gone.
            if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
                return Ok(());
            }
            last = line;
        }
        // Reading the list can touch the database's own files, so the
        // baseline is taken after the read.
        let seen = signature(&dirs);
        for _ in 0..30 {
            std::thread::sleep(Duration::from_secs(2));
            // Waybar went away without closing the pipe in our face.
            // SAFETY: getppid has no failure mode.
            if unsafe { libc::getppid() } == 1 {
                return Ok(());
            }
            if signature(&dirs) != seen {
                // Let a burst of writes finish.
                std::thread::sleep(Duration::from_millis(400));
                break;
            }
        }
    }
}

fn send(socket: &Path, line: &str) -> std::io::Result<()> {
    let mut s = UnixStream::connect(socket)?;
    writeln!(s, "{line}")
}

/// Asks the drawer to open at the bar icon under the pointer, starting the
/// drawer first if it is not running.
pub fn toggle() -> anyhow::Result<()> {
    // Ask the compositor first: the pointer is still on the icon.
    let anchor = bar_anchor::anchor("waybar", bar_anchor::BarHint::default(), false);
    let line = serde_json::json!({ "toggle": anchor }).to_string();
    let socket = waybill_core::paths::runtime_dir().join("drawer.sock");
    if send(&socket, &line).is_ok() {
        return Ok(());
    }
    let board = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("waybill-board")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("waybill-board"));
    std::process::Command::new(&board)
        .arg("--drawer")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("could not start {}", board.display()))?;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(100));
        if send(&socket, &line).is_ok() {
            return Ok(());
        }
    }
    anyhow::bail!("the drawer did not come up")
}
