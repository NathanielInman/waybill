//! Where each view puts things: the lanes in board order, the Desk's
//! sections, what the drawer has room for and the headline over both. They
//! are worked out here, once, so the board, the drawer and the phone show
//! the same cards in the same order.

use crate::model::{Card, Lane, Snapshot};
use serde::{Serialize, Serializer};

/// The board's lanes, left to right: the lane, its title and what it says
/// when it is empty. Parked cards sit under Held.
pub const LANES: [(Lane, &str, &str); 6] = [
    (
        Lane::Ready,
        "Ready for agents",
        "Nothing is waiting for an agent.",
    ),
    (Lane::Doing, "Doing", "Nothing is started."),
    (Lane::Yours, "Needs you", "Nothing needs your hands."),
    (Lane::Held, "Held", "Nothing waits on another task."),
    (Lane::Parked, "Parked until later", "Nothing is parked."),
    (Lane::Done, "Done today", "Nothing has closed yet today."),
];

/// How many rows the drawer shows below its decisions.
pub const DRAWER_ROWS: usize = 6;

fn uuids<S: Serializer>(cards: &[&Card], s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(cards.iter().map(|c| c.uuid.as_str()))
}

/// The sentence over the Desk and the drawer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Headline {
    /// "12 tasks wait on you."
    pub count: String,
    /// "4 of them hold up 8 others.", when any do.
    pub holding: Option<String>,
    /// "Something broke.", while an alarm is open.
    pub broke: Option<String>,
}

pub fn headline(s: &Snapshot) -> Headline {
    let p = &s.pulse;
    let count = match s.lane(Lane::Yours).len() {
        0 => "Nothing is waiting on you.".to_string(),
        1 => "One task waits on you.".to_string(),
        n => format!("{n} tasks wait on you."),
    };
    let holding = (p.holding > 0).then(|| {
        format!(
            "{} of them hold{} up {} other{}.",
            p.holding,
            if p.holding == 1 { "s" } else { "" },
            p.held_up,
            if p.held_up == 1 { "" } else { "s" },
        )
    });
    let broke = match crate::lamp::alarms(s).len() {
        0 => None,
        1 => Some("Something broke.".to_string()),
        n => Some(format!("{n} things broke.")),
    };
    Headline {
        count,
        holding,
        broke,
    }
}

/// The Desk's three sections, each in the order it is shown.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Desk<'a> {
    /// Tasks of yours that free others, the most first.
    #[serde(serialize_with = "uuids")]
    pub frees: Vec<&'a Card>,
    /// Decisions that free nothing (one that does is under `frees`).
    #[serde(serialize_with = "uuids")]
    pub decisions: Vec<&'a Card>,
    /// The rest of your hands, grouped by project, the busiest first.
    #[serde(serialize_with = "uuids")]
    pub hands: Vec<&'a Card>,
    /// The most any one card frees, for the bars (at least 1).
    pub most: usize,
}

pub fn desk(s: &Snapshot) -> Desk<'_> {
    let yours = s.lane(Lane::Yours);
    let most = yours
        .iter()
        .map(|c| c.frees.len())
        .max()
        .unwrap_or(0)
        .max(1);
    let frees = yours
        .iter()
        .filter(|c| !c.frees.is_empty())
        .copied()
        .collect();
    let decisions = yours
        .iter()
        .filter(|c| c.decision && c.frees.is_empty())
        .copied()
        .collect();
    let rest: Vec<&Card> = yours
        .iter()
        .filter(|c| !c.decision && c.frees.is_empty())
        .copied()
        .collect();
    let mut projects: Vec<&str> = rest.iter().map(|c| c.project.as_str()).collect();
    projects.sort();
    projects.dedup();
    projects.sort_by_key(|p| std::cmp::Reverse(rest.iter().filter(|c| c.project == *p).count()));
    let hands = projects
        .iter()
        .flat_map(|p| rest.iter().filter(move |c| c.project == *p).copied())
        .collect();
    Desk {
        frees,
        decisions,
        hands,
        most,
    }
}

/// What the drawer (and the phone's widget) has room for.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Drawer<'a> {
    /// Open alarms, at most three: something broke.
    #[serde(serialize_with = "uuids")]
    pub alarms: Vec<&'a Card>,
    /// Decisions, at most three: a click settles them.
    #[serde(serialize_with = "uuids")]
    pub decisions: Vec<&'a Card>,
    /// The rest of your tasks, by what they free, to fill the rows.
    #[serde(serialize_with = "uuids")]
    pub rest: Vec<&'a Card>,
    /// How many of yours did not fit.
    pub more: usize,
}

pub fn drawer(s: &Snapshot) -> Drawer<'_> {
    let yours = s.lane(Lane::Yours);
    let alarms: Vec<&Card> = crate::lamp::alarms(s).into_iter().take(3).collect();
    let (decisions, rest): (Vec<&Card>, Vec<&Card>) =
        yours.iter().filter(|c| !c.alarm).partition(|c| c.decision);
    let decisions: Vec<&Card> = decisions.into_iter().take(3).collect();
    let shown = DRAWER_ROWS.saturating_sub(decisions.len());
    let rest: Vec<&Card> = rest.into_iter().take(shown).collect();
    let more = yours.len().saturating_sub(decisions.len() + rest.len());
    Drawer {
        alarms,
        decisions,
        rest,
        more,
    }
}

/// One lane of the board, in order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Column<'a> {
    pub lane: Lane,
    pub title: &'static str,
    pub empty: &'static str,
    #[serde(serialize_with = "uuids")]
    pub cards: Vec<&'a Card>,
}

pub fn columns(s: &Snapshot) -> Vec<Column<'_>> {
    LANES
        .iter()
        .map(|&(lane, title, empty)| Column {
            lane,
            title,
            empty,
            cards: s.lane(lane),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Zone;
    use crate::model::tests::NOW;

    fn demo() -> Snapshot {
        crate::demo::snapshot(NOW, Zone::Fixed(-6 * 3600))
    }

    #[test]
    fn the_desk_holds_every_card_of_yours_once() {
        let s = demo();
        let d = desk(&s);
        let shown = d.frees.len() + d.decisions.len() + d.hands.len();
        assert_eq!(shown, s.lane(Lane::Yours).len());
        assert_eq!(d.frees[0].title, "Playtest the lantern level");
        assert_eq!(d.most, 3);
        assert!(d.decisions.iter().all(|c| c.decision));
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["frees"][0], d.frees[0].uuid.as_str());
    }

    #[test]
    fn the_drawer_fits_its_rows() {
        let s = demo();
        let d = drawer(&s);
        assert!(d.decisions.len() <= 3);
        assert_eq!(d.decisions.len() + d.rest.len(), DRAWER_ROWS);
        assert_eq!(
            d.decisions.len() + d.rest.len() + d.more,
            s.lane(Lane::Yours).len()
        );
    }

    #[test]
    fn the_headline_counts_what_waits() {
        let h = headline(&demo());
        assert_eq!(h.count, "12 tasks wait on you.");
        assert_eq!(h.holding.as_deref(), Some("4 of them hold up 8 others."));
        assert_eq!(h.broke, None);
    }
}
