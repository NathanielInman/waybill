//! Who holds a started task. `waybill next` and `waybill start` leave a
//! claim: a small file in the runtime directory naming the session that
//! started the task. The runtime directory does not survive a reboot and a
//! claim does not outlive its process, so a started task whose claim is dead
//! is a start somebody left behind.

use crate::model::{Lane, Snapshot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    pub uuid: String,
    /// The session's process, when one could be found.
    pub pid: Option<i32>,
    /// That process's start time (clock ticks since boot), so a reused pid
    /// is not mistaken for it.
    pub born: Option<u64>,
    /// "claude in waybill (main)".
    pub who: String,
    /// The harness's own name for the session, when it has one.
    pub session: String,
    pub cwd: String,
    pub since: i64,
    /// The last time the holder did anything with the task.
    pub touched: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HoldState {
    /// A live session holds it.
    Held,
    /// Started without a claim (by hand, or by a session that does not use
    /// Waybill). Nobody can say who has it.
    Unclaimed,
    /// Its session is gone, or it was started before the machine booted.
    Stale,
}

/// What a started card says about its holder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hold {
    pub state: HoldState,
    pub who: String,
    pub session: String,
}

/// A claim with no process goes stale after this long without a touch.
pub const QUIET: i64 = 12 * 3_600;

pub fn dir() -> PathBuf {
    crate::paths::runtime_dir().join("claims")
}

fn path(dir: &Path, uuid: &str) -> PathBuf {
    dir.join(format!("{uuid}.json"))
}

pub fn write_in(dir: &Path, c: &Claim) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        path(dir, &c.uuid),
        serde_json::to_string(c).unwrap_or_default(),
    )
}

pub fn read_in(dir: &Path, uuid: &str) -> Option<Claim> {
    serde_json::from_str(&std::fs::read_to_string(path(dir, uuid)).ok()?).ok()
}

pub fn remove_in(dir: &Path, uuid: &str) {
    let _ = std::fs::remove_file(path(dir, uuid));
}

pub fn list_in(dir: &Path) -> Vec<Claim> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok())
        .collect()
}

pub fn list() -> Vec<Claim> {
    list_in(&dir())
}

pub fn remove(uuid: &str) {
    remove_in(&dir(), uuid);
}

/// `/proc/<pid>/stat`: the command, the parent and the start time.
fn stat(pid: i32) -> Option<(String, i32, u64)> {
    parse_stat(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// The command sits in parentheses and may itself hold spaces and
/// parentheses, so everything is counted from the last `)`.
fn parse_stat(text: &str) -> Option<(String, i32, u64)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let comm = text.get(open + 1..close)?.to_string();
    let rest: Vec<&str> = text.get(close + 1..)?.split_whitespace().collect();
    // After the command: state, ppid, ... and starttime is the 22nd field.
    Some((
        comm,
        rest.get(1)?.parse().ok()?,
        rest.get(19)?.parse().ok()?,
    ))
}

/// Programs that only pass a command along. The session is above them.
const WRAPPERS: [&str; 16] = [
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "env",
    "timeout",
    "nice",
    "lowprio",
    "sudo",
    "time",
    "xargs",
    "make",
    "just",
    "systemd-run",
    "waybill",
];

/// The process that asked: the nearest ancestor that is not a shell or a
/// wrapper. `None` when the chain runs out (a command typed at a login
/// shell has no session above it).
pub fn holder() -> Option<(i32, u64, String)> {
    #[cfg(unix)]
    // SAFETY: getppid has no failure mode.
    let mut pid = unsafe { libc::getppid() };
    #[cfg(not(unix))]
    let mut pid = 0;
    for _ in 0..32 {
        if pid <= 1 {
            return None;
        }
        let (comm, parent, born) = stat(pid)?;
        if !WRAPPERS.contains(&comm.as_str()) {
            return Some((pid, born, comm));
        }
        pid = parent;
    }
    None
}

/// The branch checked out at `dir`, read from `.git` without running git.
pub fn branch(dir: &Path) -> Option<String> {
    let mut at = Some(dir);
    while let Some(d) = at {
        let dot = d.join(".git");
        if dot.is_dir() {
            return head(&dot.join("HEAD"));
        }
        // A worktree's `.git` is a file that points at its own git folder.
        if let Ok(text) = std::fs::read_to_string(&dot) {
            let gitdir = text.trim().strip_prefix("gitdir:")?.trim();
            return head(&d.join(gitdir).join("HEAD"));
        }
        at = d.parent();
    }
    None
}

fn head(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    text.trim()
        .strip_prefix("ref: refs/heads/")
        .map(String::from)
}

/// Claims `uuid` for the session that ran this command.
pub fn take(uuid: &str, now: i64) -> Claim {
    let cwd = std::env::current_dir().unwrap_or_default();
    let found = holder();
    let folder = cwd
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut who = match &found {
        Some((_, _, comm)) => format!("{comm} in {folder}"),
        None => format!("a shell in {folder}"),
    };
    if let Some(b) = branch(&cwd) {
        who.push_str(&format!(" ({b})"));
    }
    let claim = Claim {
        uuid: uuid.to_string(),
        pid: found.as_ref().map(|f| f.0),
        born: found.as_ref().map(|f| f.1),
        who,
        session: ["WAYBILL_SESSION", "T3CODE_THREAD_ID", "CLAUDE_SESSION_ID"]
            .iter()
            .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
            .unwrap_or_default(),
        cwd: cwd.to_string_lossy().into_owned(),
        since: now,
        touched: now,
    };
    let _ = write_in(&dir(), &claim);
    claim
}

/// Claims `uuid` for a person rather than a process: the board does this
/// when the user starts a task by hand. With no process to watch, the claim
/// lives by its last touch.
pub fn take_as(uuid: &str, who: &str, now: i64) -> Claim {
    let claim = Claim {
        uuid: uuid.to_string(),
        pid: None,
        born: None,
        who: who.to_string(),
        session: String::new(),
        cwd: String::new(),
        since: now,
        touched: now,
    };
    let _ = write_in(&dir(), &claim);
    claim
}

/// Marks the claim as still in use.
pub fn touch(uuid: &str, now: i64) {
    if let Some(mut c) = read_in(&dir(), uuid) {
        c.touched = now;
        let _ = write_in(&dir(), &c);
    }
}

/// Whether the claim's process is still the one that made it.
pub fn alive(c: &Claim, now: i64) -> bool {
    match (c.pid, c.born) {
        (Some(pid), Some(born)) => stat(pid).is_some_and(|(_, _, b)| b == born),
        _ => now - c.touched < QUIET,
    }
}

/// When this machine booted, from `/proc/stat`.
pub fn boot_time() -> Option<i64> {
    std::fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()
}

/// What a started task's card should say, given its claim (and whether the
/// claim's process lives) and when the machine booted.
pub fn judge(started: i64, claim: Option<(&Claim, bool)>, boot: Option<i64>) -> Hold {
    match claim {
        Some((c, live)) => Hold {
            state: if live {
                HoldState::Held
            } else {
                HoldState::Stale
            },
            who: c.who.clone(),
            session: c.session.clone(),
        },
        None => Hold {
            // Nothing that started before the boot can still be running.
            state: if boot.is_some_and(|b| started < b) {
                HoldState::Stale
            } else {
                HoldState::Unclaimed
            },
            who: String::new(),
            session: String::new(),
        },
    }
}

/// Fills in `hold` on every started card.
pub fn apply(snap: &mut Snapshot, claims: &[Claim], boot: Option<i64>) {
    let now = snap.now;
    for card in snap.cards.iter_mut().filter(|c| c.lane == Lane::Doing) {
        let claim = claims.iter().find(|c| c.uuid == card.uuid);
        card.hold = Some(judge(
            card.started.unwrap_or(now),
            claim.map(|c| (c, alive(c, now))),
            boot,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim() -> Claim {
        Claim {
            uuid: "u".into(),
            pid: Some(4242),
            born: Some(99),
            who: "claude in waybill (main)".into(),
            session: "thread-7".into(),
            cwd: "/home/me/waybill".into(),
            since: 1_000,
            touched: 1_000,
        }
    }

    #[test]
    fn reads_a_stat_line_with_a_strange_command() {
        let line = "1521414 (tmux: server (1)) S 1521404 1521404 1521404 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 30545967 1000 200 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 3 0 0";
        assert_eq!(
            parse_stat(line),
            Some(("tmux: server (1)".into(), 1521404, 30545967))
        );
        assert_eq!(parse_stat("garbage"), None);
    }

    #[test]
    fn a_started_task_is_held_unclaimed_or_stale() {
        let c = claim();
        assert_eq!(
            judge(500, Some((&c, true)), Some(100)).state,
            HoldState::Held
        );
        assert_eq!(
            judge(500, Some((&c, false)), Some(100)).state,
            HoldState::Stale
        );
        assert_eq!(judge(500, None, Some(100)).state, HoldState::Unclaimed);
        assert_eq!(
            judge(50, None, Some(100)).state,
            HoldState::Stale,
            "started before the boot"
        );
        assert_eq!(judge(50, None, None).state, HoldState::Unclaimed);
        assert_eq!(
            judge(500, Some((&c, true)), None).who,
            "claude in waybill (main)"
        );
    }

    #[test]
    fn a_claim_without_a_process_lives_by_its_last_touch() {
        let mut c = claim();
        c.pid = None;
        assert!(alive(&c, 1_000 + QUIET - 1));
        assert!(!alive(&c, 1_000 + QUIET));
        // A pid that cannot be this claim's process.
        c.pid = Some(i32::MAX);
        assert!(!alive(&c, 1_001));
    }

    #[test]
    fn claims_round_trip_through_their_folder() {
        let dir = std::env::temp_dir().join(format!("waybill-claims-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(list_in(&dir).is_empty());
        let c = claim();
        write_in(&dir, &c).unwrap();
        assert_eq!(read_in(&dir, "u"), Some(c.clone()));
        assert_eq!(list_in(&dir), vec![c]);
        remove_in(&dir, "u");
        assert!(read_in(&dir, "u").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_the_branch_of_this_checkout() {
        // The crate lives in a git checkout; whatever is checked out, a
        // folder that is not one has no branch.
        assert!(branch(Path::new("/proc")).is_none());
    }
}
