//! What a click or a key does to the task list. Every action runs on a
//! worker thread through `waybill-core`, which means through the `task`
//! binary, and comes back as one line for the footer.

use std::path::Path;
use waybill_core::act::{self, Change, NewTask};
use waybill_core::agent::{self, Desk};
use waybill_core::claim;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Start(String),
    Stop(String),
    /// Finish, with an optional closing line.
    Done(String, String),
    Visit(String, String),
    Priority(String, char),
    /// Put the task on the user's desk, or hand it to the agents.
    Hands(String, bool),
    /// Hide until a date; an empty date shows it again.
    Park(String, String),
    /// The user's ruling on a decision.
    Rule(String, String),
    SaveNote(String, String),
    Add {
        new: NewTask,
        note: String,
        /// Tag the new task for the user's hands.
        mine: bool,
    },
}

/// The user's words carry the user's name, as agents write them.
fn signed(desk: &Desk, text: &str) -> String {
    let you = desk.you();
    if waybill_core::model::is_ruling(text, &you) {
        text.trim().to_string()
    } else {
        format!("{you}: {}", text.trim())
    }
}

fn notes(desk: &Desk) -> anyhow::Result<&Path> {
    desk.notes
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("there is no notes folder"))
}

/// Runs one action and says what happened.
pub fn perform(desk: &Desk, action: Action, now: i64) -> anyhow::Result<String> {
    let cfg = &desk.cfg;
    Ok(match action {
        Action::Start(uuid) => {
            act::start(cfg, &uuid)?;
            claim::take_as(&uuid, &format!("{}, at the board", desk.you()), now);
            "Started".into()
        }
        Action::Stop(uuid) => {
            act::stop(cfg, &uuid)?;
            claim::remove(&uuid);
            "Stopped".into()
        }
        Action::Done(uuid, text) => {
            let closed = agent::done(
                desk,
                &uuid,
                Some(&text)
                    .filter(|t| !t.trim().is_empty())
                    .map(|t| signed(desk, t))
                    .as_deref(),
                now,
            )?;
            match closed.freed.len() {
                0 => "Finished".into(),
                1 => format!("Finished. That frees: {}", closed.freed[0].title),
                n => format!("Finished. That frees {n} tasks"),
            }
        }
        Action::Visit(uuid, text) => {
            act::annotate(cfg, &uuid, &signed(desk, &text))?;
            claim::touch(&uuid, now);
            "Visit logged".into()
        }
        Action::Priority(uuid, p) => {
            act::modify(cfg, &uuid, &[Change::Priority(p)])?;
            format!(
                "Priority {}",
                if p == ' ' {
                    "cleared".to_string()
                } else {
                    p.to_string()
                }
            )
        }
        Action::Hands(uuid, mine) => {
            let changes: Vec<Change> = if mine {
                cfg.tags
                    .hands
                    .first()
                    .cloned()
                    .map(Change::AddTag)
                    .into_iter()
                    .collect()
            } else {
                cfg.tags
                    .hands
                    .iter()
                    .cloned()
                    .map(Change::DropTag)
                    .collect()
            };
            act::modify(cfg, &uuid, &changes)?;
            if mine {
                "It is on your desk"
            } else {
                "Handed to the agents"
            }
            .into()
        }
        Action::Park(uuid, until) => {
            act::modify(cfg, &uuid, &[Change::Wait(until.clone())])?;
            if until.is_empty() {
                "Back on the board".into()
            } else {
                format!("Parked until {until}")
            }
        }
        Action::Rule(uuid, choice) => {
            agent::rule(desk, &uuid, &choice, now)?;
            format!("Ruled: {}", choice.trim())
        }
        Action::SaveNote(uuid, text) => {
            act::write_note(notes(desk)?, &uuid, &text)?;
            "Note saved".into()
        }
        Action::Add {
            mut new,
            note,
            mine,
        } => {
            if mine && let Some(tag) = cfg.tags.hands.first() {
                new.tags.push(tag.clone());
            }
            let uuid = act::add(cfg, &new)?;
            if !note.trim().is_empty() {
                let text = if note.trim_start().starts_with('#') {
                    note
                } else {
                    format!("# {}\n\n{note}", new.title.trim())
                };
                act::write_note(notes(desk)?, &uuid, &text)?;
            }
            format!("Added: {}", new.title.trim())
        }
    })
}
