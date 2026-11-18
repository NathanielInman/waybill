//! Board: every open task in the lane it stands in, plus what closed today.

use super::{Tx, open};
use crate::charts::{self, Canvas};
use crate::theme::{self, Rgba};
use crate::ui::{self, chip, hbox, label, vbox};
use gtk::prelude::*;
use waybill_core::clock;
use waybill_core::model::{Card, Lane, Snapshot};

struct Column {
    lane: Lane,
    count: gtk::Label,
    mix: Canvas<Vec<(f64, Rgba)>>,
    list: gtk::Box,
}

pub struct BoardView {
    pub root: gtk::Box,
    columns: Vec<Column>,
    tx: Tx,
}

const COLUMNS: [(Lane, &str, &str, &str); 5] = [
    (
        Lane::Ready,
        "Ready for agents",
        "accent",
        "Nothing is waiting for an agent.",
    ),
    (Lane::Doing, "Doing", "ok", "Nothing is started."),
    (
        Lane::Yours,
        "Needs you",
        "warn",
        "Nothing needs your hands.",
    ),
    (
        Lane::Held,
        "Held",
        "plain",
        "Nothing waits on another task.",
    ),
    (
        Lane::Done,
        "Done today",
        "done",
        "Nothing has closed yet today.",
    ),
];

impl BoardView {
    pub fn new(tx: Tx) -> BoardView {
        let root = hbox(12);
        root.add_css_class("board");
        root.set_homogeneous(true);
        let columns = COLUMNS
            .iter()
            .map(|(lane, title, class, _)| {
                let col = vbox(8);
                col.add_css_class("lane");
                let head = hbox(8);
                head.add_css_class("lane-head");
                head.append(&label("●", &["lamp", class]));
                head.append(&label(title, &["lane-title"]));
                head.append(&ui::spring());
                let count = label("0", &["lane-count", "num"]);
                head.append(&count);
                let mix = Canvas::new(0, 4, Vec::new(), |f, parts: &Vec<(f64, Rgba)>| {
                    charts::split(f, parts)
                });
                mix.area
                    .set_tooltip_text(Some("The lane by priority: high, medium, low"));
                let list = vbox(8);
                list.add_css_class("lane-list");
                col.append(&head);
                col.append(&mix.area);
                col.append(&ui::scrolled(&list));
                root.append(&col);
                Column {
                    lane: *lane,
                    count,
                    mix,
                    list,
                }
            })
            .collect();
        BoardView { root, columns, tx }
    }

    pub fn set(&self, s: &Snapshot) {
        let pal = theme::current();
        for (col, (_, _, _, empty)) in self.columns.iter().zip(COLUMNS) {
            let cards = s.lane(col.lane);
            col.count.set_label(&cards.len().to_string());
            let of = |p: char| cards.iter().filter(|c| c.priority == p).count() as f64;
            let rest = cards.len() as f64 - of('H') - of('M') - of('L');
            col.mix.set(vec![
                (of('H'), pal.bad),
                (of('M'), pal.fg.with_alpha(0.55)),
                (of('L') + rest, pal.dim.with_alpha(0.45)),
            ]);
            ui::clear(&col.list);
            if cards.is_empty() {
                col.list.append(&ui::wrapped(empty, &["empty"], 3));
            }
            for c in cards {
                col.list.append(&card(s, c, &self.tx));
            }
            if col.lane == Lane::Held {
                let parked = s.lane(Lane::Parked);
                if !parked.is_empty() {
                    let head = hbox(8);
                    head.add_css_class("sub-head");
                    head.append(&label("Parked until later", &["dim"]));
                    head.append(&ui::spring());
                    head.append(&label(&parked.len().to_string(), &["dim", "num"]));
                    col.list.append(&head);
                    for c in parked {
                        let b = card(s, c, &self.tx);
                        b.add_css_class("parked");
                        col.list.append(&b);
                    }
                }
            }
        }
    }

    pub fn play(&self) {
        for c in &self.columns {
            c.mix.play();
        }
    }
}

/// One task as a card. The whole card is a button that opens it.
pub fn card(s: &Snapshot, c: &Card, tx: &Tx) -> gtk::Button {
    let body = vbox(7);
    body.append(&ui::wrapped(&c.title, &["card-title"], 3));

    let meta = hbox(6);
    meta.append(&ui::project(&c.project));
    if let Some(p) = ui::priority(c.priority) {
        meta.append(&p);
    }
    if c.decision {
        meta.append(&chip("decision", "violet"));
    }
    for t in c.tags.iter().take(2) {
        meta.append(&chip(t, ui::tag_class(t)));
    }
    meta.append(&ui::spring());
    match c.lane {
        Lane::Doing => {
            if c.yours {
                meta.append(&chip("you", "warn"));
            }
        }
        Lane::Done | Lane::Past => {
            if let Some(end) = c.ended {
                meta.append(&label(&ui::when(s, end), &["num", "dim"]));
            }
        }
        _ => {
            if !c.frees.is_empty() {
                let (word, class) = if c.yours {
                    ("frees", "warn")
                } else {
                    ("opens", "accent")
                };
                meta.append(&chip(&format!("{word} {}", c.frees.len()), class));
            } else if c.blocked {
                meta.append(&chip(&format!("waits on {}", c.waits_on.len()), "plain"));
            }
        }
    }
    body.append(&meta);

    let doing = c.lane == Lane::Doing;
    if doing || (!c.visits.is_empty() && !matches!(c.lane, Lane::Done | Lane::Past)) {
        let foot = hbox(6);
        if doing {
            foot.append(&label("●", &["lamp", "ok", "live"]));
            foot.append(&label(
                &format!("held {}", ui::age(s, c)),
                &["num", "ok", "small"],
            ));
        }
        let n = c.visits.len();
        if n > 0 {
            let last = c.visits.iter().map(|v| v.at).max().unwrap_or(c.entry);
            let visits = format!("{n} {}", if n == 1 { "visit" } else { "visits" });
            let text = if doing {
                format!("· {visits}")
            } else {
                let ago = match clock::span(s.now - last).as_str() {
                    "now" => "just now".to_string(),
                    span => format!("{span} ago"),
                };
                format!("{visits} · last {ago}")
            };
            foot.append(&label(&text, &["dim", "small"]));
        }
        foot.append(&ui::spring());
        if c.activity.iter().any(|v| *v > 0) {
            let bars = Canvas::new(48, 12, c.activity.clone(), |f, d: &Vec<u16>| {
                charts::activity(f, d)
            });
            bars.area.set_valign(gtk::Align::Center);
            foot.append(&bars.area);
        }
        body.append(&foot);
    }

    let b = ui::row_button(&body, &["card"]);
    if c.yours {
        b.add_css_class("yours");
    }
    if c.lane == Lane::Doing {
        b.add_css_class("doing");
    }
    let (tx, uuid) = (tx.clone(), c.uuid.clone());
    b.connect_clicked(move |_| open(&tx, &uuid));
    b
}
