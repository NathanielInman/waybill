//! Desk: the user's own queue, ordered by how much work each task frees.

use super::{Tx, open};
use crate::charts::{self, Canvas, Ring};
use crate::theme::{self, Rgba};
use crate::ui::{self, chip, hbox, label, vbox};
use gtk::prelude::*;
use waybill_core::model::{Card, Lane, Snapshot};

pub struct DeskView {
    pub root: gtk::Box,
    headline: gtk::Label,
    list: gtk::Box,
    ring: Canvas<Ring>,
    legend: gtk::Box,
    facts: gtk::Box,
    tx: Tx,
}

impl DeskView {
    pub fn new(tx: Tx) -> DeskView {
        let root = hbox(16);
        root.add_css_class("desk");

        let main = vbox(14);
        let headline = ui::wrapped("", &["headline"], 3);
        let list = vbox(6);
        main.append(&headline);
        main.append(&ui::scrolled(&list));
        main.set_hexpand(true);

        let side = vbox(12);
        side.set_size_request(330, -1);
        let tile = vbox(10);
        tile.add_css_class("tile");
        tile.append(&label("Who the open work waits on", &["tile-title"]));
        let ring = Canvas::new(
            0,
            210,
            Ring {
                parts: vec![],
                big: String::new(),
                small: String::new(),
            },
            charts::ring,
        );
        tile.append(&ring.area);
        let legend = vbox(4);
        tile.append(&legend);
        let facts = vbox(8);
        side.append(&tile);
        side.append(&facts);

        root.append(&main);
        root.append(&side);
        DeskView {
            root,
            headline,
            list,
            ring,
            legend,
            facts,
            tx,
        }
    }

    pub fn set(&self, s: &Snapshot) {
        let pal = theme::current();
        let p = &s.pulse;
        let yours = s.lane(Lane::Yours);
        let warn = pal.warn.hex();
        let mut text = match yours.len() {
            0 => "Nothing is waiting on you.".to_string(),
            1 => "One task waits on you.".to_string(),
            n => format!("{n} tasks wait on you."),
        };
        if p.holding > 0 {
            text.push_str(&format!(
                " <span foreground=\"{warn}\">{} of them hold{} up {} other{}.</span>",
                p.holding,
                if p.holding == 1 { "s" } else { "" },
                p.held_up,
                if p.held_up == 1 { "" } else { "s" },
            ));
        }
        self.headline.set_markup(&text);

        ui::clear(&self.list);
        let most = yours
            .iter()
            .map(|c| c.frees.len())
            .max()
            .unwrap_or(0)
            .max(1);
        let freeing: Vec<&&Card> = yours.iter().filter(|c| !c.frees.is_empty()).collect();
        if !freeing.is_empty() {
            self.list.append(&label("Frees the most", &["section"]));
            for c in freeing {
                self.list.append(&row(s, c, most, &self.tx));
            }
        }
        let decisions: Vec<&&Card> = yours
            .iter()
            .filter(|c| c.decision && c.frees.is_empty())
            .collect();
        if !decisions.is_empty() {
            self.list.append(&label("Decisions", &["section"]));
            for c in decisions {
                self.list.append(&row(s, c, most, &self.tx));
            }
        }
        let rest: Vec<&&Card> = yours
            .iter()
            .filter(|c| !c.decision && c.frees.is_empty())
            .collect();
        let mut projects: Vec<&str> = rest.iter().map(|c| c.project.as_str()).collect();
        projects.sort();
        projects.dedup();
        // The project with the most to do comes first.
        projects
            .sort_by_key(|p| std::cmp::Reverse(rest.iter().filter(|c| c.project == *p).count()));
        if !rest.is_empty() {
            self.list
                .append(&label("Your hands, by project", &["section"]));
        }
        for project in projects {
            for c in rest.iter().filter(|c| c.project == project) {
                self.list.append(&row(s, c, most, &self.tx));
            }
        }
        if yours.is_empty() {
            self.list.append(&ui::wrapped(
                "Tasks tagged for your hands, and decisions, show up here.",
                &["empty"],
                3,
            ));
        }

        let lanes = p.lanes;
        let parts: [(&str, u32, Rgba); 5] = [
            ("you", lanes.yours, pal.warn),
            ("agents, ready now", lanes.ready, pal.accent),
            ("agents, started", lanes.doing, pal.ok),
            ("another task", lanes.held, pal.dim),
            ("a later date", lanes.parked, pal.dim.with_alpha(0.45)),
        ];
        self.ring.set(Ring {
            parts: parts.iter().map(|(_, v, c)| (*v as f64, *c)).collect(),
            big: lanes.open().to_string(),
            small: "open tasks".into(),
        });
        ui::clear(&self.legend);
        for (name, v, color) in parts {
            let line = hbox(8);
            line.append(&ui::markup(
                &format!(
                    "<span foreground=\"{}\">●</span> {name}",
                    color.with_alpha(1.0).hex()
                ),
                &[],
            ));
            line.append(&ui::spring());
            line.append(&label(&v.to_string(), &["num", "bright"]));
            self.legend.append(&line);
        }

        ui::clear(&self.facts);
        let fact = |big: String, small: &str| {
            let t = vbox(2);
            t.add_css_class("tile");
            t.append(&label(&big, &["fact", "num"]));
            t.append(&ui::wrapped(small, &["dim"], 2));
            t
        };
        self.facts.append(&fact(
            p.decisions_open.to_string(),
            if p.decisions_open == 1 {
                "decision is open"
            } else {
                "decisions are open"
            },
        ));
        if let Some(h) = p.decision_hours {
            let shown = if h >= 48.0 {
                format!("{:.0} d", h / 24.0)
            } else {
                format!("{h:.0} h")
            };
            self.facts.append(&fact(
                shown,
                "a decision stays open, at the median over six weeks",
            ));
        }
    }

    pub fn play(&self) {
        self.ring.play();
    }
}

fn row(s: &Snapshot, c: &Card, most: usize, tx: &Tx) -> gtk::Button {
    let pal = theme::current();
    let line = hbox(14);

    let mark = vbox(0);
    mark.set_size_request(64, -1);
    mark.set_valign(gtk::Align::Center);
    if !c.frees.is_empty() {
        let n = label(&c.frees.len().to_string(), &["big", "num", "warn"]);
        n.set_xalign(0.5);
        let w = label("frees", &["small", "warn"]);
        w.set_xalign(0.5);
        mark.append(&n);
        mark.append(&w);
    } else {
        let glyph = if c.decision { "◆" } else { "■" };
        let l = label(
            glyph,
            &["glyph", if c.decision { "violet" } else { "warn" }],
        );
        l.set_xalign(0.5);
        mark.append(&l);
    }
    line.append(&mark);

    let mid = vbox(4);
    mid.set_hexpand(true);
    mid.append(&ui::wrapped(&c.title, &["row-title"], 2));
    let meta = hbox(6);
    meta.append(&ui::project(&c.project));
    if let Some(p) = ui::priority(c.priority) {
        meta.append(&p);
    }
    if c.decision {
        meta.append(&chip("decision", "violet"));
    }
    if c.blocked {
        meta.append(&chip(&format!("waits on {}", c.waits_on.len()), "plain"));
    }
    meta.append(&label(
        &format!("open {}", ui::age(s, c)),
        &["dim", "small"],
    ));
    mid.append(&meta);
    if !c.frees.is_empty() {
        let names: Vec<&str> = c
            .frees
            .iter()
            .filter_map(|u| s.card(u))
            .map(|f| f.title.as_str())
            .collect();
        mid.append(&ui::wrapped(
            &format!("then: {}", names.join(" · ")),
            &["dim", "small"],
            2,
        ));
    }
    line.append(&mid);

    if !c.frees.is_empty() {
        let bar = Canvas::new(
            130,
            8,
            vec![
                (c.frees.len() as f64, pal.warn),
                ((most - c.frees.len()) as f64, pal.fg.with_alpha(0.0)),
            ],
            |f, parts: &Vec<(f64, Rgba)>| charts::split(f, parts),
        );
        bar.area.set_valign(gtk::Align::Center);
        line.append(&bar.area);
    }

    let b = ui::row_button(&line, &["row"]);
    let (tx, uuid) = (tx.clone(), c.uuid.clone());
    b.connect_clicked(move |_| open(&tx, &uuid));
    b
}
