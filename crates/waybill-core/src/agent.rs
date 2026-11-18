//! The pick-up protocol, as functions: find the next task, start it under a
//! claim, log visits, finish it and say what that freed. The command line is
//! a thin layer over these, and the board uses the same ones, so an agent
//! and a click do exactly the same thing to the list.

use crate::act::{self, Change, NewTask};
use crate::claim::{self, Hold, HoldState};
use crate::clock::{HOUR, Zone, civil_from_days};
use crate::config::Config;
use crate::model::{Card, Lane, Snapshot, Visit};
use anyhow::{Context, bail};
use serde::Serialize;
use std::path::PathBuf;

/// A config and where its notes live.
pub struct Desk {
    pub cfg: Config,
    pub notes: Option<PathBuf>,
    pub zone: Zone,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Named {
    pub uuid: String,
    pub title: String,
    pub project: String,
}

impl From<&Card> for Named {
    fn from(c: &Card) -> Named {
        Named {
            uuid: c.uuid.clone(),
            title: c.title.clone(),
            project: c.project.clone(),
        }
    }
}

/// Everything a session needs to begin work on a task.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Brief {
    pub uuid: String,
    pub title: String,
    pub project: String,
    pub priority: String,
    pub tags: Vec<String>,
    pub yours: bool,
    pub decision: bool,
    pub lane: Lane,
    /// The inline marker in code or docs that points at this task.
    pub reference: Option<String>,
    /// The note's path. Read it before touching code.
    pub note: Option<String>,
    pub has_note: bool,
    pub visits: Vec<Visit>,
    pub frees: Vec<Named>,
    pub waits_on: Vec<Named>,
    pub hold: Option<Hold>,
}

/// A start that `next` found abandoned and stopped.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Left {
    pub uuid: String,
    pub title: String,
    pub project: String,
    /// Who held it, when a claim said.
    pub was: String,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Next {
    /// The task to work, or nothing when no task is ready.
    pub task: Option<Brief>,
    pub started: bool,
    /// Abandoned starts that were stopped. Check each one's repo for
    /// leftover work before resuming it.
    pub cleared: Vec<Left>,
    /// Started more than twelve hours ago with a session that may still own
    /// them. Tell the user; do not stop them.
    pub long_running: Vec<Named>,
    /// How many other tasks are ready.
    pub ready: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Closed {
    pub task: Named,
    /// Tasks that waited only on this one and can start now.
    pub freed: Vec<Named>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Asked {
    pub uuid: String,
    pub title: String,
    pub options: Vec<String>,
}

impl Desk {
    pub fn open(cfg: Config) -> Desk {
        let notes = crate::load::notes_dir(&cfg);
        Desk {
            cfg,
            notes,
            zone: Zone::Local,
        }
    }

    pub fn snapshot(&self, now: i64) -> anyhow::Result<Snapshot> {
        let snap = crate::load::snapshot(&self.cfg, self.notes.as_deref(), now, self.zone);
        match &snap.error {
            Some(e) => bail!("{e}"),
            None => Ok(snap),
        }
    }

    /// The user's name as visits write it: the first hands tag, capitalized.
    pub fn you(&self) -> String {
        let tag = self
            .cfg
            .tags
            .hands
            .first()
            .map(String::as_str)
            .unwrap_or("me");
        let mut chars = tag.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => "Me".into(),
        }
    }

    pub fn brief(&self, snap: &Snapshot, c: &Card) -> Brief {
        let named = |uuids: &[String]| {
            uuids
                .iter()
                .filter_map(|u| snap.card(u))
                .map(Named::from)
                .collect()
        };
        Brief {
            uuid: c.uuid.clone(),
            title: c.title.clone(),
            project: c.project.clone(),
            priority: c.priority.to_string().trim().to_string(),
            tags: c.tags.clone(),
            yours: c.yours,
            decision: c.decision,
            lane: c.lane,
            reference: c.reference.clone(),
            note: self
                .notes
                .as_ref()
                .map(|d| act::note_path(d, &c.uuid).to_string_lossy().into_owned()),
            has_note: c.has_note,
            visits: c.visits.clone(),
            frees: named(&c.frees),
            waits_on: named(&c.waits_on),
            hold: c.hold.clone(),
        }
    }
}

/// Finds the one card `id` names: a full uuid, or the start of one.
pub fn resolve<'a>(snap: &'a Snapshot, id: &str) -> anyhow::Result<&'a Card> {
    let id = id.trim().to_ascii_lowercase();
    if id.len() < 6 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        bail!("{id:?} is not a task uuid; give the uuid or its first 8 characters");
    }
    let mut found = snap.cards.iter().filter(|c| c.uuid.starts_with(&id));
    match (found.next(), found.next()) {
        (Some(c), None) => Ok(c),
        (None, _) => bail!("no open or recent task starts with {id}"),
        (Some(_), Some(_)) => bail!("more than one task starts with {id}; give more of the uuid"),
    }
}

fn in_project(card: &Card, filter: Option<&str>) -> bool {
    match filter {
        None => true,
        // A family name matches every project under it.
        Some(f) => {
            card.project == f
                || card
                    .project
                    .strip_prefix(f)
                    .is_some_and(|r| r.starts_with('.'))
        }
    }
}

/// Clears abandoned starts, then picks the most urgent ready task and (unless
/// `peek`) starts it under a claim.
pub fn next(desk: &Desk, project: Option<&str>, peek: bool, now: i64) -> anyhow::Result<Next> {
    let snap = desk.snapshot(now)?;
    let mut cleared = Vec::new();
    let mut long_running = Vec::new();
    for c in snap.lane(Lane::Doing) {
        let Some(hold) = &c.hold else { continue };
        // The user's own starts are never stopped for them.
        if hold.state == HoldState::Stale && !c.yours {
            let why = if hold.who.is_empty() {
                "started before this machine last booted".to_string()
            } else {
                format!("the session that started it is gone ({})", hold.who)
            };
            if !peek {
                act::stop(&desk.cfg, &c.uuid)?;
                act::annotate(
                    &desk.cfg,
                    &c.uuid,
                    &format!("stopped: {why}; check its repo for leftover work before resuming"),
                )?;
                claim::remove(&c.uuid);
            }
            cleared.push(Left {
                uuid: c.uuid.clone(),
                title: c.title.clone(),
                project: c.project.clone(),
                was: hold.who.clone(),
                why,
            });
        } else {
            let last = c
                .visits
                .iter()
                .map(|v| v.at)
                .chain(c.started)
                .max()
                .unwrap_or(now);
            if now - last > 12 * HOUR {
                long_running.push(Named::from(c));
            }
        }
    }
    let ready: Vec<&Card> = snap
        .lane(Lane::Ready)
        .into_iter()
        .filter(|c| in_project(c, project))
        .collect();
    let Some(pick) = ready.first() else {
        return Ok(Next {
            task: None,
            started: false,
            cleared,
            long_running,
            ready: 0,
        });
    };
    let mut brief = desk.brief(&snap, pick);
    if !peek {
        act::start(&desk.cfg, &pick.uuid)?;
        let claim = claim::take(&pick.uuid, now);
        brief.lane = Lane::Doing;
        brief.hold = Some(claim::judge(now, Some((&claim, true)), None));
    }
    Ok(Next {
        task: Some(brief),
        started: !peek,
        cleared,
        long_running,
        ready: ready.len() - 1,
    })
}

/// Starts a named task under a claim. A task a live session holds is left
/// alone unless `take` says to take it over.
pub fn start(desk: &Desk, id: &str, take: bool, now: i64) -> anyhow::Result<Brief> {
    let snap = desk.snapshot(now)?;
    let card = resolve(&snap, id)?;
    if card.ended.is_some() {
        bail!("{:?} is already finished", card.title);
    }
    if let Some(hold) = card.hold.as_ref().filter(|h| h.state == HoldState::Held)
        && !take
    {
        bail!(
            "{:?} is held by {}; pass --take to take it over",
            card.title,
            hold.who
        );
    }
    if card.lane != Lane::Doing {
        act::start(&desk.cfg, &card.uuid)?;
    } else if take {
        act::annotate(&desk.cfg, &card.uuid, "taken over by another session")?;
    }
    let claim = claim::take(&card.uuid, now);
    let mut brief = desk.brief(&snap, card);
    brief.lane = Lane::Doing;
    brief.hold = Some(claim::judge(now, Some((&claim, true)), None));
    Ok(brief)
}

/// Stops a started task and drops its claim, with a line saying why.
pub fn stop(desk: &Desk, id: &str, why: Option<&str>, now: i64) -> anyhow::Result<Named> {
    let snap = desk.snapshot(now)?;
    let card = resolve(&snap, id)?;
    if card.lane != Lane::Doing {
        bail!("{:?} is not started", card.title);
    }
    if let Some(why) = why.filter(|w| !w.trim().is_empty()) {
        act::annotate(&desk.cfg, &card.uuid, why)?;
    }
    act::stop(&desk.cfg, &card.uuid)?;
    claim::remove(&card.uuid);
    Ok(Named::from(card))
}

/// Logs a visit and keeps the claim fresh.
pub fn visit(desk: &Desk, id: &str, text: &str, now: i64) -> anyhow::Result<Named> {
    let snap = desk.snapshot(now)?;
    let card = resolve(&snap, id)?;
    act::annotate(&desk.cfg, &card.uuid, text)?;
    claim::touch(&card.uuid, now);
    Ok(Named::from(card))
}

/// Finishes a task, with an optional closing line, and says what that freed.
pub fn done(desk: &Desk, id: &str, text: Option<&str>, now: i64) -> anyhow::Result<Closed> {
    let snap = desk.snapshot(now)?;
    let card = resolve(&snap, id)?;
    if card.ended.is_some() {
        bail!("{:?} is already finished", card.title);
    }
    if let Some(text) = text.filter(|t| !t.trim().is_empty()) {
        act::annotate(&desk.cfg, &card.uuid, text)?;
    }
    act::done(&desk.cfg, &card.uuid)?;
    claim::remove(&card.uuid);
    let freed = snap
        .cards
        .iter()
        .filter(|c| c.waits_on.len() == 1 && c.waits_on[0] == card.uuid)
        .map(Named::from)
        .collect();
    Ok(Closed {
        task: Named::from(card),
        freed,
    })
}

fn options_section(options: &[String]) -> String {
    let mut out = String::from("## Options\n\n");
    for o in options {
        out.push_str(&format!("- {}\n", o.trim()));
    }
    out
}

/// Files a new decision for the user: a task of theirs, tagged as a
/// decision, with its options in the note.
pub fn ask_new(
    desk: &Desk,
    mut new: NewTask,
    options: &[String],
    context: Option<&str>,
) -> anyhow::Result<Asked> {
    let dir = desk
        .notes
        .as_ref()
        .context("there is no notes folder to write the options in")?;
    for tag in [desk.cfg.tags.hands.first(), Some(&desk.cfg.tags.decision)]
        .into_iter()
        .flatten()
    {
        if !new.tags.contains(tag) {
            new.tags.push(tag.clone());
        }
    }
    let uuid = act::add(&desk.cfg, &new)?;
    let mut note = format!("# {}\n", new.title.trim());
    if let Some(c) = context.filter(|c| !c.trim().is_empty()) {
        note.push_str(&format!("\n{}\n", c.trim()));
    }
    if !options.is_empty() {
        note.push_str(&format!("\n{}", options_section(options)));
    }
    act::write_note(dir, &uuid, &note)?;
    Ok(Asked {
        uuid,
        title: new.title.trim().to_string(),
        options: options.to_vec(),
    })
}

/// Turns an existing task into a decision for the user.
pub fn ask_about(
    desk: &Desk,
    id: &str,
    options: &[String],
    context: Option<&str>,
    now: i64,
) -> anyhow::Result<Asked> {
    let dir = desk
        .notes
        .as_ref()
        .context("there is no notes folder to write the options in")?;
    let snap = desk.snapshot(now)?;
    let card = resolve(&snap, id)?;
    let mut changes = vec![Change::AddTag(desk.cfg.tags.decision.clone())];
    if let Some(hands) = desk.cfg.tags.hands.first() {
        changes.push(Change::AddTag(hands.clone()));
    }
    act::modify(&desk.cfg, &card.uuid, &changes)?;
    let mut section = String::new();
    if let Some(c) = context.filter(|c| !c.trim().is_empty()) {
        section.push_str(&format!("{}\n\n", c.trim()));
    }
    section.push_str(&options_section(options));
    act::append_note(dir, &card.uuid, &section)?;
    Ok(Asked {
        uuid: card.uuid.clone(),
        title: card.title.clone(),
        options: options.to_vec(),
    })
}

/// Records the user's ruling on a decision: a visit in their name, a
/// "Decided" section in the note, and the task handed back to the agents.
pub fn rule(desk: &Desk, id: &str, choice: &str, now: i64) -> anyhow::Result<Named> {
    let choice = choice.trim();
    if choice.is_empty() {
        bail!("a ruling needs some words");
    }
    let snap = desk.snapshot(now)?;
    let card = resolve(&snap, id)?;
    act::annotate(&desk.cfg, &card.uuid, &format!("{}: {choice}", desk.you()))?;
    if let Some(dir) = &desk.notes {
        let (y, m, d) = civil_from_days(desk.zone.day(now));
        act::append_note(
            dir,
            &card.uuid,
            &format!("## Decided {y:04}-{m:02}-{d:02}\n\n- {choice}\n"),
        )?;
    }
    let mut changes = vec![Change::DropTag(desk.cfg.tags.decision.clone())];
    changes.extend(desk.cfg.tags.hands.iter().cloned().map(Change::DropTag));
    act::modify(&desk.cfg, &card.uuid, &changes)?;
    Ok(Named::from(card))
}
