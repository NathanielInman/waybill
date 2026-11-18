//! `waybill`: the command line. Today it reads; the commands that write
//! (`next`, `visit`, `done`) come with the agent protocol.

use waybill_core::clock::{self, Zone};
use waybill_core::config::Config;
use waybill_core::model::Lane;

const HELP: &str = "\
waybill: a board for Taskwarrior

Usage:
  waybill snapshot [--demo]   Print everything the board draws, as JSON
  waybill lanes [--demo]      Print each lane's count and its first cards
  waybill --version

The board itself is `waybill-board`.";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let demo = args.iter().any(|a| a == "--demo");
    let snapshot = || {
        let now = clock::now();
        if demo {
            return waybill_core::demo::snapshot(now, Zone::Local);
        }
        let (cfg, problem) = Config::load();
        if let Some(p) = problem {
            eprintln!("waybill: config ignored: {p}");
        }
        let notes = waybill_core::load::notes_dir(&cfg);
        waybill_core::load::snapshot(&cfg, notes.as_deref(), now, Zone::Local)
    };
    match args.first().map(String::as_str) {
        Some("snapshot") => {
            let s = snapshot();
            println!("{}", serde_json::to_string(&s)?);
            if let Some(e) = s.error {
                anyhow::bail!(e);
            }
        }
        Some("lanes") => {
            let s = snapshot();
            if let Some(e) = &s.error {
                anyhow::bail!(e.clone());
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
        }
        Some("--version" | "-V") => println!("waybill {}", env!("CARGO_PKG_VERSION")),
        Some("--help" | "-h") | None => println!("{HELP}"),
        Some(other) => anyhow::bail!("unknown command {other:?}; try waybill --help"),
    }
    Ok(())
}
