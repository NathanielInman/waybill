//! `waybill`: the command line. Agents work the task list through it
//! (`next`, `visit`, `done`), people ask it questions (`lanes`, `lint`), and
//! waybar runs `waybill bar`.

mod bar;

use anyhow::{Context, bail};
use serde::Serialize;
use std::collections::HashMap;
use waybill_core::act::{self, Change, NewTask};
use waybill_core::agent::{self, Brief, Desk};
use waybill_core::clock::{self, Zone};
use waybill_core::config::Config;
use waybill_core::model::{Lane, Snapshot};

const HELP: &str = "\
waybill: a board for Taskwarrior

Working a task (add --json to any of these for JSON):
  waybill next [--project P] [--peek]   Clear abandoned starts, pick the most urgent
                                        ready task and start it under a claim
  waybill start ID [--take]             Start a named task under a claim
  waybill visit ID TEXT...              Log what happened and what is next
  waybill done ID [TEXT...]             Finish it; prints what that freed
  waybill stop ID [WHY...]              Put it back, unfinished
  waybill show ID                       A task in full: note path, visits, ties
  waybill find [--project P] [--all] WORDS...
                                        Tasks whose title, ref, visits or note
                                        hold every word; --all adds closed ones

Changing the list:
  waybill add [--project P] [--priority H|M|L] [--tag T]... [--ref R]
              [--after ID]... [--note TEXT | --note-file F] [--force] -- TITLE
                                        Add a task; refuses what lint would flag
                                        unless --force
  waybill edit ID [--priority H|M|L] [--tag T]... [--untag T]... [--project P]
              [--title TEXT] [--ref R] [--force]
                                        Change a task; refuses what lint would flag
  waybill ask [ID] [--project P] [--priority P] --option O... [--context TEXT] [-- TITLE]
                                        File a decision for the user, with its options
  waybill rule ID TEXT...               Record the user's ruling on a decision

Looking:
  waybill lanes                         Each lane's count and its first cards
  waybill lint [--strict]               Open tasks that break the house rules
  waybill snapshot                      Everything the board draws, as JSON
  waybill whoami                        Who a claim made here would name

The bar:
  waybill bar                           The waybar module (prints a line per change)
  waybill toggle                        Open or close the Desk drawer

ID is a task's uuid or its first 8 characters. --demo reads a made-up list.
The board itself is `waybill-board`.";

/// Flags that take a value. Everything else starting with `--` is a switch.
const VALUED: [&str; 11] = [
    "--untag",
    "--title",
    "--project",
    "--priority",
    "--tag",
    "--ref",
    "--after",
    "--note",
    "--note-file",
    "--option",
    "--context",
];

struct Args {
    values: HashMap<String, Vec<String>>,
    switches: Vec<String>,
    words: Vec<String>,
}

impl Args {
    fn parse(raw: &[String]) -> anyhow::Result<Args> {
        let mut a = Args {
            values: HashMap::new(),
            switches: vec![],
            words: vec![],
        };
        let mut it = raw.iter();
        while let Some(arg) = it.next() {
            if arg == "--" {
                a.words.extend(it.cloned());
                break;
            } else if VALUED.contains(&arg.as_str()) {
                let v = it.next().with_context(|| format!("{arg} needs a value"))?;
                a.values.entry(arg.clone()).or_default().push(v.clone());
            } else if arg.starts_with("--") {
                a.switches.push(arg.clone());
            } else {
                a.words.push(arg.clone());
            }
        }
        Ok(a)
    }

    fn has(&self, switch: &str) -> bool {
        self.switches.iter().any(|s| s == switch)
    }

    fn one(&self, flag: &str) -> Option<&str> {
        self.values
            .get(flag)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    fn all(&self, flag: &str) -> Vec<String> {
        self.values.get(flag).cloned().unwrap_or_default()
    }

    /// The first word (the task) and the rest joined as text.
    fn id_and_text(&self, what: &str) -> anyhow::Result<(&str, String)> {
        let id = self
            .words
            .first()
            .with_context(|| format!("{what} needs a task: its uuid or first 8 characters"))?;
        Ok((id, self.words[1..].join(" ")))
    }
}

fn print<T: Serialize>(json: bool, value: &T, human: impl FnOnce()) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        human();
    }
    Ok(())
}

fn show_brief(b: &Brief) {
    println!("{}", b.title);
    println!("  uuid      {}", b.uuid);
    let mut line = format!(
        "  project   {}",
        if b.project.is_empty() {
            "(none)"
        } else {
            &b.project
        }
    );
    if !b.priority.is_empty() {
        line.push_str(&format!("   priority {}", b.priority));
    }
    if let Some(r) = &b.reference {
        line.push_str(&format!("   ref {r}"));
    }
    println!("{line}");
    let mut tags = b.tags.clone();
    if b.decision {
        tags.push("decision".into());
    }
    if b.yours {
        tags.push("needs the user".into());
    }
    if !tags.is_empty() {
        println!("  tags      {}", tags.join(", "));
    }
    if let Some(note) = &b.note {
        println!(
            "  note      {note}{}",
            if b.has_note {
                " (read it first)"
            } else {
                " (not written yet)"
            }
        );
    }
    if let Some(h) = &b.hold {
        let who = if h.who.is_empty() {
            "nobody on record"
        } else {
            &h.who
        };
        println!("  held by   {who}");
    }
    for (label, list) in [("frees", &b.frees), ("waits on", &b.waits_on)] {
        for (i, n) in list.iter().enumerate() {
            println!(
                "  {:<9} {} {}",
                if i == 0 { label } else { "" },
                &n.uuid[..8],
                n.title
            );
        }
    }
    for v in b.visits.iter().rev().take(5).rev() {
        println!("  visit     {}", v.text);
    }
}

fn snapshot(demo: bool) -> Snapshot {
    let now = clock::now();
    if demo {
        return waybill_core::demo::snapshot(now, Zone::Local);
    }
    let desk = desk();
    waybill_core::load::snapshot(&desk.cfg, desk.notes.as_deref(), now, Zone::Local)
}

fn desk() -> Desk {
    let (cfg, problem) = Config::load();
    if let Some(p) = problem {
        eprintln!("waybill: config ignored: {p}");
    }
    Desk::open(cfg)
}

fn priority(a: &Args) -> anyhow::Result<Option<char>> {
    match a.one("--priority") {
        None => Ok(None),
        Some(p @ ("H" | "M" | "L")) => Ok(p.chars().next()),
        Some(p) => bail!("--priority is H, M or L, not {p:?}"),
    }
}

fn run() -> anyhow::Result<()> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = raw.first().cloned() else {
        println!("{HELP}");
        return Ok(());
    };
    let a = Args::parse(&raw[1..])?;
    let (json, demo, now) = (a.has("--json"), a.has("--demo"), clock::now());
    match command.as_str() {
        "next" => {
            let n = agent::next(&desk(), a.one("--project"), a.has("--peek"), now)?;
            print(json, &n, || {
                for c in &n.cleared {
                    println!(
                        "Stopped an abandoned start: {} {} ({})",
                        &c.uuid[..8],
                        c.title,
                        c.why
                    );
                    println!("  Check its repo for leftover work before resuming it.");
                }
                for l in &n.long_running {
                    println!(
                        "Started over 12 hours ago, left alone: {} {}",
                        &l.uuid[..8],
                        l.title
                    );
                }
                match &n.task {
                    Some(b) => {
                        println!(
                            "{}",
                            if n.started {
                                "Started:"
                            } else {
                                "Next up (not started):"
                            }
                        );
                        show_brief(b);
                        println!("{} more ready", n.ready);
                    }
                    None => println!("Nothing is ready."),
                }
            })
        }
        "start" => {
            let (id, _) = a.id_and_text("start")?;
            let b = agent::start(&desk(), id, a.has("--take"), now)?;
            print(json, &b, || {
                println!("Started:");
                show_brief(&b);
            })
        }
        "show" => {
            let (id, _) = a.id_and_text("show")?;
            let desk = desk();
            let snap = desk.snapshot(now)?;
            let b = desk.brief(&snap, agent::resolve(&snap, id)?);
            print(json, &b, || show_brief(&b))
        }
        "find" => {
            let desk = desk();
            if a.words.is_empty() {
                bail!("find needs some words to look for");
            }
            let found = agent::find(&desk, &a.words, a.one("--project"), a.has("--all"), now)?;
            print(json, &found, || {
                if found.is_empty() {
                    println!("Nothing matches");
                }
                for f in &found {
                    let lane = match (f.status, f.lane) {
                        ("done", _) => "done".to_string(),
                        (_, Some(l)) => format!("{l:?}").to_lowercase(),
                        _ => "open".to_string(),
                    };
                    println!(
                        "{} {:<14} {:<7} {}",
                        &f.uuid[..8.min(f.uuid.len())],
                        f.project,
                        lane,
                        f.title
                    );
                    println!("         in {}", f.matched.join(", "));
                }
            })
        }
        "visit" => {
            let (id, text) = a.id_and_text("visit")?;
            let n = agent::visit(&desk(), id, &text, now)?;
            print(json, &n, || {
                println!("Logged on {} {}", &n.uuid[..8], n.title)
            })
        }
        "stop" => {
            let (id, why) = a.id_and_text("stop")?;
            let n = agent::stop(&desk(), id, Some(&why), now)?;
            print(json, &n, || {
                println!("Stopped {} {}", &n.uuid[..8], n.title)
            })
        }
        "done" => {
            let (id, text) = a.id_and_text("done")?;
            let c = agent::done(&desk(), id, Some(&text), now)?;
            print(json, &c, || {
                println!("Finished {} {}", &c.task.uuid[..8], c.task.title);
                for f in &c.freed {
                    println!("  freed {} {}", &f.uuid[..8], f.title);
                }
            })
        }
        "rule" => {
            let (id, text) = a.id_and_text("rule")?;
            let n = agent::rule(&desk(), id, &text, now)?;
            print(json, &n, || {
                println!("Ruled on {} {}", &n.uuid[..8], n.title)
            })
        }
        "edit" => {
            let (id, rest) = a.id_and_text("edit")?;
            if !rest.is_empty() {
                bail!("edit takes flags, not words: did you mean --title?");
            }
            let mut changes = Vec::new();
            if let Some(p) = priority(&a)? {
                changes.push(Change::Priority(p));
            }
            changes.extend(a.all("--tag").into_iter().map(Change::AddTag));
            changes.extend(a.all("--untag").into_iter().map(Change::DropTag));
            if let Some(p) = a.one("--project") {
                changes.push(Change::Project(p.to_string()));
            }
            if let Some(t) = a.one("--title") {
                changes.push(Change::Title(t.to_string()));
            }
            if let Some(r) = a.one("--ref") {
                changes.push(Change::Ref(r.to_string()));
            }
            let e = agent::edit(&desk(), id, &changes, a.has("--force"), now)?;
            print(json, &e, || {
                println!("Edited {} {}", &e.task.uuid[..8], e.task.title);
                println!("  {}", e.changes.join(", "));
            })
        }
        "add" => {
            let desk = desk();
            let title = a.words.join(" ");
            let snap = desk.snapshot(now)?;
            let after = a
                .all("--after")
                .iter()
                .map(|id| agent::resolve(&snap, id).map(|c| c.uuid.clone()))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let new = NewTask {
                title: title.clone(),
                project: a.one("--project").map(String::from),
                priority: priority(&a)?.or(Some('M')),
                tags: a.all("--tag"),
                reference: a.one("--ref").map(String::from),
                after,
            };
            house_rules(&new, &desk.cfg, a.has("--force"))?;
            let uuid = act::add(&desk.cfg, &new)?;
            let note = match (a.one("--note"), a.one("--note-file")) {
                (Some(text), _) => Some(text.to_string()),
                (None, Some(file)) => Some(
                    std::fs::read_to_string(file)
                        .with_context(|| format!("could not read {file}"))?,
                ),
                _ => None,
            };
            if let (Some(text), Some(dir)) = (&note, &desk.notes) {
                // A note that does not open with a heading gets the title.
                let text = if text.trim_start().starts_with('#') {
                    text.clone()
                } else {
                    format!("# {}\n\n{text}", title.trim())
                };
                act::write_note(dir, &uuid, &text)?;
            }
            let path = desk
                .notes
                .as_ref()
                .map(|d| act::note_path(d, &uuid).to_string_lossy().into_owned());
            print(
                json,
                &serde_json::json!({ "uuid": uuid, "title": title.trim(), "note": path }),
                || {
                    println!("Added {uuid}");
                    if let Some(p) = &path {
                        println!(
                            "  note  {p}{}",
                            if note.is_some() {
                                ""
                            } else {
                                " (not written yet)"
                            }
                        );
                    }
                },
            )
        }
        "ask" => {
            let desk = desk();
            let options = a.all("--option");
            if options.len() < 2 {
                bail!("a decision needs at least two --option choices");
            }
            let existing = a
                .words
                .first()
                .filter(|w| w.len() >= 6 && w.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'));
            let asked = match existing {
                Some(id) if a.words.len() == 1 => {
                    agent::ask_about(&desk, id, &options, a.one("--context"), now)?
                }
                _ => {
                    let title = a.words.join(" ");
                    if title.trim().is_empty() {
                        bail!("give the decision a title after --, or name an existing task");
                    }
                    let new = NewTask {
                        title,
                        project: a.one("--project").map(String::from),
                        priority: priority(&a)?.or(Some('M')),
                        tags: a.all("--tag"),
                        ..Default::default()
                    };
                    house_rules(&new, &desk.cfg, a.has("--force"))?;
                    agent::ask_new(&desk, new, &options, a.one("--context"))?
                }
            };
            print(json, &asked, || {
                println!("Asked {}: {}", desk.you(), asked.title);
                println!("  uuid  {}", asked.uuid);
                for o in &asked.options {
                    println!("  - {o}");
                }
            })
        }
        "lint" => {
            let desk = desk();
            let tasks = waybill_core::load::export(&desk.cfg)?;
            let found = waybill_core::lint::check(&tasks, &desk.cfg);
            print(json, &found, || {
                for f in &found {
                    println!(
                        "{} {:<16} {}",
                        &f.uuid[..8.min(f.uuid.len())],
                        f.rule,
                        f.title
                    );
                    println!("         {}", f.message);
                }
                println!(
                    "{} finding{}",
                    found.len(),
                    if found.len() == 1 { "" } else { "s" }
                );
            })?;
            if a.has("--strict") && !found.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        "snapshot" => {
            let s = snapshot(demo);
            println!("{}", serde_json::to_string(&s)?);
            match s.error {
                Some(e) => bail!(e),
                None => Ok(()),
            }
        }
        "lanes" => {
            let s = snapshot(demo);
            if let Some(e) = &s.error {
                bail!(e.clone());
            }
            for (lane, name) in [
                (Lane::Ready, "Ready for agents"),
                (Lane::Doing, "Doing"),
                (Lane::Yours, "Needs you"),
                (Lane::Held, "Held"),
                (Lane::Parked, "Parked"),
                (Lane::Done, "Done today"),
            ] {
                let cards = s.lane(lane);
                println!("{name}: {}", cards.len());
                for c in cards.iter().take(5) {
                    let frees = match c.frees.len() {
                        0 => String::new(),
                        n => format!("  frees {n}"),
                    };
                    println!("  {} {:<12} {}{frees}", c.short(), c.project, c.title);
                }
            }
            Ok(())
        }
        "whoami" => {
            let found = waybill_core::claim::holder();
            let cwd = std::env::current_dir().unwrap_or_default();
            match &found {
                Some((pid, _, comm)) => println!("{comm} (pid {pid}) in {}", cwd.display()),
                None => println!("a shell with no session above it, in {}", cwd.display()),
            }
            if let Some(b) = waybill_core::claim::branch(&cwd) {
                println!("branch {b}");
            }
            Ok(())
        }
        "bar" => bar::run(demo),
        "toggle" => bar::toggle(),
        "--version" | "-V" => {
            println!("waybill {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--help" | "-h" | "help" => {
            println!("{HELP}");
            Ok(())
        }
        other => bail!("unknown command {other:?}; try waybill --help"),
    }
}

/// Refuses a task that breaks the house rules `waybill lint` checks, so a
/// bad card never reaches the list. `--force` files it anyway.
fn house_rules(
    new: &NewTask,
    cfg: &waybill_core::config::Config,
    force: bool,
) -> anyhow::Result<()> {
    let problems = waybill_core::lint::new_task(new, cfg);
    if problems.is_empty() || force {
        return Ok(());
    }
    let all: Vec<String> = problems.into_iter().map(|p| p.1).collect();
    bail!("{}; pass --force to add it anyway", all.join("; "))
}

fn main() {
    // `waybill lanes | head` should end quietly, not with a panic.
    #[cfg(unix)]
    // SAFETY: restoring a signal's default action before any thread starts.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    if let Err(e) = run() {
        eprintln!("waybill: {e:#}");
        std::process::exit(1);
    }
}
