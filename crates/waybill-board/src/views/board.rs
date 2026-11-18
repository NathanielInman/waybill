//! Board: every open task in the lane it stands in, plus what closed today.
//! Dragging a card to another lane does what the lane means.

use super::{Tx, open};
use crate::actions::Action;
use crate::charts::{self, Canvas};
use crate::shell::Msg;
use crate::theme::{self, Rgba};
use crate::ui::{self, chip, hbox, label, vbox};
use gtk::prelude::*;
use gtk::{gdk, glib};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use waybill_core::claim::HoldState;
use waybill_core::model::{Card, Lane, Snapshot};

struct Column {
    lane: Lane,
    count: gtk::Label,
    mix: Canvas<Vec<(f64, Rgba)>>,
    list: gtk::Box,
}

/// Where each card stands and whether it is the user's, for a drop to read.
type Standing = Rc<RefCell<HashMap<String, (Lane, bool)>>>;

pub struct BoardView {
    pub root: gtk::Box,
    columns: Vec<Column>,
    standing: Standing,
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

impl BoardView {
    pub fn new(tx: Tx) -> BoardView {
        let root = hbox(12);
        root.add_css_class("board");
        root.set_homogeneous(true);
        let standing: Standing = Rc::new(RefCell::new(HashMap::new()));
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

                let drop = gtk::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
                let (tx, standing, to) = (tx.clone(), standing.clone(), *lane);
                drop.connect_drop(move |_, value, _, _| {
                    let Ok(uuid) = value.get::<String>() else {
                        return false;
                    };
                    let Some((from, yours)) = standing.borrow().get(&uuid).copied() else {
                        return false;
                    };
                    let msg = match dropped(&uuid, from, yours, to) {
                        Ok(Some(action)) => Msg::Do(action),
                        Ok(None) => return false,
                        Err(why) => Msg::Did(Err(why.to_string())),
                    };
                    let _ = tx.send_blocking(msg);
                    true
                });
                col.add_controller(drop);
                root.append(&col);
                Column {
                    lane: *lane,
                    count,
                    mix,
                    list,
                }
            })
            .collect();
        BoardView {
            root,
            columns,
            standing,
            tx,
        }
    }

    pub fn set(&self, s: &Snapshot) {
        let pal = theme::current();
        *self.standing.borrow_mut() = s
            .cards
            .iter()
            .filter(|c| c.ended.is_none())
            .map(|c| (c.uuid.clone(), (c.lane, c.yours)))
            .collect();
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

/// One task as a card. The whole card is a button that opens it, and it can
/// be dragged to another lane.
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
    if doing {
        // Who has it, or that nobody does.
        let hold = hbox(6);
        let state = c.hold.as_ref().map(|h| h.state);
        let stale = state == Some(HoldState::Stale);
        hold.append(&label(
            "●",
            &[
                "lamp",
                if stale { "bad" } else { "ok" },
                if stale { "still" } else { "live" },
            ],
        ));
        let who = match (&c.hold, state) {
            (Some(h), Some(HoldState::Held)) => h.who.clone(),
            (Some(h), Some(HoldState::Stale)) if !h.who.is_empty() => format!("left by {}", h.who),
            (_, Some(HoldState::Stale)) => "left from before the last boot".to_string(),
            _ => "no claim".to_string(),
        };
        let l = label(
            &format!(
                "{} · {who}",
                match ui::age(s, c).as_str() {
                    "now" => "just started".to_string(),
                    age => format!("held {age}"),
                }
            ),
            &["num", "small", if stale { "bad" } else { "ok" }],
        );
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        hold.append(&l);
        body.append(&hold);
    }
    if !c.visits.is_empty() && !matches!(c.lane, Lane::Done | Lane::Past) {
        let foot = hbox(6);
        let n = c.visits.len();
        let last = c.visits.iter().map(|v| v.at).max().unwrap_or(c.entry);
        let ago = ui::ago(s.now - last);
        foot.append(&label(
            &format!(
                "{n} {} · last {ago}",
                if n == 1 { "visit" } else { "visits" }
            ),
            &["dim", "small"],
        ));
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
    if doing {
        b.add_css_class("doing");
    }
    let (tx, uuid) = (tx.clone(), c.uuid.clone());
    b.connect_clicked(move |_| open(&tx, &uuid));
    if c.ended.is_none() {
        let drag = gtk::DragSource::new();
        drag.set_actions(gdk::DragAction::MOVE);
        drag.set_content(Some(&gdk::ContentProvider::for_value(&c.uuid.to_value())));
        b.add_controller(drag);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
