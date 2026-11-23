//! `waybill doctor`: is everything Waybill leans on in place? Taskwarrior 3
//! and its database, the `ref` field, the config, the notes folder, the
//! waybar module, the drawer's library and the board's launcher entry.
//!
//! Each check is one line: `ok`, `fail` (something Waybill needs is
//! missing or broken) or `info` (optional, or only worth knowing). The CLI
//! adds the checks that need the compositor and prints them.

use crate::config::Config;
use crate::paths::tilde;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Fail,
    Info,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub status: Status,
    pub what: String,
    pub detail: String,
}

impl Check {
    pub fn new(status: Status, what: &str, detail: impl Into<String>) -> Check {
        Check {
            status,
            what: what.into(),
            detail: detail.into(),
        }
    }

    pub fn ok(what: &str, detail: impl Into<String>) -> Check {
        Check::new(Status::Ok, what, detail)
    }

    pub fn fail(what: &str, detail: impl Into<String>) -> Check {
        Check::new(Status::Fail, what, detail)
    }

    pub fn info(what: &str, detail: impl Into<String>) -> Check {
        Check::new(Status::Info, what, detail)
    }

    /// `ok   what: detail`, the way the sibling apps' doctors print.
    pub fn line(&self) -> String {
        let tag = match self.status {
            Status::Ok => "ok  ",
            Status::Fail => "fail",
            Status::Info => "info",
        };
        if self.detail.is_empty() {
            format!("{tag} {}", self.what)
        } else {
            format!("{tag} {}: {}", self.what, self.detail)
        }
    }
}

/// Any check failed: `waybill doctor` exits 1.
pub fn failed(checks: &[Check]) -> bool {
    checks.iter().any(|c| c.status == Status::Fail)
}

/// The config, loaded the way every command loads it, and what to say
/// about it.
pub fn config() -> (Config, Check) {
    let (cfg, err) = Config::load();
    let file = crate::paths::config_file();
    let check = match err {
        Some(e) => Check::fail(
            "config",
            format!("{}: {e}; the defaults are used", tilde(&file)),
        ),
        None if file.exists() => Check::ok("config", tilde(&file)),
        None => Check::info("config", "no config file, using defaults"),
    };
    (cfg, check)
}

/// `3.5.0` from what `task _version` prints.
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let word = text.split_whitespace().next()?;
    let mut parts = word.split('.').map(|p| {
        let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u32>().ok()
    });
    Some((
        parts.next()??,
        parts.next().flatten()?,
        parts.next().flatten().unwrap_or(0),
    ))
}

/// `task _get <name>`, through `task.command`. Empty when unset.
fn get(cfg: &Config, name: &str) -> anyhow::Result<String> {
    let mut c = crate::load::command(cfg)?;
    c.args(["_get", name]);
    Ok(crate::load::run(c)?.trim().to_string())
}

/// Taskwarrior and its database: the binary and its version, the data
/// location, an export, the `ref` field and the notes folder. Everything
/// here reads; nothing changes the list.
pub fn taskwarrior(cfg: &Config) -> Vec<Check> {
    let mut out = vec![];
    let version = crate::load::command(cfg).and_then(|mut c| {
        c.arg("_version");
        crate::load::run(c)
    });
    let program = cfg.task.command.split_whitespace().next().unwrap_or("");
    match version.map(|v| (parse_version(&v), v)) {
        Ok((Some(v @ (3.., _, _)), _)) => out.push(Check::ok(
            "taskwarrior",
            format!("{program} {}.{}.{}", v.0, v.1, v.2),
        )),
        Ok((Some(v), _)) => out.push(Check::fail(
            "taskwarrior",
            format!(
                "{program} is {}.{}.{}; Waybill reads Taskwarrior 3",
                v.0, v.1, v.2
            ),
        )),
        Ok((None, text)) => out.push(Check::fail(
            "taskwarrior",
            format!("{program} _version printed {:?}", text.trim()),
        )),
        Err(e) => {
            out.push(Check::fail(
                "taskwarrior",
                format!("{e:#} (task.command is {:?})", cfg.task.command),
            ));
            return out;
        }
    }

    match get(cfg, "rc.data.location") {
        Ok(d) if d.is_empty() => out.push(Check::fail(
            "data location",
            "data.location is not set in the taskrc",
        )),
        Ok(d) => {
            let dir = crate::paths::expand(&d);
            match std::fs::read_dir(&dir) {
                Ok(_) => out.push(Check::ok("data location", tilde(&dir))),
                Err(e) => out.push(Check::fail(
                    "data location",
                    format!("{}: {e}", tilde(&dir)),
                )),
            }
        }
        Err(e) => out.push(Check::fail("data location", format!("{e:#}"))),
    }

    match crate::load::export(cfg) {
        Ok(tasks) => {
            let open = tasks.iter().filter(|t| t.open()).count();
            out.push(Check::ok(
                "task list",
                format!("{} tasks, {open} open", tasks.len()),
            ));
        }
        Err(e) => out.push(Check::fail("task list", format!("{e:#}"))),
    }

    match get(cfg, "rc.uda.ref.type") {
        Ok(t) if t == "string" => out.push(Check::ok("ref field", "uda.ref.type=string")),
        Ok(t) if t.is_empty() => out.push(Check::fail(
            "ref field",
            "no `ref` UDA, so --ref ends up in the title; add uda.ref.type=string to the taskrc",
        )),
        Ok(t) => out.push(Check::fail(
            "ref field",
            format!("uda.ref.type is {t}; Waybill writes a string"),
        )),
        Err(e) => out.push(Check::fail("ref field", format!("{e:#}"))),
    }

    match crate::load::notes_dir(cfg) {
        Some(dir) if dir.is_dir() => {
            let n = crate::load::noted(&dir).len();
            out.push(Check::ok(
                "notes",
                format!("{}, {n} note{}", tilde(&dir), if n == 1 { "" } else { "s" }),
            ));
        }
        Some(dir) => out.push(Check::info(
            "notes",
            format!(
                "{} does not exist yet; the first note makes it",
                tilde(&dir)
            ),
        )),
        None => out.push(Check::fail(
            "notes",
            "no folder: set notes.dir or data.location",
        )),
    }
    out
}

/// `config.jsonc`, then `config`, as waybar looks for them.
pub fn waybar_config() -> Option<PathBuf> {
    let dir = crate::paths::waybar_dir();
    ["config.jsonc", "config"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.exists())
}

/// Whether a waybar config defines `custom/waybill` and runs `waybill bar`.
pub fn module_in(text: &str) -> (bool, bool) {
    (
        text.contains("\"custom/waybill\""),
        text.contains("waybill bar"),
    )
}

/// The command lines of running processes, from `/proc`.
pub fn processes(proc_root: &Path) -> Vec<Vec<String>> {
    std::fs::read_dir(proc_root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter_map(|e| std::fs::read(e.path().join("cmdline")).ok())
        .map(|raw| {
            raw.split(|b| *b == 0)
                .filter(|a| !a.is_empty())
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect::<Vec<_>>()
        })
        .filter(|argv| !argv.is_empty())
        .collect()
}

fn program_is(argv: &[String], name: &str) -> bool {
    argv.first()
        .and_then(|a| Path::new(a).file_name())
        .is_some_and(|n| n == name)
}

/// What is running: waybar, and the `waybill bar` module under it.
pub fn running(procs: &[Vec<String>]) -> (bool, bool) {
    (
        procs.iter().any(|a| program_is(a, "waybar")),
        procs
            .iter()
            .any(|a| program_is(a, "waybill") && a.get(1).is_some_and(|w| w == "bar")),
    )
}

/// The waybar module: in the config, and running.
pub fn waybar() -> Vec<Check> {
    let mut out = vec![];
    match waybar_config() {
        Some(p) => {
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            let detail = match module_in(&text) {
                (true, true) => "has `custom/waybill` running `waybill bar`",
                (true, false) => "`custom/waybill` does not run `waybill bar`",
                (false, _) => "no `custom/waybill` module",
            };
            out.push(Check::new(
                if module_in(&text) == (true, true) {
                    Status::Ok
                } else {
                    Status::Fail
                },
                "waybar config",
                format!("{}: {detail}", tilde(&p)),
            ));
        }
        None => out.push(Check::info("waybar config", "not found")),
    }
    let (bar, module) = running(&processes(Path::new("/proc")));
    out.push(match (bar, module) {
        (true, true) => Check::ok("waybar", "running, with `waybill bar` under it"),
        (true, false) => Check::fail(
            "waybar",
            "running, but no `waybill bar` is; reload waybar (SIGUSR2)",
        ),
        (false, _) => Check::info("waybar", "not running; the lamp shows when it is"),
    });
    out
}

/// Directories the dynamic linker looks in for a library.
fn lib_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("LD_LIBRARY_PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    for d in [
        "/usr/lib",
        "/usr/lib64",
        "/lib",
        "/lib64",
        "/usr/local/lib",
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
    ] {
        dirs.push(PathBuf::from(d));
    }
    dirs
}

/// The first `<stem>.so*` in the linker's directories.
pub fn find_lib(stem: &str) -> Option<PathBuf> {
    lib_dirs().into_iter().find_map(|d| {
        std::fs::read_dir(&d)
            .into_iter()
            .flatten()
            .flatten()
            .find(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with(&format!("{stem}.so")))
            })
            .map(|e| e.path())
    })
}

/// The board's libraries: GTK 4, and gtk4-layer-shell for the drawer.
pub fn libraries() -> Vec<Check> {
    [
        ("GTK 4", "libgtk-4", "the board and the drawer"),
        (
            "gtk4-layer-shell",
            "libgtk4-layer-shell",
            "the drawer opens over the bar",
        ),
    ]
    .into_iter()
    .map(|(what, stem, why)| match find_lib(stem) {
        Some(p) => Check::ok(what, format!("{} ({why})", p.display())),
        None => Check::fail(what, format!("{stem}.so not found; {why} needs it")),
    })
    .collect()
}

pub const DESKTOP_FILE: &str = "io.github.nathanielinman.Waybill.desktop";

/// The board's launcher entry, where desktop launchers look.
pub fn desktop_entry() -> Check {
    let home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::paths::home().join(".local/share"));
    let dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let found = std::iter::once(home)
        .chain(dirs.split(':').map(PathBuf::from))
        .map(|d| d.join("applications").join(DESKTOP_FILE))
        .find(|p| p.exists());
    match found {
        Some(p) => Check::ok("launcher", tilde(&p)),
        None => Check::info(
            "launcher",
            format!("no {DESKTOP_FILE}; install it from packaging/ to list the board"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("3.5.0\n"), Some((3, 5, 0)));
        assert_eq!(parse_version("2.6.2"), Some((2, 6, 2)));
        assert_eq!(parse_version("3.4"), Some((3, 4, 0)));
        assert_eq!(parse_version("3.0.0-beta"), Some((3, 0, 0)));
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("task"), None);
    }

    #[test]
    fn lines_match_the_siblings() {
        assert_eq!(Check::ok("config", "~/x").line(), "ok   config: ~/x");
        assert_eq!(Check::fail("waybar", "").line(), "fail waybar");
        assert_eq!(Check::info("notes", "none").line(), "info notes: none");
        assert!(!failed(&[Check::ok("a", ""), Check::info("b", "")]));
        assert!(failed(&[Check::ok("a", ""), Check::fail("b", "")]));
        assert_eq!(
            serde_json::to_string(&Check::info("a", "b")).unwrap(),
            r#"{"status":"info","what":"a","detail":"b"}"#
        );
    }

    #[test]
    fn the_module_in_a_waybar_config() {
        let text = r#"{
          "modules-right": ["custom/waybill", "clock"],
          "custom/waybill": { "exec": "~/.local/bin/waybill bar" }
        }"#;
        assert_eq!(module_in(text), (true, true));
        assert_eq!(
            module_in(r#""custom/waybill": { "exec": "waybill-board" }"#),
            (true, false)
        );
        assert_eq!(module_in(r#"{"modules-right": ["clock"]}"#), (false, false));
    }

    #[test]
    fn what_is_running() {
        let argv = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
        let procs = vec![
            argv("/usr/bin/waybar"),
            argv("/home/me/.local/bin/waybill bar"),
            argv("/home/me/.local/bin/waybill-board --drawer"),
        ];
        assert_eq!(running(&procs), (true, true));
        assert_eq!(running(&procs[..1]), (true, false));
        // The board is not the module, and `waybill bar` alone is no bar.
        assert_eq!(running(&procs[2..]), (false, false));
        assert_eq!(running(&procs[1..2]), (false, true));
    }

    #[test]
    fn this_process_is_in_proc() {
        let procs = processes(Path::new("/proc"));
        if procs.is_empty() {
            return; // no /proc here
        }
        let me = std::env::current_exe().unwrap();
        let name = me.file_name().unwrap().to_str().unwrap();
        assert!(procs.iter().any(|a| program_is(a, name)));
    }

    #[test]
    fn a_missing_task_binary_fails_once() {
        let mut cfg = Config::default();
        cfg.task.command = "/nonexistent/waybill-doctor-task".into();
        let checks = taskwarrior(&cfg);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].status, Status::Fail);
        assert_eq!(checks[0].what, "taskwarrior");
    }
}
