//! Lines: each dependency chain as a rail line. Round stops are for agents,
//! square stops need the user's hands.

use super::{Tx, open};
use crate::charts::{self, Canvas, Rail};
use crate::theme::{self, Rgba};
use crate::ui::{self, hbox, label, vbox};
use gtk::prelude::*;
use std::cell::RefCell;
use waybill_core::lines::StopState;
use waybill_core::model::Snapshot;

pub struct LinesView {
    pub root: gtk::Box,
    list: gtk::Box,
    flow: gtk::FlowBox,
    rails: RefCell<Vec<Canvas<Rail>>>,
    tx: Tx,
}

impl LinesView {
    pub fn new(tx: Tx) -> LinesView {
        let root = vbox(0);
        root.add_css_class("lines");
        // Short lines sit side by side; a long one takes the row.
        let flow = gtk::FlowBox::new();
        flow.add_css_class("line-flow");
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_homogeneous(false);
        flow.set_column_spacing(14);
        flow.set_row_spacing(14);
        flow.set_max_children_per_line(4);
        flow.set_valign(gtk::Align::Start);
        let list = vbox(14);
        list.append(&flow);
        root.append(&ui::scrolled(&list));
        LinesView {
            root,
            list,
            flow,
            rails: RefCell::new(vec![]),
            tx,
        }
    }

    pub fn set(&self, s: &Snapshot) {
        let pal = theme::current();
        while let Some(c) = self
            .list
            .last_child()
            .filter(|c| c != self.flow.upcast_ref::<gtk::Widget>())
        {
            self.list.remove(&c);
        }
        self.flow.remove_all();
        let mut rails = Vec::new();
        if s.lines.is_empty() {
            self.list.append(&ui::wrapped(
                "No task waits on another. A line appears when one task depends on the next.",
                &["empty"],
                3,
            ));
        }
        for line in &s.lines {
            let panel = vbox(8);
            panel.add_css_class("tile");
            let head = hbox(10);
            head.append(&label(&line.name, &["line-name"]));
            head.append(&ui::project(&line.project));
            head.append(&ui::spring());
            let total = line.stops.len();
            let done = line
                .stops
                .iter()
                .filter(|s| s.state == StopState::Done)
                .count();
            let yours = line
                .stops
                .iter()
                .filter(|s| s.yours && s.state != StopState::Done)
                .count();
            let mut summary = format!("{} of {total} stops to go", total - done);
            if yours > 0 {
                summary.push_str(&format!(
                    " · {yours} need{} you",
                    if yours == 1 { "s" } else { "" }
                ));
            }
            head.append(&label(&summary, &["dim"]));
            panel.append(&head);

            let color = pal.project(&line.project);
            let progress = Canvas::new(
                0,
                4,
                vec![
                    (done as f64, color),
                    ((total - done) as f64, pal.fg.with_alpha(0.0)),
                ],
                |f, parts: &Vec<(f64, Rgba)>| charts::split(f, parts),
            );
            panel.append(&progress.area);

            let (w, h) = charts::rail_size(line);
            let rail = Canvas::new(
                w,
                h,
                Rail {
                    line: line.clone(),
                    hits: RefCell::new(vec![]),
                },
                charts::rail,
            );
            rail.keep_beating();
            let click = gtk::GestureClick::new();
            let (r, tx) = (rail.clone(), self.tx.clone());
            click.connect_released(move |_, _, x, y| {
                if let Some(uuid) = r.with(|rail| charts::rail_hit(rail, x, y)) {
                    open(&tx, &uuid);
                }
            });
            rail.area.add_controller(click);
            rail.area.set_cursor_from_name(Some("pointer"));
            let scroll = gtk::ScrolledWindow::builder()
                .vscrollbar_policy(gtk::PolicyType::Never)
                .hscrollbar_policy(gtk::PolicyType::Automatic)
                .child(&rail.area)
                .build();
            scroll.set_min_content_height(h);
            panel.append(&scroll);
            self.flow.insert(&panel, -1);
            rails.push(rail);
        }
        if !s.lines.is_empty() {
            let key = hbox(18);
            key.add_css_class("key");
            for (glyph, class, text) in [
                ("●", "accent", "finished"),
                ("◉", "bright", "next up"),
                ("●", "ok", "started"),
                ("○", "dim", "waiting"),
                ("■", "warn", "needs you"),
            ] {
                let item = hbox(6);
                item.append(&label(glyph, &[class]));
                item.append(&label(text, &["dim"]));
                key.append(&item);
            }
            self.list.append(&key);
        }
        *self.rails.borrow_mut() = rails;
    }

    pub fn play(&self) {
        for r in self.rails.borrow().iter() {
            r.play();
        }
    }
}
