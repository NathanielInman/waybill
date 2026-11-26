//! Desk: the user's own queue, ordered by how much work each task frees.

use super::{Tx, open};
use crate::charts::{self, Canvas, Ring};
use crate::shell::{Msg, World};
use crate::theme::{self, Rgba};
use crate::ui::{self, chip, hbox, label, vbox};
use gtk::prelude::*;
use std::rc::Rc;
use waybill_core::model::{Card, Lane, Snapshot};
use waybill_core::plan;
use waybill_core::say::Action;

pub struct DeskView {
    pub root: gtk::Box,
    headline: gtk::Label,
    list: gtk::Box,
    ring: Canvas<Ring>,
    legend: gtk::Box,
    facts: gtk::Box,
    tx: Tx,
    world: Rc<World>,
}

impl DeskView {
    pub fn new(tx: Tx, world: Rc<World>) -> DeskView {
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
            world,
        }
    }

    pub fn set(&self, s: &Snapshot) {
        let pal = theme::current();
        let p = &s.pulse;
        let warn = pal.warn.hex();
        let head = plan::headline(s);
        let mut text = ui::esc(&head.count);
        if let Some(holding) = &head.holding {
            text.push_str(&format!(
                " <span foreground=\"{warn}\">{}</span>",
                ui::esc(holding)
            ));
        }
        self.headline.set_markup(&text);

        ui::clear(&self.list);
        let desk = plan::desk(s);
        let most = desk.most;
        let freeing = &desk.frees;
        if !freeing.is_empty() {
            self.list.append(&label("Frees the most", &["section"]));
            for c in freeing {
                self.list.append(&row(s, c, most, &self.tx));
            }
        }
        let decisions = &desk.decisions;
        if !decisions.is_empty() {
            self.list.append(&label("Decisions", &["section"]));
            for c in decisions {
                self.list.append(&row(s, c, most, &self.tx));
                // The options, one click each, straight from the note.
                let options = self.world.options(c);
                if !options.is_empty() {
                    let strip = ui::hbox(6);
                    strip.add_css_class("options");
                    for option in options.iter().take(4) {
                        let b = gtk::Button::with_label(option);
                        b.add_css_class("act");
                        b.add_css_class("choice");
                        if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                            l.set_max_width_chars(34);
                        }
                        b.set_tooltip_text(Some(option));
                        let (tx, uuid, choice) = (self.tx.clone(), c.uuid.clone(), option.clone());
                        b.connect_clicked(move |_| {
                            let _ = tx
                                .send_blocking(Msg::Do(Action::Rule(uuid.clone(), choice.clone())));
                        });
                        strip.append(&b);
                    }
                    self.list.append(&strip);
                }
            }
        }
        if !desk.hands.is_empty() {
            self.list
                .append(&label("Your hands, by project", &["section"]));
        }
        for c in &desk.hands {
            self.list.append(&row(s, c, most, &self.tx));
        }
        if s.lane(Lane::Yours).is_empty() {
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
        &match ui::age(s, c).as_str() {
            "now" => "new".to_string(),
            age => format!("open {age}"),
        },
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
