//! Log: every visit, ruling and finished task across all projects, newest
//! first, under a week of activity by the hour.

use super::{Tx, open};
use crate::charts::{self, Canvas, Ribbon};
use crate::ui::{self, chip, hbox, label, vbox};
use gtk::prelude::*;
use waybill_core::clock::{self, DAY, HOUR};
use waybill_core::model::{Event, Kind, Snapshot};

/// More rows than this and the list stops; the ribbon still counts them all.
const ROWS: usize = 220;

pub struct LogView {
    pub root: gtk::Box,
    summary: gtk::Label,
    ribbon: Canvas<Ribbon>,
    list: gtk::Box,
    tx: Tx,
}

impl LogView {
    pub fn new(tx: Tx) -> LogView {
        let root = vbox(12);
        root.add_css_class("log");
        let tile = vbox(6);
        tile.add_css_class("tile");
        let head = hbox(8);
        head.append(&label("The last seven days, by the hour", &["tile-title"]));
        head.append(&ui::spring());
        let summary = label("", &["dim"]);
        head.append(&summary);
        tile.append(&head);
        let ribbon = Canvas::new(
            0,
            96,
            Ribbon {
                visits: vec![],
                done: vec![],
                days: vec![],
            },
            charts::ribbon,
        );
        tile.append(&ribbon.area);
        let list = vbox(2);
        root.append(&tile);
        root.append(&ui::scrolled(&list));
        LogView {
            root,
            summary,
            ribbon,
            list,
            tx,
        }
    }

    pub fn set(&self, s: &Snapshot) {
        // Hours are counted on the local clock so the day marks fall on
        // local midnights.
        let local = |ts: i64| ts + s.offset;
        let this_hour = local(s.now).div_euclid(HOUR);
        let hours = 7 * 24;
        let mut visits = vec![0u32; hours];
        let mut done = vec![0u32; hours];
        for e in &s.log {
            let back = this_hour - local(e.at).div_euclid(HOUR);
            if (0..hours as i64).contains(&back) {
                let i = hours - 1 - back as usize;
                match e.kind {
                    Kind::Visit | Kind::Ruling => visits[i] += 1,
                    Kind::Done => done[i] += 1,
                    Kind::Added => {}
                }
            }
        }
        let first_hour = this_hour - hours as i64 + 1;
        let days = (0..hours)
            .filter(|i| (first_hour + *i as i64).rem_euclid(24) == 0)
            .map(|i| (i, clock::day_label((first_hour + i as i64).div_euclid(24))))
            .collect();
        self.summary.set_label(&format!(
            "{} visits · {} finished · {} added",
            s.pulse.visits_week, s.pulse.done_week, s.pulse.added_week
        ));
        self.ribbon.set(Ribbon { visits, done, days });

        ui::clear(&self.list);
        let today = local(s.now).div_euclid(DAY);
        let mut shown_day = None;
        let events: Vec<&Event> = s
            .log
            .iter()
            .filter(|e| e.kind != Kind::Added)
            .take(ROWS)
            .collect();
        if events.is_empty() {
            self.list.append(&ui::wrapped(
                "Nothing has been visited this week.",
                &["empty"],
                2,
            ));
        }
        for e in events {
            let day = local(e.at).div_euclid(DAY);
            if shown_day != Some(day) {
                shown_day = Some(day);
                let name = match today - day {
                    0 => "Today".to_string(),
                    1 => "Yesterday".to_string(),
                    _ => format!(
                        "{} {}",
                        clock::WEEKDAYS[clock::weekday(day)],
                        clock::day_label(day)
                    ),
                };
                self.list.append(&label(&name, &["section"]));
            }
            self.list.append(&row(s, e, &self.tx));
        }
    }

    pub fn play(&self) {
        self.ribbon.play();
    }
}

fn row(s: &Snapshot, e: &Event, tx: &Tx) -> gtk::Button {
    let line = hbox(12);
    let time = label(&ui::hm(s, e.at), &["num", "dim", "time"]);
    time.set_valign(gtk::Align::Start);
    line.append(&time);
    // A plain visit needs no mark; a finish and a ruling stand out.
    let (glyph, class) = match e.kind {
        Kind::Done => ("✓", "ok"),
        Kind::Ruling => ("◆", "violet"),
        _ => ("", "dim"),
    };
    let lamp = label(glyph, &["mark", class]);
    lamp.set_xalign(0.5);
    lamp.set_valign(gtk::Align::Start);
    line.append(&lamp);

    let body = vbox(3);
    body.set_hexpand(true);
    let head = hbox(8);
    head.append(&ui::project(&e.project));
    let title = label(&e.title, &["row-title"]);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_hexpand(true);
    head.append(&title);
    if e.kind == Kind::Done {
        head.append(&chip("finished", "ok"));
    } else if e.kind == Kind::Ruling {
        head.append(&chip("ruling", "violet"));
    }
    for sha in e.shas.iter().take(3) {
        head.append(&chip(sha, "sha"));
    }
    body.append(&head);
    if !e.text.is_empty() {
        body.append(&ui::wrapped(&e.text, &["said"], 3));
    }
    line.append(&body);

    let b = ui::row_button(&line, &["row", "log-row"]);
    let (tx, uuid) = (tx.clone(), e.uuid.clone());
    b.connect_clicked(move |_| open(&tx, &uuid));
    b
}
