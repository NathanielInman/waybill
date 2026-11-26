//! What `waybill serve` hands a phone. A screen is the snapshot plus every
//! choice the board makes when it draws (the lanes in order, the Desk's
//! sections, what fits in the drawer, the headline), the lamp, the palette,
//! each project's color and every open decision's options. The phone draws
//! it and decides nothing, so a rule changed in the core reaches it with no
//! work on the phone.

use crate::lamp;
use crate::model::{Card, Snapshot};
use crate::note::{self, Block, Span};
use crate::plan::{self, Column, Headline};
use crate::pulse::LaneCounts;
use crate::say::{self, Offer, View};
use crate::theme::{Palette, Rgba};
use serde::Serialize;
use std::collections::BTreeMap;

/// The wire format's version. A change that would break an older phone
/// raises it.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LampLine {
    /// `calm`, `needs-you`, `alarm` or `error`.
    pub state: &'static str,
    pub why: String,
}

fn lamp_line(s: &Snapshot) -> LampLine {
    let l = lamp::lamp(s);
    LampLine {
        state: l.state.class(),
        why: l.why,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ViewLine {
    pub id: &'static str,
    pub name: &'static str,
    pub question: &'static str,
}

/// Everything the phone's five views draw.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Screen<'a> {
    pub wire: u32,
    /// Goes up each time the list changes; the phone asks for anything
    /// newer than the one it has.
    pub version: u64,
    /// The demo list, which cannot be changed.
    pub read_only: bool,
    pub snapshot: &'a Snapshot,
    pub lamp: LampLine,
    pub headline: Headline,
    pub columns: Vec<Column<'a>>,
    pub desk: plan::Desk<'a>,
    pub drawer: plan::Drawer<'a>,
    pub views: Vec<ViewLine>,
    /// Each open decision's options, by uuid.
    pub options: BTreeMap<String, Vec<String>>,
    pub palette: &'a Palette,
    /// Each project's color, by name.
    pub projects: BTreeMap<String, Rgba>,
}

/// The choices of every open decision, read from their notes by `note_of`.
pub fn options(
    s: &Snapshot,
    note_of: impl Fn(&Card) -> Option<String>,
) -> BTreeMap<String, Vec<String>> {
    s.cards
        .iter()
        .filter(|c| c.decision && c.ended.is_none())
        .map(|c| {
            let options = note_of(c).as_deref().map(note::options).unwrap_or_default();
            (c.uuid.clone(), options)
        })
        .collect()
}

pub fn screen<'a>(
    s: &'a Snapshot,
    palette: &'a Palette,
    options: BTreeMap<String, Vec<String>>,
    read_only: bool,
) -> Screen<'a> {
    let projects = s
        .cards
        .iter()
        .map(|c| c.project.as_str())
        .chain(s.lines.iter().map(|l| l.project.as_str()))
        .chain(s.pulse.projects.iter().map(|p| p.name.as_str()))
        .map(|p| (p.to_string(), palette.project(p)))
        .collect();
    Screen {
        wire: VERSION,
        version: 0,
        read_only,
        snapshot: s,
        lamp: lamp_line(s),
        headline: plan::headline(s),
        columns: plan::columns(s),
        desk: plan::desk(s),
        drawer: plan::drawer(s),
        views: View::ALL
            .iter()
            .map(|v| ViewLine {
                id: v.id(),
                name: v.name(),
                question: v.question(),
            })
            .collect(),
        options,
        palette,
        projects,
    }
}

/// The short version for the phone's widget, its tile and its
/// notifications: the drawer's cards in full, and the week in numbers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Glance<'a> {
    pub wire: u32,
    pub version: u64,
    pub lamp: LampLine,
    pub headline: Headline,
    pub counts: LaneCounts,
    pub done_week: u32,
    pub added_week: u32,
    /// Finished per day over the last two weeks, oldest first.
    pub spark: Vec<u32>,
    pub alarms: Vec<&'a Card>,
    pub decisions: Vec<&'a Card>,
    pub rest: Vec<&'a Card>,
    pub more: usize,
    pub options: BTreeMap<String, Vec<String>>,
}

pub fn glance<'a>(s: &'a Snapshot, options: &BTreeMap<String, Vec<String>>) -> Glance<'a> {
    let d = plan::drawer(s);
    let p = &s.pulse;
    let n = p.days.len();
    Glance {
        wire: VERSION,
        version: 0,
        lamp: lamp_line(s),
        headline: plan::headline(s),
        counts: p.lanes,
        done_week: p.done_week,
        added_week: p.added_week,
        spark: p
            .days
            .iter()
            .skip(n.saturating_sub(14))
            .map(|d| d.done)
            .collect(),
        options: d
            .decisions
            .iter()
            .filter_map(|c| Some((c.uuid.clone(), options.get(&c.uuid)?.clone())))
            .collect(),
        alarms: d.alarms,
        decisions: d.decisions,
        rest: d.rest,
        more: d.more,
    }
}

/// One block of a note, laid out by `note.rs` and split into spans.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoteBlock {
    /// `heading`, `para`, `bullet` or `code`.
    pub kind: &'static str,
    /// A heading's level, a bullet's depth.
    pub level: u8,
    /// `Some` for a checkbox.
    pub check: Option<bool>,
    pub spans: Vec<Span>,
}

pub fn note_blocks(md: &str) -> Vec<NoteBlock> {
    note::parse(md)
        .into_iter()
        .map(|b| match b {
            Block::Heading(level, text) => NoteBlock {
                kind: "heading",
                level,
                check: None,
                spans: note::spans(&text),
            },
            Block::Para(text) => NoteBlock {
                kind: "para",
                level: 0,
                check: None,
                spans: note::spans(&text),
            },
            Block::Bullet { depth, check, text } => NoteBlock {
                kind: "bullet",
                level: depth,
                check,
                spans: note::spans(&text),
            },
            Block::Code(text) => NoteBlock {
                kind: "code",
                level: 0,
                check: None,
                spans: vec![Span {
                    text,
                    code: true,
                    bold: false,
                }],
            },
        })
        .collect()
}

/// One card opened: its note and the buttons it offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Page {
    pub uuid: String,
    pub note: Vec<NoteBlock>,
    pub offers: Vec<Offer>,
}

pub fn page(c: &Card, note_text: Option<&str>) -> Page {
    let options = if c.decision {
        note_text.map(note::options).unwrap_or_default()
    } else {
        Vec::new()
    };
    Page {
        uuid: c.uuid.clone(),
        note: note_text.map(note_blocks).unwrap_or_default(),
        offers: say::offers(c, &options),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Zone;
    use crate::model::tests::NOW;

    #[test]
    fn a_screen_names_what_the_board_would_draw() {
        let s = crate::demo::snapshot(NOW, Zone::Fixed(-6 * 3600));
        let pal = Palette::default();
        let opts = options(&s, |c| Some(crate::demo::note(&c.title)));
        assert!(
            opts.values()
                .any(|o| o.contains(&"No offer at launch".to_string())),
            "the launch discount's options come from its note"
        );
        let json = serde_json::to_value(screen(&s, &pal, opts.clone(), true)).unwrap();
        assert_eq!(json["lamp"]["state"], "needs-you");
        assert_eq!(json["columns"][0]["lane"], "ready");
        assert_eq!(json["palette"]["warn"], "#ebcb8bff");
        assert_eq!(json["views"][1]["id"], "desk");
        let g = serde_json::to_value(glance(&s, &opts)).unwrap();
        assert_eq!(g["spark"].as_array().unwrap().len(), 14);
        assert!(g["decisions"][0]["title"].is_string());
    }

    #[test]
    fn a_page_carries_the_note_and_the_buttons() {
        let s = crate::demo::snapshot(NOW, Zone::Fixed(-6 * 3600));
        let c = s
            .cards
            .iter()
            .find(|c| c.title == "Playtest the lantern level")
            .unwrap();
        let p = page(c, Some(&crate::demo::note(&c.title)));
        assert!(p.note.iter().any(|b| b.check == Some(false)));
        assert_eq!(p.offers[0].label, "Start");
    }
}
