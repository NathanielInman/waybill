//! Reading the task list: one `task export`, and a look at which notes
//! exist. Read-only; nothing here changes the database.

use crate::clock::Zone;
use crate::config::Config;
use crate::model::{Rules, Snapshot};
use crate::task::Task;
use anyhow::{Context, bail};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn command(cfg: &Config) -> anyhow::Result<Command> {
    let mut words = cfg.task.command.split_whitespace();
    let Some(program) = words.next() else {
        bail!("task.command is empty");
    };
    let mut c = Command::new(crate::paths::expand(program));
    c.args(words);
    // Quiet, no id renumbering as a side effect, and one JSON array.
    c.args([
        "rc.verbose=nothing",
        "rc.gc=off",
        "rc.confirmation=off",
        "rc.json.array=on",
    ]);
    Ok(c)
}

fn run(mut c: Command) -> anyhow::Result<String> {
    let out = c
        .output()
        .with_context(|| format!("could not run {:?}", c.get_program()))?;
    if !out.status.success() {
        bail!(
            "{:?} failed: {}",
            c.get_program(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every task Taskwarrior knows.
pub fn export(cfg: &Config) -> anyhow::Result<Vec<Task>> {
    let mut c = command(cfg)?;
    c.arg("export");
    crate::task::parse(&run(c)?).context("task export did not print the JSON Waybill reads")
}

/// Taskwarrior's `data.location`.
pub fn data_dir(cfg: &Config) -> Option<PathBuf> {
    let mut c = command(cfg).ok()?;
    c.args(["_get", "rc.data.location"]);
    let out = run(c).ok()?;
    let dir = out.trim();
    (!dir.is_empty()).then(|| crate::paths::expand(dir))
}

/// The folder of `<uuid>.md` notes.
pub fn notes_dir(cfg: &Config) -> Option<PathBuf> {
    if cfg.notes.dir.trim().is_empty() {
        data_dir(cfg).map(|d| d.join("notes"))
    } else {
        Some(crate::paths::expand(&cfg.notes.dir))
    }
}

/// The uuids that have a note.
pub fn noted(dir: &Path) -> HashSet<String> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            name.to_str()?.strip_suffix(".md").map(String::from)
        })
        .collect()
}

pub fn rules(cfg: &Config) -> Rules {
    Rules {
        hands: cfg.tags.hands.clone(),
        decision: cfg.tags.decision.clone(),
    }
}

/// The whole read: export, notes, model. A failure comes back as a snapshot
/// that says why, so the board has something to show.
pub fn snapshot(cfg: &Config, notes: Option<&Path>, now: i64, zone: Zone) -> Snapshot {
    match export(cfg) {
        Ok(tasks) => {
            let noted = notes.map(noted).unwrap_or_default();
            crate::model::build(&tasks, &noted, &rules(cfg), now, zone)
        }
        Err(e) => Snapshot::failed(now, zone, format!("{e:#}")),
    }
}
