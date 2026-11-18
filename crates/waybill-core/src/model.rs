//! The snapshot every view draws from: tasks sorted into lanes, with what
//! each one waits on and what it would free.

use crate::clock::{DAY, Zone};
use crate::lines::Line;
use crate::pulse::Pulse;
use crate::task::{Status, Task};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Which tags sort a task onto the user's own desk.
#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    pub hands: Vec<String>,
    pub decision: String,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            hands: vec!["me".into()],
            decision: "decision".into(),
        }
    }
}

impl Rules {
    pub fn yours(&self, t: &Task) -> bool {
        t.tags
            .iter()
            .any(|g| self.hands.contains(g) || *g == self.decision)
    }

    /// The name visits use for the user: the first hands tag.
    pub fn you(&self) -> &str {
        self.hands.first().map(String::as_str).unwrap_or("me")
    }
}

/// Where a card sits on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lane {
    /// An agent can pick it up now.
    Ready,
    /// Started.
    Doing,
    /// Needs the user's hands or a decision.
    Yours,
    /// Waits on another task.
    Held,
    /// Hidden until a later date.
    Parked,
    /// Finished today.
    Done,
    /// Finished earlier this week; kept for the log and the card pane.
    Past,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Visit {
    pub at: i64,
    pub text: String,
    /// Commit ids the text names.
    #[serde(default)]
    pub shas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub uuid: String,
    pub title: String,
    pub project: String,
    /// 'H', 'M', 'L' or ' '.
    pub priority: char,
    /// Tags other than the hands and decision tags.
    pub tags: Vec<String>,
    pub yours: bool,
    pub decision: bool,
    /// Waits on at least one open task.
    pub blocked: bool,
    pub lane: Lane,
    pub entry: i64,
    pub started: Option<i64>,
    pub ended: Option<i64>,
    pub urgency: f64,
    pub reference: Option<String>,
    pub has_note: bool,
    pub visits: Vec<Visit>,
    /// Open tasks that cannot start until this one closes, however far down
    /// the line.
    pub frees: Vec<String>,
    /// Open tasks this one waits on directly.
    pub waits_on: Vec<String>,
    /// Visits per day over the last two weeks, oldest first.
    pub activity: Vec<u16>,
}

impl Card {
    /// The first eight characters, as Taskwarrior shortens a uuid.
    pub fn short(&self) -> &str {
        self.uuid.get(..8).unwrap_or(&self.uuid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Visit,
    /// A visit that records what the user decided.
    Ruling,
    Done,
    Added,
}

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub at: i64,
    pub uuid: String,
    pub title: String,
    pub project: String,
    pub kind: Kind,
    pub text: String,
    #[serde(default)]
    pub shas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub now: i64,
    /// Seconds east of UTC, for printing times.
    pub offset: i64,
    /// What the user is called in visits ("nate").
    pub you: String,
    pub cards: Vec<Card>,
    pub lines: Vec<Line>,
    pub log: Vec<Event>,
    pub pulse: Pulse,
    /// Why the task list could not be read, when it could not.
    #[serde(default)]
    pub error: Option<String>,
}

impl Snapshot {
    pub fn failed(now: i64, zone: Zone, error: String) -> Snapshot {
        Snapshot {
            now,
            offset: zone.offset(now),
            error: Some(error),
            ..Default::default()
        }
    }

    /// A lane's cards in the order the board shows them.
    pub fn lane(&self, lane: Lane) -> Vec<&Card> {
        let mut cards: Vec<&Card> = self.cards.iter().filter(|c| c.lane == lane).collect();
        let rank = |p: char| match p {
            'H' => 0,
            'M' => 1,
            'L' => 2,
            _ => 3,
        };
        match lane {
            Lane::Doing => cards.sort_by_key(|c| c.started),
            Lane::Done | Lane::Past => cards.sort_by_key(|c| std::cmp::Reverse(c.ended)),
            // What frees the most comes first, then what was marked urgent.
            Lane::Yours => cards.sort_by(|a, b| {
                (b.frees.len(), a.blocked, rank(a.priority))
                    .cmp(&(a.frees.len(), b.blocked, rank(b.priority)))
                    .then(b.urgency.total_cmp(&a.urgency))
            }),
            _ => cards.sort_by(|a, b| b.urgency.total_cmp(&a.urgency)),
        }
        cards
    }

    pub fn card(&self, uuid: &str) -> Option<&Card> {
        self.cards.iter().find(|c| c.uuid == uuid)
    }
}

/// Commit ids in a visit: hex words of 7 to 12 characters. A word of digits
/// only counts unless it reads as a date.
pub fn shas(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        let n = word.len();
        if !(7..=12).contains(&n) || !word.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        if word.bytes().any(|b| b.is_ascii_uppercase()) {
            continue;
        }
        let digits = word.bytes().filter(u8::is_ascii_digit).count();
        let dated = n == 8 && (word.starts_with("19") || word.starts_with("20"));
        if digits == 0 || (digits == n && (dated || n > 8)) {
            continue;
        }
        if !out.iter().any(|s| s == word) {
            out.push(word.to_string());
        }
    }
    out
}

/// True when a visit opens with the user's name: "Nate: ...", "Nate chose ...".
pub fn is_ruling(text: &str, you: &str) -> bool {
    let (t, y) = (text.trim_start(), you.trim());
    !y.is_empty()
        && t.len() > y.len()
        && t.is_char_boundary(y.len())
        && t[..y.len()].eq_ignore_ascii_case(y)
        && !t[y.len()..].starts_with(|c: char| c.is_alphanumeric())
}

/// How far back the log and the finished cards reach.
pub const RECENT: i64 = 7 * DAY;
const LOG_CAP: usize = 2_000;

/// Builds the snapshot. `notes` holds the uuids that have a note file.
pub fn build(
    tasks: &[Task],
    notes: &HashSet<String>,
    rules: &Rules,
    now: i64,
    zone: Zone,
) -> Snapshot {
    let by: HashMap<&str, &Task> = tasks.iter().map(|t| (t.uuid.as_str(), t)).collect();
    let open = |u: &str| by.get(u).is_some_and(|t| t.open());

    // Who waits on whom, among open tasks.
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in tasks.iter().filter(|t| t.open()) {
        for d in t.depends.iter().filter(|d| open(d)) {
            dependents.entry(d.as_str()).or_default().push(&t.uuid);
        }
    }
    let frees = |uuid: &str| -> Vec<String> {
        let mut seen: Vec<&str> = Vec::new();
        let mut stack = vec![uuid];
        while let Some(u) = stack.pop() {
            for v in dependents.get(u).into_iter().flatten() {
                if !seen.contains(v) {
                    seen.push(v);
                    stack.push(v);
                }
            }
        }
        seen.into_iter().map(String::from).collect()
    };

    let today = zone.day(now);
    let today_start = zone.day_start(today);
    let mut cards = Vec::new();
    let mut log = Vec::new();
    for t in tasks {
        let recent_end = t.end.is_some_and(|e| e >= now - RECENT);
        let lane = match t.status {
            Status::Pending | Status::Waiting => {
                let waits = t.depends.iter().any(|d| open(d));
                if t.start.is_some() {
                    Lane::Doing
                } else if t.status == Status::Waiting || t.wait.is_some_and(|w| w > now) {
                    Lane::Parked
                } else if rules.yours(t) {
                    Lane::Yours
                } else if waits {
                    Lane::Held
                } else {
                    Lane::Ready
                }
            }
            Status::Completed if t.end.is_some_and(|e| e >= today_start) => Lane::Done,
            Status::Completed if recent_end => Lane::Past,
            _ => continue,
        };
        let project = t.project.clone().unwrap_or_default();
        let visits: Vec<Visit> = t
            .annotations
            .iter()
            .map(|a| Visit {
                at: a.entry,
                text: a.description.clone(),
                shas: shas(&a.description),
            })
            .collect();
        for v in visits.iter().filter(|v| v.at >= now - RECENT) {
            log.push(Event {
                at: v.at,
                uuid: t.uuid.clone(),
                title: t.description.clone(),
                project: project.clone(),
                kind: if is_ruling(&v.text, rules.you()) {
                    Kind::Ruling
                } else {
                    Kind::Visit
                },
                text: v.text.clone(),
                shas: v.shas.clone(),
            });
        }
        if let (Status::Completed, Some(end), true) = (t.status, t.end, recent_end) {
            log.push(Event {
                at: end,
                uuid: t.uuid.clone(),
                title: t.description.clone(),
                project: project.clone(),
                kind: Kind::Done,
                text: String::new(),
                shas: vec![],
            });
        }
        if t.entry >= now - RECENT {
            log.push(Event {
                at: t.entry,
                uuid: t.uuid.clone(),
                title: t.description.clone(),
                project: project.clone(),
                kind: Kind::Added,
                text: String::new(),
                shas: vec![],
            });
        }
        let mut activity = vec![0u16; 14];
        for v in &visits {
            let back = today - zone.day(v.at);
            if (0..14).contains(&back) {
                activity[13 - back as usize] += 1;
            }
        }
        cards.push(Card {
            uuid: t.uuid.clone(),
            title: t.description.clone(),
            project,
            priority: t
                .priority
                .as_deref()
                .and_then(|p| p.chars().next())
                .unwrap_or(' '),
            tags: t
                .tags
                .iter()
                .filter(|g| !rules.hands.contains(g) && **g != rules.decision)
                .cloned()
                .collect(),
            yours: rules.yours(t),
            decision: t.has_tag(&rules.decision),
            blocked: t.open() && t.depends.iter().any(|d| open(d)),
            lane,
            entry: t.entry,
            started: t.start,
            ended: t.end,
            urgency: t.urgency,
            reference: t.reference.clone(),
            has_note: notes.contains(&t.uuid),
            visits,
            frees: if t.open() { frees(&t.uuid) } else { vec![] },
            waits_on: t.depends.iter().filter(|d| open(d)).cloned().collect(),
            activity,
        });
    }
    // Newest first; within one second, a closing "done" reads above its visit.
    log.sort_by(|a, b| {
        b.at.cmp(&a.at)
            .then((b.kind == Kind::Done).cmp(&(a.kind == Kind::Done)))
    });
    log.truncate(LOG_CAP);

    Snapshot {
        now,
        offset: zone.offset(now),
        you: rules.you().to_string(),
        lines: crate::lines::build(tasks, rules, now),
        pulse: crate::pulse::build(tasks, &cards, rules, now, zone),
        cards,
        log,
        error: None,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::clock::HOUR;
    use crate::task::Annotation;

    pub fn task(uuid: &str, title: &str) -> Task {
        Task {
            uuid: uuid.into(),
            description: title.into(),
            status: Status::Pending,
            project: Some("storefront".into()),
            priority: Some("M".into()),
            tags: vec![],
            depends: vec![],
            annotations: vec![],
            entry: NOW - 3 * DAY,
            start: None,
            end: None,
            modified: None,
            wait: None,
            due: None,
            urgency: 5.0,
            reference: None,
        }
    }

    /// 2026-11-18 15:00 UTC, a Wednesday.
    pub const NOW: i64 = 1_795_014_000;

    fn world() -> Vec<Task> {
        let mut playtest = task("play", "Playtest the cart");
        playtest.tags = vec!["me".into()];
        playtest.priority = Some("H".into());
        let mut build = task("build", "Build the receipt");
        build.depends = vec!["play".into()];
        let mut ship = task("ship", "Ship the receipt");
        ship.depends = vec!["build".into(), "old".into()];
        let mut doing = task("doing", "Wire the cart");
        doing.start = Some(NOW - HOUR);
        doing.annotations = vec![
            Annotation {
                entry: NOW - 1800,
                description: "split the loader; parser next (a1b2c3d)".into(),
            },
            Annotation {
                entry: NOW - 600,
                description: "Me: keep the old parser".into(),
            },
        ];
        let mut parked = task("parked", "Someday");
        parked.wait = Some(NOW + 400 * DAY);
        let mut old = task("old", "Draw the cart");
        old.status = Status::Completed;
        old.end = Some(NOW - 2 * HOUR);
        let mut gone = task("gone", "Dropped");
        gone.status = Status::Deleted;
        gone.end = Some(NOW - HOUR);
        let mut decide = task("decide", "Pick the font");
        decide.tags = vec!["decision".into()];
        vec![
            playtest,
            build,
            ship,
            doing,
            parked,
            old,
            gone,
            decide,
            task("free", "Fix a typo"),
        ]
    }

    fn snap() -> Snapshot {
        let notes = HashSet::from(["play".to_string()]);
        build(&world(), &notes, &Rules::default(), NOW, Zone::Fixed(0))
    }

    #[test]
    fn lanes() {
        let s = snap();
        let lane = |u: &str| s.card(u).map(|c| c.lane);
        assert_eq!(lane("play"), Some(Lane::Yours));
        assert_eq!(lane("decide"), Some(Lane::Yours));
        assert_eq!(lane("build"), Some(Lane::Held));
        assert_eq!(lane("ship"), Some(Lane::Held));
        assert_eq!(lane("doing"), Some(Lane::Doing));
        assert_eq!(lane("parked"), Some(Lane::Parked));
        assert_eq!(lane("old"), Some(Lane::Done));
        assert_eq!(lane("free"), Some(Lane::Ready));
        assert_eq!(lane("gone"), None, "a deleted task has no card");
    }

    #[test]
    fn what_a_card_frees_runs_down_the_line() {
        let s = snap();
        let play = s.card("play").unwrap();
        assert_eq!(play.frees.len(), 2, "the build and the ship behind it");
        assert!(play.has_note && play.yours && !play.blocked);
        let ship = s.card("ship").unwrap();
        assert_eq!(
            ship.waits_on,
            ["build"],
            "a finished dependency no longer holds it"
        );
        // Yours sorts by what it frees.
        assert_eq!(s.lane(Lane::Yours)[0].uuid, "play");
    }

    #[test]
    fn the_log_is_newest_first_and_knows_rulings() {
        let s = snap();
        let doing: Vec<&Event> = s.log.iter().filter(|e| e.uuid == "doing").collect();
        assert_eq!(doing[0].kind, Kind::Ruling);
        assert_eq!(doing[1].kind, Kind::Visit);
        assert_eq!(doing[1].shas, ["a1b2c3d"]);
        assert!(s.log.windows(2).all(|w| w[0].at >= w[1].at));
        assert!(
            s.log
                .iter()
                .any(|e| e.uuid == "old" && e.kind == Kind::Done)
        );
        assert_eq!(s.card("doing").unwrap().activity[13], 2);
    }

    #[test]
    fn commit_ids() {
        assert_eq!(
            shas("committed as 23922946, 13b995e5 after 2058 tests"),
            ["23922946", "13b995e5"]
        );
        assert_eq!(
            shas("built 5201bd7..b235eeb on 20261109"),
            ["5201bd7", "b235eeb"]
        );
        assert!(shas("decade of defaced facades, 1234567890").is_empty());
    }

    #[test]
    fn rulings() {
        assert!(is_ruling("Nate: full game $5", "nate"));
        assert!(is_ruling("Nate chose map-binding", "nate"));
        assert!(!is_ruling("Nathan wrote in", "nate"));
        assert!(!is_ruling("re-scoped with Nate", "nate"));
        assert!(!is_ruling("Nate", "nate"));
    }
}
