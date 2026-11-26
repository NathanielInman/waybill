//! What the user can say to the list, from any screen: the actions a card
//! offers, the plain words of the `/` line, what a drop on a lane means and
//! the one function that carries an action out. The board, the drawer and
//! `waybill serve` (the phone) all call these, so a new word or a new
//! button is added here once.

use crate::act::{self, Change, NewTask};
use crate::agent::{self, Desk};
use crate::claim;
use crate::model::{Card, Lane};
use serde::Serialize;
use std::path::Path;

/// The five views, in the order of their keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Board,
    Desk,
    Lines,
    Log,
    Pulse,
}

impl View {
    pub const ALL: [View; 5] = [View::Board, View::Desk, View::Lines, View::Log, View::Pulse];

    pub fn name(self) -> &'static str {
        match self {
            View::Board => "Board",
            View::Desk => "Desk",
            View::Lines => "Lines",
            View::Log => "Log",
            View::Pulse => "Pulse",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            View::Board => "board",
            View::Desk => "desk",
            View::Lines => "lines",
            View::Log => "log",
            View::Pulse => "pulse",
        }
    }

    /// The one question the view answers.
    pub fn question(self) -> &'static str {
        match self {
            View::Board => "Where is everything right now?",
            View::Desk => "What is waiting on me, and what does it free?",
            View::Lines => "What has to happen in what order?",
            View::Log => "What happened while I was away?",
            View::Pulse => "How is the work flowing?",
        }
    }

    pub fn parse(s: &str) -> Option<View> {
        View::ALL.into_iter().find(|t| t.id() == s)
    }

    pub fn index(self) -> usize {
        View::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

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
    if crate::model::is_ruling(text, &you) {
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

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Do(Action),
    View(View),
    /// Open the new-task form with this title.
    New(String),
}

/// What `text` means for the open card `uuid`, with the words to show for
/// it. `None` when it is not a command (it is then a search).
pub fn parse(text: &str, uuid: Option<&str>) -> Option<(Command, String)> {
    let text = text.trim();
    let lower = text.to_lowercase();
    let (word, rest) = match lower.split_once(char::is_whitespace) {
        Some((w, _)) => (w, text[w.len()..].trim()),
        None => (lower.as_str(), ""),
    };
    if let Some(view) = View::parse(&lower) {
        return Some((Command::View(view), format!("Go to {}", view.name())));
    }
    if matches!(word, "new" | "add") && !rest.is_empty() {
        return Some((Command::New(rest.to_string()), format!("New task: {rest}")));
    }
    let uuid = uuid?.to_string();
    let act = |a: Action, say: &str| Some((Command::Do(a), say.to_string()));
    match lower.as_str() {
        "start" => return act(Action::Start(uuid), "Start it"),
        "stop" => return act(Action::Stop(uuid), "Stop it"),
        "h" | "high" | "high priority" => {
            return act(Action::Priority(uuid, 'H'), "Set priority high");
        }
        "m" | "medium" | "medium priority" | "normal" => {
            return act(Action::Priority(uuid, 'M'), "Set priority medium");
        }
        "l" | "low" | "low priority" | "someday maybe" => {
            return act(Action::Priority(uuid, 'L'), "Set priority low");
        }
        "mine" | "take" | "take it" | "needs me" => {
            return act(Action::Hands(uuid, true), "Put it on your desk");
        }
        "agent" | "agents" | "hand to agent" | "hand to agents" | "hand off" => {
            return act(Action::Hands(uuid, false), "Hand it to the agents");
        }
        "park" | "someday" => {
            return act(
                Action::Park(uuid, "someday".into()),
                "Park it until someday",
            );
        }
        "unpark" | "show" | "wake" => {
            return act(
                Action::Park(uuid, String::new()),
                "Put it back on the board",
            );
        }
        _ => {}
    }
    match word {
        "done" | "finish" => Some((
            Command::Do(Action::Done(uuid, rest.to_string())),
            if rest.is_empty() {
                "Finish it".to_string()
            } else {
                format!("Finish it: {rest}")
            },
        )),
        "visit" | "log" | "say" if !rest.is_empty() => Some((
            Command::Do(Action::Visit(uuid, rest.to_string())),
            format!("Log a visit: {rest}"),
        )),
        "rule" | "decide" if !rest.is_empty() => Some((
            Command::Do(Action::Rule(uuid, rest.to_string())),
            format!("Rule: {rest}"),
        )),
        "wait" | "park" => {
            // "wait until friday" and "wait friday" both end in the date.
            let date = rest
                .rsplit(char::is_whitespace)
                .next()
                .unwrap_or("")
                .to_lowercase();
            (!date.is_empty() && date != "until").then(|| {
                (
                    Command::Do(Action::Park(uuid, date.clone())),
                    format!("Park it until {date}"),
                )
            })
        }
        _ => None,
    }
}

/// How well `text` matches a card: lower is better, `None` is no match.
/// Every word must appear somewhere in the title, project, tags or uuid.
pub fn score(text: &str, title: &str, project: &str, tags: &[String], uuid: &str) -> Option<usize> {
    let hay_title = title.to_lowercase();
    let mut total = 0;
    for word in text.to_lowercase().split_whitespace() {
        total += if let Some(at) = hay_title.find(word) {
            at
        } else if project.to_lowercase().contains(word)
            || tags.iter().any(|t| t.to_lowercase() == word)
            || uuid.starts_with(word)
        {
            200
        } else {
            return None;
        };
    }
    Some(total)
}

/// What dropping a card from `from` onto `to` means.
pub fn dropped(
    uuid: &str,
    from: Lane,
    yours: bool,
    to: Lane,
) -> Result<Option<Action>, &'static str> {
    let uuid = uuid.to_string();
    Ok(match to {
        _ if from == to => None,
        Lane::Doing => Some(Action::Start(uuid)),
        Lane::Done => Some(Action::Done(uuid, String::new())),
        Lane::Yours if !yours => Some(Action::Hands(uuid, true)),
        Lane::Ready if from == Lane::Doing => Some(Action::Stop(uuid)),
        Lane::Ready if from == Lane::Parked => Some(Action::Park(uuid, String::new())),
        Lane::Ready if yours => Some(Action::Hands(uuid, false)),
        Lane::Held => return Err("A task is held by what it waits on, not by hand."),
        _ => None,
    })
}

/// One button a card offers: what it says, its key on the board, and the
/// words it hands to [`parse`]. A button with `ask` first asks for a line,
/// which goes after the words ("visit" + "tried it" is "visit tried it").
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Offer {
    pub label: String,
    pub key: Option<char>,
    pub words: String,
    pub ask: Option<String>,
    /// The row it sits in: "act", "priority" or "decide".
    pub group: &'static str,
    /// "go" for start and finish, "choice" for a decision's options.
    pub style: &'static str,
    /// Already true of the card, like its current priority.
    pub on: bool,
}

impl Offer {
    fn new(group: &'static str, label: &str, key: char, words: &str) -> Offer {
        Offer {
            label: label.into(),
            key: Some(key),
            words: words.into(),
            ask: None,
            group,
            style: "",
            on: false,
        }
    }

    fn asking(mut self, hint: &str) -> Offer {
        self.ask = Some(hint.into());
        self
    }

    fn styled(mut self, style: &'static str) -> Offer {
        self.style = style;
        self
    }

    /// The words to parse once the user has answered `ask`.
    pub fn with(&self, line: &str) -> String {
        format!("{} {}", self.words, line.trim()).trim().to_string()
    }

    /// The action this button runs, given the line it asked for.
    pub fn action(&self, uuid: &str, line: &str) -> Option<Action> {
        match parse(&self.with(line), Some(uuid))?.0 {
            Command::Do(a) => Some(a),
            _ => None,
        }
    }
}

/// Every button an open card offers, in the order the card pane shows
/// them. `options` are a decision's choices, from its note. A finished card
/// offers nothing.
pub fn offers(c: &Card, options: &[String]) -> Vec<Offer> {
    if c.ended.is_some() {
        return Vec::new();
    }
    let mut out = vec![
        if c.lane == Lane::Doing {
            Offer::new("act", "Stop", 's', "stop")
        } else {
            Offer::new("act", "Start", 's', "start").styled("go")
        },
        Offer::new("act", "Done", 'd', "done")
            .asking("A closing line, or just Enter")
            .styled("go"),
        Offer::new("act", "Visit", 'v', "visit").asking("What happened, and what is next"),
        if c.lane == Lane::Parked {
            Offer::new("act", "Unpark", 'w', "unpark")
        } else {
            Offer::new("act", "Park", 'w', "park").asking("Until when: friday, 2026-12-01, someday")
        },
    ];
    for (p, key, words) in [('H', 'h', "high"), ('M', 'm', "medium"), ('L', 'l', "low")] {
        let mut o = Offer::new("priority", &p.to_string(), key, words);
        o.on = c.priority == p;
        out.push(o);
    }
    out.push(if c.yours {
        Offer::new("priority", "Hand to agents", 't', "hand to agent")
    } else {
        Offer::new("priority", "Take it", 't', "mine")
    });
    if c.decision {
        for (i, option) in options.iter().enumerate() {
            let mut o =
                Offer::new("decide", option, 'a', &format!("rule {option}")).styled("choice");
            o.key = ['a', 'b', 'c'].get(i).copied();
            out.push(o);
        }
        let label = if options.is_empty() {
            "Write the ruling"
        } else {
            "Something else"
        };
        out.push(Offer::new("decide", label, 'o', "rule").asking("Your ruling, in your words"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(text: &str) -> Option<Action> {
        match parse(text, Some("u"))?.0 {
            Command::Do(a) => Some(a),
            _ => None,
        }
    }

    #[test]
    fn plain_words_act_on_the_open_card() {
        assert_eq!(
            act("high priority"),
            Some(Action::Priority("u".into(), 'H'))
        );
        assert_eq!(act("Low"), Some(Action::Priority("u".into(), 'L')));
        assert_eq!(act("hand to agent"), Some(Action::Hands("u".into(), false)));
        assert_eq!(act("mine"), Some(Action::Hands("u".into(), true)));
        assert_eq!(
            act("wait until Friday"),
            Some(Action::Park("u".into(), "friday".into()))
        );
        assert_eq!(
            act("park"),
            Some(Action::Park("u".into(), "someday".into()))
        );
        assert_eq!(
            act("done Shipped in a1b2c3d"),
            Some(Action::Done("u".into(), "Shipped in a1b2c3d".into()))
        );
        assert_eq!(act("done"), Some(Action::Done("u".into(), String::new())));
        assert_eq!(
            act("visit Tried it, feels slow"),
            Some(Action::Visit("u".into(), "Tried it, feels slow".into()))
        );
        assert_eq!(
            act("rule Keep 4 s"),
            Some(Action::Rule("u".into(), "Keep 4 s".into()))
        );
        assert_eq!(act("netplay"), None, "anything else is a search");
        assert_eq!(act("wait until"), None);
    }

    #[test]
    fn some_words_need_no_card() {
        assert_eq!(
            parse("pulse", None).map(|c| c.0),
            Some(Command::View(View::Pulse))
        );
        assert_eq!(
            parse("new Fix the typo", None).map(|c| c.0),
            Some(Command::New("Fix the typo".into()))
        );
        assert_eq!(parse("high", None), None, "nothing is open to act on");
    }

    #[test]
    fn search_wants_every_word() {
        let tags = vec!["bug".to_string()];
        let s = |q: &str| {
            score(
                q,
                "Netplay: lobby and discovery",
                "detour-33",
                &tags,
                "c5f00fc0-aaaa",
            )
        };
        assert_eq!(s("netplay"), Some(0));
        assert!(s("lobby") > s("netplay"));
        assert_eq!(s("lobby detour"), Some(209));
        assert_eq!(s("bug"), Some(200));
        assert_eq!(s("c5f0"), Some(200));
        assert_eq!(s("lobby kepler"), None);
    }

    #[test]
    fn a_drop_does_what_the_lane_means() {
        let d = |from, yours, to| dropped("u", from, yours, to);
        assert_eq!(
            d(Lane::Ready, false, Lane::Doing),
            Ok(Some(Action::Start("u".into())))
        );
        assert_eq!(
            d(Lane::Doing, false, Lane::Ready),
            Ok(Some(Action::Stop("u".into())))
        );
        assert_eq!(
            d(Lane::Yours, true, Lane::Ready),
            Ok(Some(Action::Hands("u".into(), false)))
        );
        assert_eq!(
            d(Lane::Ready, false, Lane::Yours),
            Ok(Some(Action::Hands("u".into(), true)))
        );
        assert_eq!(
            d(Lane::Parked, false, Lane::Ready),
            Ok(Some(Action::Park("u".into(), String::new())))
        );
        assert_eq!(
            d(Lane::Yours, true, Lane::Done),
            Ok(Some(Action::Done("u".into(), String::new())))
        );
        assert_eq!(d(Lane::Ready, false, Lane::Ready), Ok(None));
        assert_eq!(d(Lane::Doing, true, Lane::Yours), Ok(None), "already yours");
        assert!(d(Lane::Ready, false, Lane::Held).is_err());
    }

    fn card(lane: Lane, yours: bool, decision: bool) -> Card {
        let mut c = crate::demo::snapshot(crate::model::tests::NOW, crate::clock::Zone::Fixed(0))
            .cards
            .into_iter()
            .find(|c| c.ended.is_none())
            .unwrap();
        c.uuid = "u".into();
        (c.lane, c.yours, c.decision, c.priority) = (lane, yours, decision, 'M');
        c
    }

    #[test]
    fn every_offer_is_words_the_line_understands() {
        let options = vec!["Keep 4 s".to_string(), "Lower it to 2 s".to_string()];
        for c in [
            card(Lane::Ready, false, false),
            card(Lane::Doing, false, false),
            card(Lane::Parked, false, false),
            card(Lane::Yours, true, true),
        ] {
            for o in offers(&c, &options) {
                let line = if o.ask.is_some() { "friday" } else { "" };
                assert!(o.action("u", line).is_some(), "{o:?} parses to nothing");
            }
        }
    }

    #[test]
    fn a_card_offers_what_fits_it() {
        let labels = |c: &Card| -> Vec<String> {
            offers(c, &["Keep 4 s".to_string()])
                .into_iter()
                .map(|o| o.label)
                .collect()
        };
        let ready = labels(&card(Lane::Ready, false, false));
        assert_eq!(
            ready,
            ["Start", "Done", "Visit", "Park", "H", "M", "L", "Take it"]
        );
        let doing = labels(&card(Lane::Doing, false, false));
        assert_eq!(doing[0], "Stop");
        let decision = labels(&card(Lane::Yours, true, true));
        assert!(decision.ends_with(&["Keep 4 s".to_string(), "Something else".to_string()]));
        assert!(decision.contains(&"Hand to agents".to_string()));
        let c = card(Lane::Yours, true, true);
        let rule = offers(&c, &["Keep 4 s".to_string()])
            .into_iter()
            .find(|o| o.label == "Keep 4 s")
            .unwrap();
        assert_eq!(rule.key, Some('a'));
        assert_eq!(
            rule.action("u", ""),
            Some(Action::Rule("u".into(), "Keep 4 s".into()))
        );
        let m = offers(&c, &[])
            .into_iter()
            .find(|o| o.label == "M")
            .unwrap();
        assert!(m.on, "the card's own priority is lit");
        let mut done = c.clone();
        done.ended = Some(1);
        assert!(offers(&done, &[]).is_empty());
    }
}
