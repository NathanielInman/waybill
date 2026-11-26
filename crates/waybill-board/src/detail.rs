//! The card pane: one task in full, and everything you can do to it. Its
//! note laid out beside its visits, what it waits on and what it frees, and
//! a row of buttons that each print their key.

use crate::shell::{Msg, World};
use crate::ui::{self, chip, esc, hbox, label, vbox};
use crate::views::Tx;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use waybill_core::act::NewTask;
use waybill_core::claim::HoldState;
use waybill_core::model::{Card, Lane, Snapshot, is_ruling};
use waybill_core::note::{self, Block};
use waybill_core::say::{self, Action, Offer};

type Run = Rc<dyn Fn()>;

pub struct Detail {
    pub root: gtk::Box,
    body: gtk::Box,
    tx: Tx,
    world: Rc<World>,
    /// The keys that work on the card being shown.
    keys: RefCell<Vec<(char, Run)>>,
    /// Set while the user is typing in the pane, so a refresh of the list
    /// does not rebuild it under their hands.
    busy: Cell<bool>,
}

impl Detail {
    pub fn new(tx: Tx, world: Rc<World>) -> Rc<Detail> {
        let root = vbox(0);
        root.add_css_class("detail");
        root.set_size_request(480, -1);
        let body = vbox(14);
        root.append(&ui::scrolled(&body));
        Rc::new(Detail {
            root,
            body,
            tx,
            world,
            keys: RefCell::new(vec![]),
            busy: Cell::new(false),
        })
    }

    pub fn busy(&self) -> bool {
        self.busy.get()
    }

    /// Lets the next refresh rebuild the pane again.
    pub fn settle(&self) {
        self.busy.set(false);
    }

    /// Runs the action bound to `key`, if the card has one.
    pub fn key(&self, key: char) -> bool {
        let run = self
            .keys
            .borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, r)| r.clone());
        match run {
            Some(run) => {
                run();
                true
            }
            None => false,
        }
    }

    fn send(&self, msg: Msg) {
        let _ = self.tx.send_blocking(msg);
    }

    /// A button that prints its key, and the key itself.
    fn button(&self, text: &str, key: Option<char>, class: &str, run: Run) -> gtk::Button {
        let inner = hbox(7);
        inner.append(&label(text, &[]));
        if let Some(k) = key {
            inner.append(&label(&k.to_string(), &["key"]));
            self.keys.borrow_mut().push((k, run.clone()));
        }
        let b = gtk::Button::builder().child(&inner).build();
        b.add_css_class("act");
        if !class.is_empty() {
            b.add_css_class(class);
        }
        b.connect_clicked(move |_| run());
        b
    }

    /// A button for one of the card's offers. One that asks for a line
    /// opens a prompt in `slot` first.
    fn offer(self: &Rc<Self>, offer: &Offer, uuid: &str, slot: &gtk::Box) -> gtk::Button {
        let run: Run = match &offer.ask {
            None => match offer.action(uuid, "") {
                Some(action) => self.doing(action),
                None => Rc::new(|| {}),
            },
            Some(hint) => {
                let (me, sl, hint, o, u) = (
                    Rc::downgrade(self),
                    slot.clone(),
                    hint.clone(),
                    offer.clone(),
                    uuid.to_string(),
                );
                Rc::new(move || {
                    if let Some(d) = me.upgrade() {
                        let (o, u) = (o.clone(), u.clone());
                        d.prompt(&sl, &hint, move |t| o.action(&u, &t));
                    }
                })
            }
        };
        self.button(&offer.label, offer.key, offer.style, run)
    }

    fn doing(self: &Rc<Self>, action: Action) -> Run {
        let me = Rc::downgrade(self);
        Rc::new(move || {
            if let Some(d) = me.upgrade() {
                d.send(Msg::Do(action.clone()));
            }
        })
    }

    fn top(&self, left: &[gtk::Label]) {
        let top = hbox(6);
        for l in left {
            top.append(l);
        }
        top.append(&ui::spring());
        let close = gtk::Button::builder().child(&{
            let inner = hbox(7);
            inner.append(&label("Close", &[]));
            inner.append(&label("Esc", &["key"]));
            inner
        });
        let close = close.build();
        close.add_css_class("act");
        let tx = self.tx.clone();
        close.connect_clicked(move |_| {
            let _ = tx.send_blocking(Msg::Close);
        });
        top.append(&close);
        self.body.append(&top);
    }

    /// A line to type into, shown under the buttons. Enter submits, Esc
    /// puts it away.
    fn prompt(
        self: &Rc<Self>,
        slot: &gtk::Box,
        hint: &str,
        submit: impl Fn(String) -> Option<Action> + 'static,
    ) {
        ui::clear(slot);
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some(hint));
        entry.set_hexpand(true);
        entry.add_css_class("prompt");
        let line = hbox(8);
        line.append(&entry);
        line.append(&label("Enter", &["key"]));
        slot.append(&line);
        self.busy.set(true);
        let (me, s) = (Rc::downgrade(self), slot.clone());
        entry.connect_activate(move |e| {
            let Some(d) = me.upgrade() else { return };
            if let Some(action) = submit(e.text().to_string()) {
                d.send(Msg::Do(action));
            }
            ui::clear(&s);
            d.busy.set(false);
        });
        let keys = gtk::EventControllerKey::new();
        let (me, s) = (Rc::downgrade(self), slot.clone());
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                ui::clear(&s);
                if let Some(d) = me.upgrade() {
                    d.busy.set(false);
                    d.root.grab_focus();
                }
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        entry.add_controller(keys);
        entry.grab_focus();
    }

    pub fn show(self: &Rc<Self>, s: &Snapshot, c: &Card) {
        ui::clear(&self.body);
        self.keys.borrow_mut().clear();
        self.busy.set(false);
        let note_text = self.world.note(c);

        let mut chips = vec![ui::project(&c.project)];
        chips.extend(ui::priority(c.priority));
        if c.decision {
            chips.push(chip("decision", "violet"));
        }
        chips.extend(c.tags.iter().map(|t| chip(t, ui::tag_class(t))));
        self.top(&chips);

        let title = ui::wrapped(&c.title, &["detail-title"], 6);
        title.set_selectable(true);
        self.body.append(&title);

        let open = c.ended.is_none();
        let uuid = c.uuid.clone();
        let slot = vbox(0);
        if open {
            let options = if c.decision {
                note_text.as_deref().map(note::options).unwrap_or_default()
            } else {
                Vec::new()
            };
            let mut row: Option<(&str, gtk::Box)> = None;
            for offer in say::offers(c, &options) {
                let b = self.offer(&offer, &uuid, &slot);
                if offer.on {
                    b.add_css_class("on");
                }
                let decide = offer.group == "decide";
                if decide {
                    b.set_halign(gtk::Align::Start);
                }
                match &row {
                    Some((group, r)) if *group == offer.group => r.append(&b),
                    _ => {
                        if decide {
                            self.body.append(&label("Decide", &["section"]));
                        }
                        let r = if decide { vbox(5) } else { hbox(6) };
                        r.append(&b);
                        self.body.append(&r);
                        row = Some((offer.group, r));
                    }
                }
            }
            self.body.append(&slot);
        }

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
                "started",
                label(
                    &format!("{} (at {})", ui::ago(s.now - start), ui::when(s, start)),
                    &["num"],
                )
                .upcast(),
            );
        }
        if let Some(hold) = c.hold.as_ref().filter(|_| c.ended.is_none()) {
            let (text, class) = match hold.state {
                HoldState::Held => (hold.who.clone(), "ok"),
                HoldState::Unclaimed => (
                    "nobody on record: started without a claim".to_string(),
                    "dim",
                ),
                HoldState::Stale if hold.who.is_empty() => (
                    "nobody: it was started before the last boot".to_string(),
                    "bad",
                ),
                HoldState::Stale => (format!("nobody: {} is gone", hold.who), "bad"),
            };
            fact("held by", ui::wrapped(&text, &[class], 2).upcast());
        }
        if let Some(end) = c.ended {
            fact("finished", label(&ui::when(s, end), &["num"]).upcast());
        }
        fact("added", label(&ui::ago(s.now - c.entry), &["num"]).upcast());
        let short = label(c.short(), &["num", "mono"]);
        short.set_selectable(true);
        short.set_tooltip_text(Some(&c.uuid));
        fact("uuid", short.upcast());
        if let Some(r) = &c.reference {
            fact("marker", chip(r, "sha").upcast());
        }
        self.body.append(&facts);

        self.related(s, "Frees when it closes", &c.frees);
        self.related(s, "Waits on", &c.waits_on);

        let head = hbox(8);
        head.append(&label("Note", &["section"]));
        head.append(&ui::spring());
        let note_box = vbox(0);
        if open {
            let (me, nb, u, text) = (
                Rc::downgrade(self),
                note_box.clone(),
                uuid.clone(),
                note_text.clone(),
            );
            let title = c.title.clone();
            head.append(&self.button(
                if note_text.is_some() {
                    "Edit"
                } else {
                    "Write one"
                },
                Some('e'),
                "",
                Rc::new(move || {
                    if let Some(d) = me.upgrade() {
                        let start = text.clone().unwrap_or_else(|| format!("# {title}\n\n"));
                        d.edit_note(&nb, &u, &start);
                    }
                }),
            ));
        }
        self.body.append(&head);
        match &note_text {
            Some(text) => note_box.append(&render_note(text)),
            None => note_box.append(&ui::wrapped(
                "No note. The card title carries everything.",
                &["empty"],
                2,
            )),
        }
        self.body.append(&note_box);

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

    /// Swaps the note for an editor.
    fn edit_note(self: &Rc<Self>, slot: &gtk::Box, uuid: &str, text: &str) {
        ui::clear(slot);
        self.busy.set(true);
        let view = gtk::TextView::new();
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.set_monospace(true);
        view.add_css_class("editor");
        view.buffer().set_text(text);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(260)
            .child(&view)
            .build();
        slot.append(&scroll);
        let row = hbox(6);
        row.set_margin_top(8);
        let (me, u, v) = (Rc::downgrade(self), uuid.to_string(), view.clone());
        let save = gtk::Button::with_label("Save");
        save.add_css_class("act");
        save.add_css_class("go");
        save.connect_clicked(move |_| {
            if let Some(d) = me.upgrade() {
                let b = v.buffer();
                let text = b.text(&b.start_iter(), &b.end_iter(), false).to_string();
                d.busy.set(false);
                d.send(Msg::Do(Action::SaveNote(u.clone(), text)));
            }
        });
        let me = Rc::downgrade(self);
        let cancel = gtk::Button::with_label("Cancel");
        cancel.add_css_class("act");
        cancel.connect_clicked(move |_| {
            if let Some(d) = me.upgrade() {
                d.busy.set(false);
                d.send(Msg::Reopen);
            }
        });
        row.append(&save);
        row.append(&cancel);
        slot.append(&row);
        // Esc leaves the editor the way Cancel does, without saving.
        let c = cancel.clone();
        on_escape(&view, move || c.emit_clicked());
        view.grab_focus();
    }

    /// The pane as a form for a new task.
    pub fn show_new(self: &Rc<Self>, s: &Snapshot, title: &str, limit: usize) {
        ui::clear(&self.body);
        self.keys.borrow_mut().clear();
        self.busy.set(true);
        self.top(&[label("New task", &["section"])]);

        let entry = gtk::Entry::new();
        entry.set_text(title);
        entry.set_placeholder_text(Some("Imperative and short: Wire the victory screen"));
        entry.add_css_class("prompt");
        let count = label("", &["dim", "small", "num"]);
        let update = {
            let (entry, count) = (entry.clone(), count.clone());
            move || {
                let n = entry.text().chars().count();
                count.set_label(&format!("{n} of {limit} characters"));
                if n > limit {
                    count.add_css_class("bad");
                } else {
                    count.remove_css_class("bad");
                }
            }
        };
        update();
        entry.connect_changed(move |_| update());
        self.body.append(&label("Title", &["dim"]));
        self.body.append(&entry);
        self.body.append(&count);

        let mut names: Vec<String> = s.pulse.projects.iter().map(|p| p.name.clone()).collect();
        names.sort();
        let project = gtk::Entry::new();
        project.add_css_class("prompt");
        project.set_placeholder_text(Some("project"));
        if let Some(first) = s.pulse.projects.first() {
            project.set_text(&first.name);
        }
        let hint = ui::wrapped(&names.join("  "), &["dim", "small"], 3);
        self.body.append(&label("Project", &["dim"]));
        self.body.append(&project);
        self.body.append(&hint);

        let row = hbox(6);
        let priority = Rc::new(Cell::new('M'));
        let buttons: Vec<gtk::ToggleButton> = ['H', 'M', 'L']
            .iter()
            .map(|p| {
                let b = gtk::ToggleButton::with_label(&p.to_string());
                b.add_css_class("act");
                b.set_active(*p == 'M');
                row.append(&b);
                b
            })
            .collect();
        for (i, b) in buttons.iter().enumerate() {
            if i > 0 {
                b.set_group(Some(&buttons[0]));
            }
            let (priority, p) = (priority.clone(), ['H', 'M', 'L'][i]);
            b.connect_toggled(move |b| {
                if b.is_active() {
                    priority.set(p);
                }
            });
        }
        let mine = gtk::CheckButton::with_label("It needs my hands");
        mine.set_margin_start(10);
        row.append(&mine);
        self.body.append(&label("Priority", &["dim"]));
        self.body.append(&row);

        let note = gtk::TextView::new();
        note.set_wrap_mode(gtk::WrapMode::WordChar);
        note.set_monospace(true);
        note.add_css_class("editor");
        // Esc in any field closes the form, the way it closes a card.
        for w in [
            entry.upcast_ref::<gtk::Widget>(),
            project.upcast_ref(),
            note.upcast_ref(),
        ] {
            let me = Rc::downgrade(self);
            on_escape(w, move || {
                if let Some(d) = me.upgrade() {
                    d.busy.set(false);
                    d.send(Msg::Close);
                }
            });
        }
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(170)
            .child(&note)
            .build();
        self.body.append(&label("Note (optional)", &["dim"]));
        self.body.append(&scroll);

        let add = gtk::Button::with_label("Add the task");
        add.add_css_class("act");
        add.add_css_class("go");
        add.set_halign(gtk::Align::Start);
        let submit: Rc<dyn Fn()> = {
            let (me, entry, project, mine, note) = (
                Rc::downgrade(self),
                entry.clone(),
                project.clone(),
                mine.clone(),
                note.clone(),
            );
            Rc::new(move || {
                let Some(d) = me.upgrade() else { return };
                if entry.text().trim().is_empty() {
                    return;
                }
                let b = note.buffer();
                d.busy.set(false);
                d.send(Msg::Do(Action::Add {
                    new: NewTask {
                        title: entry.text().to_string(),
                        project: Some(project.text().trim().to_string()).filter(|p| !p.is_empty()),
                        priority: Some(priority.get()),
                        ..Default::default()
                    },
                    note: b.text(&b.start_iter(), &b.end_iter(), false).to_string(),
                    mine: mine.is_active(),
                }));
                d.send(Msg::Close);
            })
        };
        let run = submit.clone();
        add.connect_clicked(move |_| run());
        entry.connect_activate(move |_| submit());
        self.body.append(&add);
        entry.grab_focus();
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

/// Runs `f` when Esc is pressed in `w` (a field), and stops the key there.
fn on_escape(w: &impl IsA<gtk::Widget>, f: impl Fn() + 'static) {
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            f();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    w.add_controller(keys);
}
