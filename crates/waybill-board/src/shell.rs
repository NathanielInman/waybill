//! The window: a header with the lamp and tabs, the five views, the card
//! pane sliding in from the right, and a footer with what the view is for,
//! what just happened and the `/` line.

use crate::actions::{self, Action};
use crate::charts::{self, Canvas};
use crate::detail::Detail;
use crate::palette::{self, Command};
use crate::theme::{self, Rgba};
use crate::ui::{self, hbox, label, vbox};
use crate::views::board::BoardView;
use crate::views::desk::DeskView;
use crate::views::lines::LinesView;
use crate::views::log::LogView;
use crate::views::pulse::PulseView;
use crate::views::{Tab, Tx};
use gtk::prelude::*;
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::time::Duration;
use waybill_core::agent::Desk;
use waybill_core::clock;
use waybill_core::model::{Card, Snapshot};

pub enum Msg {
    Snapshot(Box<Snapshot>),
    /// Open the card with this uuid.
    Open(String),
    /// Show the open card again, dropping whatever was being typed.
    Reopen,
    Close,
    /// The bar's stylesheet or the config changed.
    Restyle,
    Do(Action),
    /// What an action came back with.
    Did(Result<String, String>),
    /// Open the new-task form with this title.
    New(String),
    /// Show or hide the drawer, at this anchor (the drawer only).
    Toggle(Option<bar_anchor::Anchor>),
}

/// Where the board's tasks live. `desk` is `None` for the demo list and for
/// a saved snapshot, which nothing can be written to.
pub struct World {
    pub desk: Option<Arc<Desk>>,
    pub demo: bool,
}

impl World {
    pub fn note(&self, card: &Card) -> Option<String> {
        if self.demo {
            return Some(waybill_core::demo::note(&card.title));
        }
        let dir = self.desk.as_ref()?.notes.as_ref()?;
        waybill_core::act::read_note(dir, &card.uuid)
    }

    /// The choices a decision card offers.
    pub fn options(&self, card: &Card) -> Vec<String> {
        if !card.decision {
            return vec![];
        }
        self.note(card)
            .as_deref()
            .map(waybill_core::note::options)
            .unwrap_or_default()
    }

    pub fn title_limit(&self) -> usize {
        self.desk.as_ref().map(|d| d.cfg.lint.title).unwrap_or(50)
    }
}

type Run = Rc<dyn Fn()>;

pub struct Shell {
    pub window: gtk::ApplicationWindow,
    page: gtk::Box,
    stack: gtk::Stack,
    tabs: Vec<gtk::Button>,
    board: BoardView,
    desk: DeskView,
    lines: LinesView,
    log: LogView,
    pulse: PulseView,
    detail: Rc<Detail>,
    reveal: gtk::Revealer,
    lamp: gtk::Label,
    counts: gtk::Label,
    today: gtk::Label,
    spark: charts::Spark,
    notice: gtk::Label,
    question: gtk::Label,
    toast: gtk::Label,
    toast_timer: Rc<RefCell<Option<glib::SourceId>>>,
    line: gtk::Entry,
    found: gtk::Popover,
    found_list: gtk::Box,
    found_runs: RefCell<Vec<Run>>,
    snap: RefCell<Rc<Snapshot>>,
    tab: Cell<Tab>,
    stale: RefCell<[bool; 5]>,
    opened: RefCell<Option<String>>,
    world: Rc<World>,
    tx: Tx,
    /// Asks the loader to reread the list now.
    wake: mpsc::Sender<()>,
}

fn keyed(text: &str, key: &str) -> gtk::Box {
    let inner = hbox(7);
    inner.append(&label(text, &[]));
    inner.append(&label(key, &["key"]));
    inner
}

impl Shell {
    pub fn new(
        app: &gtk::Application,
        tx: Tx,
        world: World,
        wake: mpsc::Sender<()>,
        size: (i32, i32),
    ) -> Rc<Shell> {
        let world = Rc::new(world);
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Waybill")
            .default_width(size.0)
            .default_height(size.1)
            .build();
        window.add_css_class("waybill");

        let page = vbox(0);
        page.add_css_class("page");

        // Header: who we are and how things stand, the tabs, today.
        let header = gtk::CenterBox::new();
        header.add_css_class("header");
        let brand = hbox(10);
        let lamp = label("●", &["lamp", "ok", "brand-lamp"]);
        brand.append(&lamp);
        brand.append(&label("WAYBILL", &["brand"]));
        let counts = label("", &["dim"]);
        brand.append(&counts);
        header.set_start_widget(Some(&brand));

        let tab_box = hbox(2);
        tab_box.add_css_class("tabs");
        let mut tabs = Vec::new();
        for (i, tab) in Tab::ALL.into_iter().enumerate() {
            let b = gtk::Button::builder()
                .child(&keyed(tab.name(), &(i + 1).to_string()))
                .build();
            b.set_tooltip_text(Some(tab.question()));
            tab_box.append(&b);
            tabs.push(b);
        }
        header.set_center_widget(Some(&tab_box));

        let right = hbox(10);
        let spark = Canvas::new(
            84,
            22,
            (vec![], theme::current().ok),
            |f, d: &(Vec<f32>, Rgba)| charts::spark(f, &d.0, d.1),
        );
        spark.area.set_valign(gtk::Align::Center);
        spark
            .area
            .set_tooltip_text(Some("Finished per day, two weeks"));
        right.append(&spark.area);
        let today = label("", &["dim", "num"]);
        right.append(&today);
        let new = gtk::Button::builder().child(&keyed("New", "n")).build();
        new.add_css_class("act");
        let t = tx.clone();
        new.connect_clicked(move |_| {
            let _ = t.send_blocking(Msg::New(String::new()));
        });
        right.append(&new);
        header.set_end_widget(Some(&right));
        page.append(&header);

        let notice = ui::wrapped("", &["notice"], 4);
        notice.set_visible(false);
        page.append(&notice);

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_transition_duration(160);
        stack.set_hexpand(true);
        stack.set_vexpand(true);
        let board = BoardView::new(tx.clone());
        let desk = DeskView::new(tx.clone(), world.clone());
        let lines = LinesView::new(tx.clone());
        let log = LogView::new(tx.clone());
        let pulse = PulseView::new();
        stack.add_named(&board.root, Some(Tab::Board.id()));
        stack.add_named(&desk.root, Some(Tab::Desk.id()));
        stack.add_named(&lines.root, Some(Tab::Lines.id()));
        stack.add_named(&log.root, Some(Tab::Log.id()));
        stack.add_named(&pulse.root, Some(Tab::Pulse.id()));

        // The card pane slides over the right edge of the view, so opening
        // a card never reflows the board under it.
        let detail = Detail::new(tx.clone(), world.clone());
        let reveal = gtk::Revealer::new();
        reveal.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
        // A screenshot is one frame: the pane must already be there.
        reveal.set_transition_duration(if charts::animated() { 220 } else { 0 });
        reveal.set_child(Some(&detail.root));
        reveal.set_halign(gtk::Align::End);
        let middle = gtk::Overlay::new();
        middle.add_css_class("middle");
        middle.set_child(Some(&stack));
        middle.add_overlay(&reveal);
        page.append(&middle);

        let footer = hbox(14);
        footer.add_css_class("footer");
        let question = label("", &["question"]);
        footer.append(&question);
        let toast = label("", &["toast"]);
        toast.set_ellipsize(gtk::pango::EllipsizeMode::End);
        toast.set_hexpand(true);
        toast.set_xalign(1.0);
        footer.append(&toast);
        let line = gtk::Entry::new();
        line.add_css_class("prompt");
        line.set_width_chars(34);
        line.set_placeholder_text(Some("/  find a card, or say what to do"));
        footer.append(&line);
        for (key, what) in [("1 to 5", "views"), ("Esc", "close")] {
            let hint = hbox(6);
            hint.append(&label(key, &["key"]));
            hint.append(&label(what, &["dim"]));
            footer.append(&hint);
        }
        page.append(&footer);
        window.set_child(Some(&page));

        let found = gtk::Popover::new();
        found.set_parent(&line);
        found.set_autohide(false);
        found.set_has_arrow(false);
        found.set_position(gtk::PositionType::Top);
        found.set_can_focus(false);
        found.add_css_class("found");
        let found_list = vbox(2);
        found.set_child(Some(&found_list));

        let shell = Rc::new(Shell {
            window,
            page,
            stack,
            tabs,
            board,
            desk,
            lines,
            log,
            pulse,
            detail,
            reveal,
            lamp,
            counts,
            today,
            spark,
            notice,
            question,
            toast,
            toast_timer: Rc::new(RefCell::new(None)),
            line,
            found,
            found_list,
            found_runs: RefCell::new(vec![]),
            snap: RefCell::new(Rc::new(Snapshot::default())),
            tab: Cell::new(Tab::Board),
            stale: RefCell::new([true; 5]),
            opened: RefCell::new(None),
            world,
            tx,
            wake,
        });

        for (tab, button) in Tab::ALL.into_iter().zip(&shell.tabs) {
            let weak = Rc::downgrade(&shell);
            button.connect_clicked(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.set_tab(tab);
                }
            });
        }
        shell.connect_line();
        shell.connect_keys();
        shell.set_tab(Tab::Board);
        shell
    }

    fn typing(&self) -> bool {
        gtk::prelude::RootExt::focus(&self.window)
            .is_some_and(|w| w.is::<gtk::Text>() || w.is::<gtk::TextView>())
    }

    fn connect_keys(self: &Rc<Self>) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(s) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if state.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK)
                || s.typing()
            {
                return glib::Propagation::Proceed;
            }
            if key == gdk::Key::Escape && s.opened.borrow().is_some() {
                s.close();
                return glib::Propagation::Stop;
            }
            let Some(ch) = key.to_unicode() else {
                return glib::Propagation::Proceed;
            };
            if ch == '/' {
                s.line.grab_focus();
                return glib::Propagation::Stop;
            }
            if s.opened.borrow().is_some() && s.detail.key(ch) {
                return glib::Propagation::Stop;
            }
            if ch == 'n' {
                s.new_task("");
                return glib::Propagation::Stop;
            }
            match ch
                .to_digit(10)
                .and_then(|d| Tab::ALL.get((d as usize).wrapping_sub(1)))
            {
                Some(tab) => {
                    s.set_tab(*tab);
                    glib::Propagation::Stop
                }
                None => glib::Propagation::Proceed,
            }
        });
        self.window.add_controller(keys);
    }

    /// The `/` line: results as you type, Enter takes the first.
    fn connect_line(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.line.connect_changed(move |_| {
            if let Some(s) = weak.upgrade() {
                s.find();
            }
        });
        let weak = Rc::downgrade(self);
        self.line.connect_activate(move |_| {
            let Some(s) = weak.upgrade() else { return };
            let first = s.found_runs.borrow().first().cloned();
            if let Some(run) = first {
                run();
            }
            s.leave_line();
        });
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(s) = weak.upgrade()
            {
                s.leave_line();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.line.add_controller(keys);
    }

    fn leave_line(&self) {
        self.line.set_text("");
        self.found.popdown();
        self.page.grab_focus();
    }

    fn find(self: &Rc<Self>) {
        let text = self.line.text().to_string();
        let text = text.trim().trim_start_matches('/').trim();
        ui::clear(&self.found_list);
        let mut runs: Vec<Run> = Vec::new();
        let mut add = |shown: gtk::Box, run: Run| {
            let b = ui::row_button(&shown, &["row", "slim"]);
            b.set_can_focus(false);
            let r = run.clone();
            b.connect_clicked(move |_| r());
            self.found_list.append(&b);
            runs.push(run);
        };
        if !text.is_empty() {
            let opened = self.opened.borrow().clone();
            if let Some((command, say)) = palette::parse(text, opened.as_deref()) {
                let row = hbox(8);
                row.append(&label("→", &["accent"]));
                row.append(&label(&say, &["bright"]));
                let weak = Rc::downgrade(self);
                add(
                    row,
                    Rc::new(move || {
                        let Some(s) = weak.upgrade() else { return };
                        match command.clone() {
                            Command::Do(a) => s.act(a),
                            Command::View(t) => s.set_tab(t),
                            Command::New(title) => s.new_task(&title),
                        }
                        s.leave_line();
                    }),
                );
            }
            let snap = self.snap.borrow().clone();
            let mut hits: Vec<(usize, &Card)> = snap
                .cards
                .iter()
                .filter(|c| c.ended.is_none())
                .filter_map(|c| {
                    palette::score(text, &c.title, &c.project, &c.tags, &c.uuid).map(|s| (s, c))
                })
                .collect();
            hits.sort_by_key(|(s, c)| (*s, c.title.len()));
            for (_, c) in hits.into_iter().take(7) {
                let row = hbox(8);
                row.append(&ui::project(&c.project));
                let t = label(&c.title, &[]);
                t.set_ellipsize(gtk::pango::EllipsizeMode::End);
                t.set_max_width_chars(52);
                row.append(&t);
                let (weak, uuid) = (Rc::downgrade(self), c.uuid.clone());
                add(
                    row,
                    Rc::new(move || {
                        if let Some(s) = weak.upgrade() {
                            s.open(&uuid);
                            s.leave_line();
                        }
                    }),
                );
            }
        }
        let any = !runs.is_empty();
        *self.found_runs.borrow_mut() = runs;
        if any {
            self.found.popup();
        } else {
            self.found.popdown();
        }
    }

    pub fn handle(self: &Rc<Self>, msg: Msg) {
        match msg {
            Msg::Snapshot(s) => {
                *self.snap.borrow_mut() = Rc::new(*s);
                self.refresh();
            }
            Msg::Open(uuid) => self.open(&uuid),
            Msg::Reopen => {
                let opened = self.opened.borrow().clone();
                match opened {
                    Some(uuid) if !uuid.is_empty() => self.open(&uuid),
                    _ => self.close(),
                }
            }
            Msg::Close => self.close(),
            Msg::Restyle => {
                crate::app::apply_styles();
                self.refresh();
            }
            Msg::Do(action) => self.act(action),
            Msg::Did(result) => {
                match result {
                    Ok(said) => self.say(&said, false),
                    Err(e) => self.say(&e, true),
                }
                self.detail.settle();
                let _ = self.wake.send(());
            }
            Msg::New(title) => self.new_task(&title),
            Msg::Toggle(_) => {}
        }
    }

    /// Runs an action on a worker thread; the answer comes back as `Did`.
    pub fn act(&self, action: Action) {
        let Some(desk) = self.world.desk.clone() else {
            self.say("This list is read-only here; nothing was written.", true);
            return;
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result =
                actions::perform(&desk, action, clock::now()).map_err(|e| format!("{e:#}"));
            let _ = tx.send_blocking(Msg::Did(result));
        });
    }

    /// One line in the footer for a few seconds.
    fn say(&self, text: &str, bad: bool) {
        self.toast.set_label(text);
        if bad {
            self.toast.add_css_class("bad");
        } else {
            self.toast.remove_css_class("bad");
        }
        if let Some(id) = self.toast_timer.borrow_mut().take() {
            id.remove();
        }
        let (toast, slot) = (self.toast.clone(), self.toast_timer.clone());
        let id =
            glib::timeout_add_local_once(Duration::from_secs(if bad { 9 } else { 5 }), move || {
                // A timer that has fired must not be removed again.
                slot.borrow_mut().take();
                toast.set_label("");
            });
        *self.toast_timer.borrow_mut() = Some(id);
    }

    /// Redraws everything that shows the snapshot.
    fn refresh(self: &Rc<Self>) {
        *self.stale.borrow_mut() = [true; 5];
        self.render_header();
        self.render(self.tab.get());
        let opened = self.opened.borrow().clone();
        // Not while the user is typing in the pane.
        if let Some(uuid) = opened.filter(|u| !u.is_empty() && !self.detail.busy()) {
            self.open(&uuid);
        }
    }

    fn render_header(&self) {
        let s = self.snap.borrow().clone();
        let p = &s.pulse;
        let lanes = p.lanes;
        self.counts.set_label(&format!(
            "{} ready · {} doing · {} need you",
            lanes.ready, lanes.doing, lanes.yours
        ));
        for c in ["ok", "warn", "bad"] {
            self.lamp.remove_css_class(c);
        }
        let lamp = waybill_core::lamp::lamp(&s);
        self.lamp.add_css_class(match lamp.state {
            waybill_core::lamp::State::Calm => "ok",
            waybill_core::lamp::State::NeedsYou => "warn",
            waybill_core::lamp::State::Error => "bad",
        });
        self.lamp.set_tooltip_text(Some(&lamp.why));
        self.today.set_label(&format!(
            "{} finished today · {}",
            lanes.done,
            ui::hm(&s, s.now)
        ));
        let n = p.days.len();
        self.spark.set((
            p.days
                .iter()
                .skip(n.saturating_sub(14))
                .map(|d| d.done as f32)
                .collect(),
            theme::current().ok,
        ));
        match &s.error {
            Some(e) => {
                self.notice
                    .set_label(&format!("Waybill could not read the task list: {e}"));
                self.notice.set_visible(true);
            }
            None => self.notice.set_visible(false),
        }
    }

    fn render(&self, tab: Tab) {
        if !std::mem::replace(&mut self.stale.borrow_mut()[tab.index()], false) {
            return;
        }
        let s = self.snap.borrow().clone();
        match tab {
            Tab::Board => self.board.set(&s),
            Tab::Desk => self.desk.set(&s),
            Tab::Lines => self.lines.set(&s),
            Tab::Log => self.log.set(&s),
            Tab::Pulse => self.pulse.set(&s),
        }
    }

    pub fn set_tab(&self, tab: Tab) {
        let changed = self.tab.replace(tab) != tab;
        for (t, b) in Tab::ALL.into_iter().zip(&self.tabs) {
            if t == tab {
                b.add_css_class("active");
            } else {
                b.remove_css_class("active");
            }
        }
        self.question.set_markup(&format!(
            "<b>{}.</b> {}",
            tab.name(),
            ui::esc(tab.question())
        ));
        self.render(tab);
        self.stack.set_visible_child_name(tab.id());
        if changed {
            match tab {
                Tab::Board => self.board.play(),
                Tab::Desk => self.desk.play(),
                Tab::Lines => self.lines.play(),
                Tab::Log => self.log.play(),
                Tab::Pulse => self.pulse.play(),
            }
        }
    }

    fn open(self: &Rc<Self>, uuid: &str) {
        let s = self.snap.borrow().clone();
        let Some(card) = s.card(uuid) else {
            // The card left the snapshot: finished long ago, or deleted.
            self.close();
            return;
        };
        self.detail.show(&s, card);
        *self.opened.borrow_mut() = Some(uuid.to_string());
        self.reveal.set_reveal_child(true);
    }

    fn new_task(self: &Rc<Self>, title: &str) {
        let s = self.snap.borrow().clone();
        self.detail.show_new(&s, title, self.world.title_limit());
        // An empty uuid: the pane is open, on no card.
        *self.opened.borrow_mut() = Some(String::new());
        self.reveal.set_reveal_child(true);
    }

    /// Opens the first open card whose title contains `text`, else the
    /// first finished one (for screenshots).
    pub fn open_titled(self: &Rc<Self>, text: &str) {
        let s = self.snap.borrow().clone();
        let hit = |open: bool| {
            s.cards
                .iter()
                .find(|c| c.title.contains(text) && c.ended.is_none() == open)
        };
        if let Some(c) = hit(true).or_else(|| hit(false)) {
            self.open(&c.uuid);
        }
    }

    /// Opens the card with this uuid, or the start of one.
    pub fn open_card(self: &Rc<Self>, id: &str) {
        let s = self.snap.borrow().clone();
        if let Some(c) = s.cards.iter().find(|c| c.uuid.starts_with(id)) {
            self.open(&c.uuid);
        }
    }

    /// Runs a `/` line as if it had been typed (for checks without a pointer).
    pub fn type_line(self: &Rc<Self>, text: &str) {
        self.line.set_text(text);
        let first = self.found_runs.borrow().first().cloned();
        if let Some(run) = first {
            run();
        }
    }

    fn close(&self) {
        self.opened.borrow_mut().take();
        self.detail.settle();
        self.reveal.set_reveal_child(false);
        self.page.grab_focus();
    }

    /// Draws the window into a PNG at `size`, then quits. The size is forced
    /// on the page because a headless display may be smaller than the window.
    pub fn shoot(self: &Rc<Self>, out: String, size: (i32, i32), app: gtk::Application) {
        self.page.set_size_request(size.0, size.1);
        let shell = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(1400), move || {
            let tries = Rc::new(Cell::new(0));
            glib::timeout_add_local(Duration::from_millis(100), move || {
                let widget: gtk::Widget = shell.page.clone().upcast();
                let (wd, ht) = (widget.width(), widget.height());
                let paintable = gtk::WidgetPaintable::new(Some(&widget));
                let snapshot = gtk::Snapshot::new();
                paintable.snapshot(&snapshot, wd as f64, ht as f64);
                tries.set(tries.get() + 1);
                match (
                    snapshot.to_node(),
                    widget.native().and_then(|n| n.renderer()),
                ) {
                    (Some(node), Some(renderer)) => {
                        let tex = renderer.render_texture(&node, None);
                        match tex.save_to_png(&out) {
                            Ok(()) => println!("{out} {wd}x{ht}"),
                            Err(e) => eprintln!("waybill-board: {e}"),
                        }
                    }
                    _ if tries.get() < 50 => return glib::ControlFlow::Continue,
                    _ => eprintln!("waybill-board: nothing to draw ({wd}x{ht})"),
                }
                app.quit();
                glib::ControlFlow::Break
            });
        });
    }
}
