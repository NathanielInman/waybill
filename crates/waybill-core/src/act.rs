//! Every change Waybill makes to the task list. Each one is a `task`
//! command, so urgency, hooks and the sync format stay Taskwarrior's, and a
//! task is always named by its full uuid: a bare number or a filter here
//! could change more than one task.

use crate::config::Config;
use anyhow::{Context, bail};
use std::path::{Path, PathBuf};

/// `8-4-4-4-12` hex digits.
pub fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

fn tag_ok(tag: &str) -> bool {
    !tag.is_empty() && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn on(cfg: &Config, uuid: &str, args: &[&str]) -> anyhow::Result<String> {
    if !is_uuid(uuid) {
        bail!("{uuid:?} is not a task uuid");
    }
    let mut c = crate::load::command(cfg)?;
    c.args(["rc.bulk=0", "rc.recurrence.confirmation=no"]);
    c.arg(uuid);
    c.args(args);
    crate::load::run(c)
}

pub fn start(cfg: &Config, uuid: &str) -> anyhow::Result<()> {
    on(cfg, uuid, &["start"]).map(drop)
}

pub fn stop(cfg: &Config, uuid: &str) -> anyhow::Result<()> {
    on(cfg, uuid, &["stop"]).map(drop)
}

pub fn done(cfg: &Config, uuid: &str) -> anyhow::Result<()> {
    on(cfg, uuid, &["done"]).map(drop)
}

/// Logs a visit. The text goes after `--`, so nothing in it is read as an
/// attribute.
pub fn annotate(cfg: &Config, uuid: &str, text: &str) -> anyhow::Result<()> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        bail!("a visit needs some words");
    }
    on(cfg, uuid, &["annotate", "--", &text]).map(drop)
}

/// One change to a task's attributes.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// 'H', 'M', 'L', or ' ' for none.
    Priority(char),
    AddTag(String),
    DropTag(String),
    /// A Taskwarrior date ("friday", "2026-12-01", "someday"); empty shows
    /// the task again.
    Wait(String),
    Project(String),
}

impl Change {
    fn arg(&self) -> anyhow::Result<String> {
        Ok(match self {
            Change::Priority(p @ ('H' | 'M' | 'L')) => format!("priority:{p}"),
            Change::Priority(' ') => "priority:".into(),
            Change::Priority(p) => bail!("{p:?} is not a priority"),
            Change::AddTag(t) if tag_ok(t) => format!("+{t}"),
            Change::DropTag(t) if tag_ok(t) => format!("-{t}"),
            Change::AddTag(t) | Change::DropTag(t) => bail!("{t:?} is not a tag"),
            Change::Wait(w) if !w.contains(char::is_whitespace) => format!("wait:{w}"),
            Change::Wait(w) => bail!("{w:?} is not one date"),
            Change::Project(p) if !p.contains(char::is_whitespace) => format!("project:{p}"),
            Change::Project(p) => bail!("{p:?} is not a project name"),
        })
    }
}

pub fn modify(cfg: &Config, uuid: &str, changes: &[Change]) -> anyhow::Result<()> {
    if changes.is_empty() {
        return Ok(());
    }
    let args: Vec<String> = changes
        .iter()
        .map(Change::arg)
        .collect::<anyhow::Result<_>>()?;
    let mut all = vec!["modify"];
    all.extend(args.iter().map(String::as_str));
    on(cfg, uuid, &all).map(drop)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewTask {
    pub title: String,
    pub project: Option<String>,
    pub priority: Option<char>,
    pub tags: Vec<String>,
    /// The `ref` marker. Needs the `ref` field defined in the taskrc.
    pub reference: Option<String>,
    /// Uuids this task waits on.
    pub after: Vec<String>,
}

/// Adds a task and returns its uuid.
pub fn add(cfg: &Config, new: &NewTask) -> anyhow::Result<String> {
    let title = new.title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        bail!("a task needs a title");
    }
    let mut c = crate::load::command(cfg)?;
    c.args(["rc.verbose=new-uuid", "add"]);
    if let Some(p) = new.project.as_deref().filter(|p| !p.is_empty()) {
        c.arg(Change::Project(p.to_string()).arg()?);
    }
    if let Some(p) = new.priority {
        c.arg(Change::Priority(p).arg()?);
    }
    for t in &new.tags {
        c.arg(Change::AddTag(t.clone()).arg()?);
    }
    if let Some(r) = new.reference.as_deref().filter(|r| !r.is_empty()) {
        if r.contains(char::is_whitespace) {
            bail!("{r:?} is not one marker");
        }
        c.arg(format!("ref:{r}"));
    }
    if !new.after.is_empty() {
        if let Some(bad) = new.after.iter().find(|u| !is_uuid(u)) {
            bail!("{bad:?} is not a task uuid");
        }
        c.arg(format!("depends:{}", new.after.join(",")));
    }
    c.arg("--").arg(&title);
    let out = crate::load::run(c)?;
    out.split(|ch: char| !(ch.is_ascii_hexdigit() || ch == '-'))
        .find(|w| is_uuid(w))
        .map(String::from)
        .with_context(|| format!("task add did not name the new task: {}", out.trim()))
}

pub fn note_path(dir: &Path, uuid: &str) -> PathBuf {
    dir.join(format!("{uuid}.md"))
}

pub fn read_note(dir: &Path, uuid: &str) -> Option<String> {
    std::fs::read_to_string(note_path(dir, uuid)).ok()
}

pub fn write_note(dir: &Path, uuid: &str, text: &str) -> anyhow::Result<()> {
    if !is_uuid(uuid) {
        bail!("{uuid:?} is not a task uuid");
    }
    std::fs::create_dir_all(dir)?;
    let mut text = text.trim_end().to_string();
    text.push('\n');
    let path = note_path(dir, uuid);
    std::fs::write(&path, text).with_context(|| format!("could not write {}", path.display()))
}

/// Adds a section to the end of a note, starting the note if there is none.
pub fn append_note(dir: &Path, uuid: &str, section: &str) -> anyhow::Result<()> {
    let old = read_note(dir, uuid).unwrap_or_default();
    let joined = if old.trim().is_empty() {
        section.to_string()
    } else {
        format!("{}\n\n{}", old.trim_end(), section.trim_start())
    };
    write_note(dir, uuid, &joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_full_uuid_names_a_task() {
        assert!(is_uuid("a0317d24-5bc9-4a78-af5d-3c82dadba78e"));
        for bad in [
            "12",
            "a0317d24",
            "status:pending",
            "a0317d24-5bc9-4a78-af5d-3c82dadba78z",
            "",
        ] {
            assert!(!is_uuid(bad), "{bad}");
            assert!(start(&Config::default(), bad).is_err());
        }
    }

    #[test]
    fn changes_become_one_argument_each() {
        assert_eq!(Change::Priority('H').arg().unwrap(), "priority:H");
        assert_eq!(Change::Priority(' ').arg().unwrap(), "priority:");
        assert_eq!(Change::AddTag("nate".into()).arg().unwrap(), "+nate");
        assert_eq!(Change::Wait("friday".into()).arg().unwrap(), "wait:friday");
        assert_eq!(Change::Wait(String::new()).arg().unwrap(), "wait:");
        assert!(Change::Priority('X').arg().is_err());
        assert!(Change::AddTag("a b".into()).arg().is_err());
        assert!(Change::Wait("next friday".into()).arg().is_err());
        assert!(Change::Project("a status:pending".into()).arg().is_err());
    }
}
