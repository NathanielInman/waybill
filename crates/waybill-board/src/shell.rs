//! The window: a header with the lamp and tabs, the five views, the card
//! pane sliding in from the right, and a footer that says what the current
//! view is for.

use crate::charts::{self, Canvas};
use crate::detail::Detail;
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
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use waybill_core::model::Snapshot;

pub enum Msg {
    Snapshot(Box<Snapshot>),
    /// Open the card with this uuid.
    Open(String),
    Close,
    /// The bar's stylesheet or the config changed.
    Restyle,
}

/// Where a card's note comes from.
pub enum Notes {
    Demo,
    Dir(Option<PathBuf>),
}

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
    detail: Detail,
    reveal: gtk::Revealer,
    lamp: gtk::Label,
    counts: gtk::Label,
    today: gtk::Label,
    spark: charts::Spark,
    notice: gtk::Label,
    question: gtk::Label,
    snap: RefCell<Rc<Snapshot>>,
    tab: Cell<Tab>,
    stale: RefCell<[bool; 5]>,
    opened: RefCell<Option<String>>,
    notes: Notes,
}

impl Shell {
    pub fn new(app: &gtk::Application, tx: Tx, notes: Notes, size: (i32, i32)) -> Rc<Shell> {
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
            let inner = hbox(8);
            inner.append(&label(tab.name(), &[]));
            inner.append(&label(&(i + 1).to_string(), &["key"]));
            let b = gtk::Button::builder().child(&inner).build();
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
        let desk = DeskView::new(tx.clone());
        let lines = LinesView::new(tx.clone());
        let log = LogView::new(tx.clone());
        let pulse = PulseView::new();
        stack.add_named(&board.root, Some(Tab::Board.id()));
        stack.add_named(&desk.root, Some(Tab::Desk.id()));
        stack.add_named(&lines.root, Some(Tab::Lines.id()));
        stack.add_named(&log.root, Some(Tab::Log.id()));
        stack.add_named(&pulse.root, Some(Tab::Pulse.id()));

        let detail = Detail::new(tx.clone());
        // The card pane slides over the right edge of the view, so opening
        // a card never reflows the board under it.
        let reveal = gtk::Revealer::new();
        reveal.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
        reveal.set_transition_duration(220);
        reveal.set_child(Some(&detail.root));
        reveal.set_halign(gtk::Align::End);
        let middle = gtk::Overlay::new();
        middle.add_css_class("middle");
        middle.set_child(Some(&stack));
        middle.add_overlay(&reveal);
        page.append(&middle);

        let footer = hbox(16);
        footer.add_css_class("footer");
        let question = label("", &["question"]);
        footer.append(&question);
        footer.append(&ui::spring());
        for (key, what) in [
            ("1 to 5", "views"),
            ("click", "open a card"),
            ("Esc", "close it"),
        ] {
            let hint = hbox(6);
            hint.append(&label(key, &["key"]));
            hint.append(&label(what, &["dim"]));
            footer.append(&hint);
        }
        page.append(&footer);
        window.set_child(Some(&page));

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
            snap: RefCell::new(Rc::new(Snapshot::default())),
            tab: Cell::new(Tab::Board),
            stale: RefCell::new([true; 5]),
            opened: RefCell::new(None),
            notes,
        });

        for (tab, button) in Tab::ALL.into_iter().zip(&shell.tabs) {
            let weak = Rc::downgrade(&shell);
            button.connect_clicked(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.set_tab(tab);
                }
            });
        }
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&shell);
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(s) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if state.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK) {
                return glib::Propagation::Proceed;
            }
            if key == gdk::Key::Escape && s.opened.borrow().is_some() {
                s.close();
                return glib::Propagation::Stop;
            }
            let digit = key.to_unicode().and_then(|c| c.to_digit(10));
            match digit.and_then(|d| Tab::ALL.get((d as usize).wrapping_sub(1))) {
                Some(tab) => {
                    s.set_tab(*tab);
                    glib::Propagation::Stop
                }
                None => glib::Propagation::Proceed,
            }
        });
        shell.window.add_controller(keys);
        shell.set_tab(Tab::Board);
        shell
    }

    pub fn handle(self: &Rc<Self>, msg: Msg) {
        match msg {
            Msg::Snapshot(s) => {
                *self.snap.borrow_mut() = Rc::new(*s);
                self.refresh();
            }
            Msg::Open(uuid) => self.open(&uuid),
            Msg::Close => self.close(),
            Msg::Restyle => {
                crate::app::apply_styles();
                self.refresh();
            }
        }
    }

    /// Redraws everything that shows the snapshot.
    fn refresh(&self) {
        *self.stale.borrow_mut() = [true; 5];
        self.render_header();
        self.render(self.tab.get());
        let opened = self.opened.borrow().clone();
        if let Some(uuid) = opened {
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
        // Amber only when the user is what the work waits on.
        let (class, why) = if s.error.is_some() {
            ("bad", "The task list could not be read".to_string())
        } else if p.holding > 0 {
            (
                "warn",
                format!("{} of your tasks hold up {} others", p.holding, p.held_up),
            )
        } else if p.decisions_open > 0 {
            ("warn", format!("{} decisions are open", p.decisions_open))
        } else {
            ("ok", "Nothing of yours holds anything up".to_string())
        };
        self.lamp.add_css_class(class);
        self.lamp.set_tooltip_text(Some(&why));
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

    fn open(&self, uuid: &str) {
        let s = self.snap.borrow().clone();
        let Some(card) = s.card(uuid) else {
            // The card left the snapshot: finished long ago, or deleted.
            self.close();
            return;
        };
        let note = match &self.notes {
            Notes::Demo => Some(waybill_core::demo::note(&card.title)),
            Notes::Dir(dir) => dir
                .as_ref()
                .and_then(|d| std::fs::read_to_string(d.join(format!("{}.md", card.uuid))).ok()),
        };
        self.detail.show(&s, card, note.as_deref());
        *self.opened.borrow_mut() = Some(uuid.to_string());
        self.reveal.set_reveal_child(true);
    }

    /// Opens the first card whose title contains `text` (for screenshots).
    pub fn open_titled(&self, text: &str) {
        let s = self.snap.borrow().clone();
        if let Some(c) = s.cards.iter().find(|c| c.title.contains(text)) {
            self.open(&c.uuid);
        }
    }

    fn close(&self) {
        self.opened.borrow_mut().take();
        self.reveal.set_reveal_child(false);
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
