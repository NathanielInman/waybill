//! The whole protocol against a real `task` binary and a throwaway
//! database. Skipped when Taskwarrior is not installed.

use std::path::Path;
use waybill_core::act::{self, Change, NewTask};
use waybill_core::agent::{self, Desk};
use waybill_core::claim::{self, Claim, HoldState};
use waybill_core::clock::{self, Zone};
use waybill_core::config::Config;
use waybill_core::model::{Kind, Lane};

fn desk(dir: &Path) -> Desk {
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(
        dir.join("taskrc"),
        format!(
            "data.location={}\nuda.ref.type=string\nuda.ref.label=Ref\nnews.version=3.5.0\n",
            dir.join("data").display()
        ),
    )
    .unwrap();
    let mut cfg = Config::default();
    cfg.task.command = format!("task rc:{}", dir.join("taskrc").display());
    Desk {
        notes: Some(dir.join("data/notes")),
        cfg,
        zone: Zone::Local,
    }
}

#[test]
fn an_agent_and_a_person_work_a_list() {
    if std::process::Command::new("task")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipped: no task binary");
        return;
    }
    let dir = std::env::temp_dir().join(format!("waybill-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // Claims go to a throwaway folder too.
    // SAFETY: this is the only test in the binary, so nothing else reads the
    // environment while it changes.
    unsafe { std::env::set_var("WAYBILL_RUNTIME_DIR", dir.join("run")) };
    let desk = desk(&dir);
    let cfg = &desk.cfg;
    let now = clock::now();

    let add = |title: &str, tags: &[&str], after: &[&str], priority: char| {
        act::add(
            cfg,
            &NewTask {
                title: title.into(),
                project: Some("storefront".into()),
                priority: Some(priority),
                tags: tags.iter().map(|t| t.to_string()).collect(),
                reference: Some("cart".into()),
                after: after.iter().map(|u| u.to_string()).collect(),
            },
        )
        .unwrap()
    };
    let play = add("Playtest the cart", &["me"], &[], 'H');
    let build = add("Build the receipt", &[], &[&play], 'M');
    let free = add("Fix a typo: it's wrong -- everywhere", &[], &[], 'L');
    assert!(act::is_uuid(&play) && play != build);

    let snap = desk.snapshot(now).unwrap();
    assert_eq!(snap.card(&play).unwrap().lane, Lane::Yours);
    assert_eq!(
        snap.card(&play).unwrap().frees,
        std::slice::from_ref(&build)
    );
    assert_eq!(snap.card(&build).unwrap().lane, Lane::Held);
    let typo = snap.card(&free).unwrap();
    assert_eq!(typo.title, "Fix a typo: it's wrong -- everywhere");
    assert_eq!(typo.reference.as_deref(), Some("cart"));

    // Looking first changes nothing.
    let peek = agent::next(&desk, None, true, now).unwrap();
    assert_eq!(peek.task.as_ref().unwrap().uuid, free);
    assert!(!peek.started);
    assert_eq!(
        desk.snapshot(now).unwrap().card(&free).unwrap().lane,
        Lane::Ready
    );

    // The agent picks the only ready task, and it is held under a claim.
    let next = agent::next(&desk, Some("storefront"), false, now).unwrap();
    let brief = next.task.unwrap();
    assert_eq!(
        (brief.uuid.as_str(), next.started, next.ready),
        (free.as_str(), true, 0)
    );
    assert!(
        brief
            .note
            .as_deref()
            .unwrap()
            .ends_with(&format!("{free}.md"))
    );
    let snap = desk.snapshot(now).unwrap();
    let held = snap.card(&free).unwrap();
    assert_eq!(held.lane, Lane::Doing);
    assert_eq!(held.hold.as_ref().unwrap().state, HoldState::Held);
    assert!(
        agent::next(&desk, None, false, now).unwrap().task.is_none(),
        "nothing else is ready"
    );
    assert!(
        agent::start(&desk, &free[..8], false, now).is_err(),
        "a held task is not taken by accident"
    );

    agent::visit(&desk, &free[..8], "fixed in a1b2c3d;\n  docs next", now).unwrap();
    let closed = agent::done(&desk, &free, Some("done (a1b2c3d)"), now).unwrap();
    assert!(closed.freed.is_empty());
    let snap = desk.snapshot(now).unwrap();
    let finished = snap.card(&free).unwrap();
    assert_eq!(finished.lane, Lane::Done);
    assert_eq!(finished.visits.len(), 2);
    assert_eq!(finished.visits[0].text, "fixed in a1b2c3d; docs next");
    assert!(claim::list().is_empty(), "finishing drops the claim");

    // The person finishes their playtest, which frees the build.
    let closed = agent::done(&desk, &play, None, now).unwrap();
    assert_eq!(closed.freed.len(), 1);
    assert_eq!(closed.freed[0].uuid, build);
    assert_eq!(
        desk.snapshot(now).unwrap().card(&build).unwrap().lane,
        Lane::Ready
    );

    // A start whose session is gone is cleared by the next pick.
    act::start(cfg, &build).unwrap();
    assert_eq!(
        desk.snapshot(now)
            .unwrap()
            .card(&build)
            .unwrap()
            .hold
            .as_ref()
            .unwrap()
            .state,
        HoldState::Unclaimed
    );
    claim::write_in(
        &claim::dir(),
        &Claim {
            uuid: build.clone(),
            pid: Some(i32::MAX),
            born: Some(1),
            who: "claude in storefront (main)".into(),
            session: String::new(),
            cwd: String::new(),
            since: now,
            touched: now,
        },
    )
    .unwrap();
    let next = agent::next(&desk, None, false, now).unwrap();
    assert_eq!(next.cleared.len(), 1);
    assert!(next.cleared[0].why.contains("claude in storefront"));
    assert!(
        next.task.is_none(),
        "the cleared task waits for a look at its repo"
    );
    let snap = desk.snapshot(now).unwrap();
    let cleared = snap.card(&build).unwrap();
    assert_eq!(cleared.lane, Lane::Ready);
    assert!(
        cleared
            .visits
            .last()
            .unwrap()
            .text
            .starts_with("stopped: the session")
    );

    // An agent asks; the person rules; the task goes back to the agents.
    let asked = agent::ask_new(
        &desk,
        NewTask {
            title: "Pick the launch discount".into(),
            project: Some("storefront".into()),
            priority: Some('M'),
            ..Default::default()
        },
        &["10% for a week".into(), "No discount".into()],
        Some("Launch is Friday."),
    )
    .unwrap();
    let note = act::read_note(desk.notes.as_ref().unwrap(), &asked.uuid).unwrap();
    assert_eq!(
        waybill_core::note::options(&note),
        ["10% for a week", "No discount"]
    );
    let snap = desk.snapshot(now).unwrap();
    let card = snap.card(&asked.uuid).unwrap();
    assert!(card.decision && card.yours && card.has_note);
    agent::rule(&desk, &asked.uuid, "10% for a week", now).unwrap();
    let snap = desk.snapshot(now).unwrap();
    let card = snap.card(&asked.uuid).unwrap();
    assert!(!card.decision && !card.yours);
    assert_eq!(card.lane, Lane::Ready);
    assert_eq!(card.visits[0].text, "Me: 10% for a week");
    assert!(
        snap.log
            .iter()
            .any(|e| e.uuid == asked.uuid && e.kind == Kind::Ruling)
    );
    assert!(
        act::read_note(desk.notes.as_ref().unwrap(), &asked.uuid)
            .unwrap()
            .contains("## Decided 2")
    );

    // Changes by hand: priority, a tag, a wait.
    act::modify(
        cfg,
        &build,
        &[
            Change::Priority('H'),
            Change::AddTag("me".into()),
            Change::Wait("someday".into()),
        ],
    )
    .unwrap();
    let snap = desk.snapshot(now).unwrap();
    let card = snap.card(&build).unwrap();
    assert_eq!(
        (card.priority, card.yours, card.lane),
        ('H', true, Lane::Parked)
    );
    act::modify(
        cfg,
        &build,
        &[Change::Wait(String::new()), Change::DropTag("me".into())],
    )
    .unwrap();
    assert_eq!(
        desk.snapshot(now).unwrap().card(&build).unwrap().lane,
        Lane::Ready
    );

    let _ = std::fs::remove_dir_all(&dir);
}
