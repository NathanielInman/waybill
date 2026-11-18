//! The card pane: one task in full. Its note laid out beside its visits,
//! what it waits on and what it frees.

use crate::shell::Msg;
use crate::ui::{self, chip, esc, hbox, label, vbox};
use crate::views::Tx;
use gtk::prelude::*;
use waybill_core::clock;
use waybill_core::model::{Card, Lane, Snapshot, is_ruling};
use waybill_core::note::{self, Block};

pub struct Detail {
    pub root: gtk::Box,
    body: gtk::Box,
    tx: Tx,
}

impl Detail {
    pub fn new(tx: Tx) -> Detail {
        let root = vbox(0);
        root.add_css_class("detail");
        root.set_size_request(460, -1);
        let body = vbox(14);
        root.append(&ui::scrolled(&body));
        Detail { root, body, tx }
    }

    pub fn show(&self, s: &Snapshot, c: &Card, note_text: Option<&str>) {
        ui::clear(&self.body);

        let top = hbox(6);
        top.append(&ui::project(&c.project));
        if let Some(p) = ui::priority(c.priority) {
            top.append(&p);
        }
        if c.decision {
            top.append(&chip("decision", "violet"));
        }
        for t in &c.tags {
            top.append(&chip(t, ui::tag_class(t)));
        }
        top.append(&ui::spring());
        let close = gtk::Button::with_label("Close  Esc");
        close.add_css_class("ghost");
        let tx = self.tx.clone();
        close.connect_clicked(move |_| {
            let _ = tx.send_blocking(Msg::Close);
        });
        top.append(&close);
        self.body.append(&top);

        let title = ui::wrapped(&c.title, &["detail-title"], 6);
        title.set_selectable(true);
        self.body.append(&title);

        let (state, class) = match c.lane {
            Lane::Ready => ("ready for an agent", "accent"),
            Lane::Doing => ("started", "ok"),
            Lane::Yours if c.blocked => ("yours, once the line reaches it", "warn"),
            Lane::Yours => ("needs you", "warn"),
            Lane::Held => ("held by another task", "plain"),
            Lane::Parked => ("parked until later", "plain"),
            Lane::Done | Lane::Past => ("finished", "ok"),
        };
        let facts = gtk::Grid::new();
        facts.set_column_spacing(14);
        facts.set_row_spacing(5);
        facts.add_css_class("facts");
        let mut at = 0;
        let mut fact = |key: &str, value: gtk::Widget| {
            facts.attach(&label(key, &["dim"]), 0, at, 1, 1);
            facts.attach(&value, 1, at, 1, 1);
            at += 1;
        };
        let state_line = hbox(6);
        state_line.append(&label("●", &["lamp", class]));
        state_line.append(&label(state, &[]));
        fact("state", state_line.upcast());
        if let Some(start) = c.started.filter(|_| c.ended.is_none()) {
            fact(
                "held for",
                label(
                    &format!(
                        "{} (since {})",
                        clock::span(s.now - start),
                        ui::when(s, start)
                    ),
                    &["num"],
                )
                .upcast(),
            );
        }
        if let Some(end) = c.ended {
            fact("finished", label(&ui::when(s, end), &["num"]).upcast());
        }
        fact(
            "added",
            label(&format!("{} ago", clock::span(s.now - c.entry)), &["num"]).upcast(),
        );
        let uuid = label(c.short(), &["num", "mono"]);
        uuid.set_selectable(true);
        uuid.set_tooltip_text(Some(&c.uuid));
        fact("uuid", uuid.upcast());
        if let Some(r) = &c.reference {
            fact("marker", chip(r, "sha").upcast());
        }
        self.body.append(&facts);

        self.related(s, "Frees when it closes", &c.frees);
        self.related(s, "Waits on", &c.waits_on);

        self.body.append(&label("Note", &["section"]));
        match note_text {
            Some(text) => self.body.append(&render_note(text)),
            None => self.body.append(&ui::wrapped(
                "No note. The card title carries everything.",
                &["empty"],
                2,
            )),
        }

        self.body.append(&label(
            &match c.visits.len() {
                0 => "Visits".to_string(),
                n => format!("Visits ({n})"),
            },
            &["section"],
        ));
        if c.visits.is_empty() {
            self.body.append(&ui::wrapped(
                "Nobody has logged a visit yet.",
                &["empty"],
                2,
            ));
        }
        let rail = vbox(0);
        rail.add_css_class("visits");
        for v in c.visits.iter().rev() {
            let item = vbox(3);
            item.add_css_class("visit");
            let head = hbox(8);
            let ruling = is_ruling(&v.text, &s.you);
            head.append(&label(
                if ruling { "◆" } else { "●" },
                &["lamp", if ruling { "violet" } else { "accent" }],
            ));
            head.append(&label(&ui::when(s, v.at), &["num", "dim", "small"]));
            for sha in &v.shas {
                head.append(&chip(sha, "sha"));
            }
            item.append(&head);
            let text = ui::wrapped(&v.text, &["said"], 12);
            text.set_selectable(true);
            item.append(&text);
            rail.append(&item);
        }
        self.body.append(&rail);
    }

    fn related(&self, s: &Snapshot, title: &str, uuids: &[String]) {
        let cards: Vec<&Card> = uuids.iter().filter_map(|u| s.card(u)).collect();
        if cards.is_empty() {
            return;
        }
        self.body.append(&label(title, &["section"]));
        let list = vbox(2);
        for c in cards {
            let line = hbox(8);
            let lamp = if c.yours {
                ("■", "warn")
            } else {
                ("●", "accent")
            };
            line.append(&label(lamp.0, &["lamp", lamp.1]));
            let t = label(&c.title, &[]);
            t.set_ellipsize(gtk::pango::EllipsizeMode::End);
            t.set_hexpand(true);
            line.append(&t);
            let b = ui::row_button(&line, &["row", "slim"]);
            let (tx, uuid) = (self.tx.clone(), c.uuid.clone());
            b.connect_clicked(move |_| {
                let _ = tx.send_blocking(Msg::Open(uuid.clone()));
            });
            list.append(&b);
        }
        self.body.append(&list);
    }
}

/// Inline `code` and **bold** as Pango markup.
fn inline(text: &str) -> String {
    note::spans(text)
        .iter()
        .map(|s| {
            let t = esc(&s.text);
            match (s.code, s.bold) {
                (true, _) => format!("<span background=\"#00000048\">{t}</span>"),
                (false, true) => format!("<b>{t}</b>"),
                _ => t,
            }
        })
        .collect()
}

fn render_note(text: &str) -> gtk::Box {
    let out = vbox(7);
    out.add_css_class("note");
    let mut blocks = note::parse(text).into_iter().peekable();
    // The first heading repeats the card title.
    if matches!(blocks.peek(), Some(Block::Heading(1, _))) {
        blocks.next();
    }
    for block in blocks {
        match block {
            Block::Heading(_, t) => out.append(&ui::wrapped(&t, &["note-head"], 3)),
            Block::Para(t) => {
                let l = ui::wrapped("", &["note-text"], 40);
                l.set_markup(&inline(&t));
                l.set_selectable(true);
                out.append(&l);
            }
            Block::Bullet { depth, check, text } => {
                let line = hbox(8);
                line.set_margin_start(depth as i32 * 16);
                let (glyph, class) = match check {
                    Some(true) => ("☑", "ok"),
                    Some(false) => ("☐", "dim"),
                    None => ("•", "dim"),
                };
                let g = label(glyph, &[class]);
                g.set_valign(gtk::Align::Start);
                line.append(&g);
                let l = ui::wrapped("", &["note-text"], 40);
                l.set_markup(&inline(&text));
                l.set_selectable(true);
                if check == Some(true) {
                    l.add_css_class("dim");
                }
                line.append(&l);
                out.append(&line);
            }
            Block::Code(t) => {
                let l = label(&t, &["code"]);
                l.set_selectable(true);
                l.set_wrap(true);
                l.set_wrap_mode(gtk::pango::WrapMode::Char);
                out.append(&l);
            }
        }
    }
    out
}
