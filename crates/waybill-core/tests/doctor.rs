//! `waybill doctor`'s Taskwarrior checks against a real `task` binary and
//! throwaway databases. Skipped without Taskwarrior 3.

use std::path::Path;
use waybill_core::config::Config;
use waybill_core::doctor::{self, Status};

fn config(dir: &Path, rc: &str) -> Config {
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(
        dir.join("taskrc"),
        format!(
            "data.location={}\nnews.version=3.5.0\n{rc}",
            dir.join("data").display()
        ),
    )
    .unwrap();
    let mut cfg = Config::default();
    cfg.task.command = format!("task rc:{}", dir.join("taskrc").display());
    cfg
}

fn status(checks: &[doctor::Check], what: &str) -> Status {
    checks
        .iter()
        .find(|c| c.what == what)
        .unwrap_or_else(|| panic!("no {what} check in {checks:?}"))
        .status
}

#[test]
fn taskwarrior_checks() {
    // CI's Ubuntu ships Taskwarrior 2, which the doctor rightly fails.
    let major = std::process::Command::new("task")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|v| v.trim().split('.').next()?.parse::<u32>().ok());
    if major.is_none_or(|m| m < 3) {
        eprintln!("skipped: no Taskwarrior 3 binary");
        return;
    }
    let root = std::env::temp_dir().join(format!("waybill-doctor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    // A list set up the way Waybill wants it.
    let good = config(
        &root.join("good"),
        "uda.ref.type=string\nuda.ref.label=Ref\n",
    );
    let checks = doctor::taskwarrior(&good);
    assert!(!doctor::failed(&checks), "{checks:#?}");
    for what in ["taskwarrior", "data location", "task list", "ref field"] {
        assert_eq!(status(&checks, what), Status::Ok, "{what}");
    }
    // No note has been written, so the folder is not there yet.
    assert_eq!(status(&checks, "notes"), Status::Info);

    // No `ref` field: `--ref` would land in the title.
    let bare = config(&root.join("bare"), "");
    let checks = doctor::taskwarrior(&bare);
    assert_eq!(status(&checks, "ref field"), Status::Fail);
    assert_eq!(status(&checks, "task list"), Status::Ok);

    let _ = std::fs::remove_dir_all(&root);
}
