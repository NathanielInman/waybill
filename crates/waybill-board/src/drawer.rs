//! The Desk drawer: a small layer-shell panel that opens from the bar icon.
//! What waits on the user, what it frees, and decisions answered in one
//! click, without going to the board.

use crate::actions::{self, Action};
use crate::anim::{Motion, Spring};
use crate::charts::{self, Canvas};
use crate::shell::{Msg, World};
use crate::stage::Stage;
use crate::theme::{self, Rgba};
use crate::ui::{self, chip, hbox, label, vbox};
use crate::views::Tx;
use bar_anchor::Anchor;
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;
use waybill_core::clock;
use waybill_core::model::{Card, Lane, Snapshot};

const WIDTH: f64 = 460.0;
const MAX_HEIGHT: f64 = 780.0;
const GAP: f64 = 8.0;
/// How many of the user's tasks the drawer lists before it says "and more".
const ROWS: usize = 6;

pub struct Drawer {
    window: gtk::ApplicationWindow,
    stage: Stage,
    spring: Spring,
    lamp: gtk::Label,
    counts: gtk::Label,
    headline: gtk::Label,
    list: gtk::Box,
    spark: charts::Spark,
    today: gtk::Label,
    said: gtk::Label,
    visible: Cell<bool>,
    snap: RefCell<Rc<Snapshot>>,
    world: Rc<World>,
    tx: Tx,
    wake: mpsc::Sender<()>,
}

fn find_monitor(name: &str) -> Option<gdk::Monitor> {
    let list = gdk::Display::default()?.monitors();
    (0..list.n_items())
        .filter_map(|i| list.item(i).and_downcast::<gdk::Monitor>())
        .find(|m| m.connector().is_some_and(|c| c == name))
}

/// Opens the board on a card, as its own window.
fn open_board(uuid: Option<&str>) {
    let board = std::env::current_exe().unwrap_or_else(|_| "waybill-board".into());
    let mut c = std::process::Command::new(board);
    if let Some(u) = uuid {
        c.args(["--card", u]);
    }
    let _ = c.spawn();
}

impl Drawer {
    pub fn new(
        app: &gtk::Application,
        tx: Tx,
        world: World,
        wake: mpsc::Sender<()>,
        layer: bool,
    ) -> Rc<Drawer> {
        let window = gtk::ApplicationWindow::new(app);
        window.add_css_class("waybill");
        window.add_css_class("drawer");
        window.set_decorated(false);
        window.set_title(Some("Waybill"));
        if layer {
            window.init_layer_shell();
            window.set_layer(Layer::Overlay);
            window.set_namespace(Some("waybill"));
            for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
                window.set_anchor(e, true);
            }
            window.set_exclusive_zone(0);
            window.set_keyboard_mode(KeyboardMode::Exclusive);
        }

        let panel = vbox(10);
        panel.add_css_class("panel");
        let header = hbox(10);
        let lamp = label("●", &["lamp", "ok", "brand-lamp"]);
        header.append(&lamp);
        header.append(&label("WAYBILL", &["brand"]));
        let counts = label("", &["dim", "small"]);
        counts.set_hexpand(true);
        counts.set_ellipsize(gtk::pango::EllipsizeMode::End);
        header.append(&counts);
        let board = gtk::Button::with_label("Open the board");
        board.add_css_class("act");
        header.append(&board);
        panel.append(&header);

        let headline = ui::wrapped("", &["headline"], 3);
        panel.append(&headline);
        let list = vbox(5);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(520)
            .child(&list)
            .build();
        panel.append(&scroll);

        let tile = hbox(12);
        tile.add_css_class("tile");
        let spark = Canvas::new(
            150,
            30,
            (vec![], theme::current().ok),
            |f, d: &(Vec<f32>, Rgba)| charts::spark(f, &d.0, d.1),
        );
        spark.area.set_valign(gtk::Align::Center);
        let today = ui::wrapped("", &["small"], 2);
        tile.append(&spark.area);
        tile.append(&today);
        panel.append(&tile);
        let said = label("", &["toast", "small"]);
        said.set_ellipsize(gtk::pango::EllipsizeMode::End);
        panel.append(&said);

        let spring = Spring::new(&panel, Motion::PANEL);
        let stage = Stage::new(&spring, WIDTH, MAX_HEIGHT, GAP);
        if !layer {
            // A plain window the panel is centered in.
            window.set_default_size(WIDTH as i32 + 120, 900);
        }
        window.set_child(Some(&stage));

        let d = Rc::new(Drawer {
            window,
            stage,
            spring,
            lamp,
            counts,
            headline,
            list,
            spark,
            today,
            said,
            visible: Cell::new(false),
            snap: RefCell::new(Rc::new(Snapshot::default())),
            world: Rc::new(world),
            tx,
            wake,
        });
        let w = Rc::downgrade(&d);
        d.stage.connect_outside(move || {
            if let Some(d) = w.upgrade() {
                d.hide();
            }
        });
        let s = d.spring.clone();
        d.stage.connect_placed(move |(x, y)| s.set_origin(x, y));
        let w = Rc::downgrade(&d);
        board.connect_clicked(move |_| {
            open_board(None);
            if let Some(d) = w.upgrade() {
                d.hide();
            }
        });
        let keys = gtk::EventControllerKey::new();
        let w = Rc::downgrade(&d);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(d) = w.upgrade()
            {
                d.hide();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        d.window.add_controller(keys);
        d
    }

    pub fn handle(self: &Rc<Self>, msg: Msg) {
        match msg {
            Msg::Snapshot(s) => {
                *self.snap.borrow_mut() = Rc::new(*s);
                if self.visible.get() {
                    self.render();
                }
            }
            Msg::Toggle(anchor) => {
                if std::env::var_os("WAYBILL_TRACE").is_some() {
                    eprintln!(
                        "waybill-board: toggle, {} an anchor, drawer was {}",
                        if anchor.is_some() { "with" } else { "without" },
                        if self.visible.get() { "open" } else { "closed" }
                    );
                }
                if self.visible.get() {
                    self.hide();
                } else {
                    self.show(anchor);
                }
            }
            Msg::Open(uuid) => {
                open_board(Some(&uuid));
                self.hide();
            }
            Msg::Do(action) => self.act(action),
            Msg::Did(result) => {
                match result {
                    Ok(said) => {
                        self.said.remove_css_class("bad");
                        self.said.set_label(&said);
                    }
                    Err(e) => {
                        self.said.add_css_class("bad");
                        self.said.set_label(&e);
                    }
                }
                let _ = self.wake.send(());
            }
            Msg::Restyle => {
                crate::app::apply_styles();
                self.render();
            }
            Msg::Close => self.hide(),
            Msg::Reopen | Msg::New(_) => {}
        }
    }

    fn act(&self, action: Action) {
        let Some(desk) = self.world.desk.clone() else {
            self.said.add_css_class("bad");
            self.said
                .set_label("This list is read-only here; nothing was written.");
            return;
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result =
                actions::perform(&desk, action, clock::now()).map_err(|e| format!("{e:#}"));
            let _ = tx.send_blocking(Msg::Did(result));
        });
    }

    fn render(&self) {
        let s = self.snap.borrow().clone();
        let pal = theme::current();
        let p = &s.pulse;
        for c in ["ok", "warn", "bad"] {
            self.lamp.remove_css_class(c);
        }
        let lamp = waybill_core::lamp::lamp(&s);
        self.lamp.add_css_class(match lamp.state {
            waybill_core::lamp::State::Calm => "ok",
            waybill_core::lamp::State::NeedsYou => "warn",
            waybill_core::lamp::State::Error => "bad",
        });
        self.counts.set_label(&format!(
            "{} ready · {} doing",
            p.lanes.ready, p.lanes.doing
        ));
        let yours = s.lane(Lane::Yours);
        let mut text = match (&s.error, yours.len()) {
            (Some(e), _) => ui::esc(&format!("The task list could not be read: {e}")),
            (None, 0) => "Nothing is waiting on you.".to_string(),
            (None, 1) => "One task waits on you.".to_string(),
            (None, n) => format!("{n} tasks wait on you."),
        };
        if p.holding > 0 {
            text.push_str(&format!(
                " <span foreground=\"{}\">{} of them hold{} up {} other{}.</span>",
                pal.warn.hex(),
                p.holding,
                if p.holding == 1 { "s" } else { "" },
                p.held_up,
                if p.held_up == 1 { "" } else { "s" },
            ));
        }
        self.headline.set_markup(&text);

        ui::clear(&self.list);
        // Decisions first: they are the ones a click here can settle.
        let (decisions, rest): (Vec<&Card>, Vec<&Card>) = yours.iter().partition(|c| c.decision);
        for c in decisions.iter().take(3) {
            self.list.append(&self.row(c));
            let options = self.world.options(c);
            if !options.is_empty() {
                let strip = hbox(6);
                strip.add_css_class("options");
                for option in options.iter().take(3) {
                    let b = gtk::Button::with_label(option);
                    b.add_css_class("act");
                    b.add_css_class("choice");
                    if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                        l.set_max_width_chars(18);
                    }
                    b.set_tooltip_text(Some(option));
                    let (tx, uuid, choice) = (self.tx.clone(), c.uuid.clone(), option.clone());
                    b.connect_clicked(move |_| {
                        let _ =
                            tx.send_blocking(Msg::Do(Action::Rule(uuid.clone(), choice.clone())));
                    });
                    strip.append(&b);
                }
                self.list.append(&strip);
            }
        }
        let shown = ROWS.saturating_sub(decisions.len().min(3));
        for c in rest.iter().take(shown) {
            self.list.append(&self.row(c));
        }
        let more = yours
            .len()
            .saturating_sub(decisions.len().min(3) + rest.len().min(shown));
        if more > 0 {
            self.list.append(&label(
                &format!("and {more} more on the board"),
                &["dim", "small"],
            ));
        }

        let n = p.days.len();
        self.spark.set((
            p.days
                .iter()
                .skip(n.saturating_sub(14))
                .map(|d| d.done as f32)
                .collect(),
            pal.ok,
        ));
        self.today.set_label(&format!(
            "{} finished today, {} this week. {} added.",
            p.lanes.done, p.done_week, p.added_week
        ));
    }

    fn row(&self, c: &Card) -> gtk::Button {
        let line = hbox(10);
        let mark = if !c.frees.is_empty() {
            label(&c.frees.len().to_string(), &["big", "num", "warn"])
        } else if c.decision {
            label("◆", &["violet"])
        } else {
            label("■", &["warn", "small"])
        };
        mark.set_size_request(22, -1);
        mark.set_xalign(0.5);
        line.append(&mark);
        let mid = vbox(2);
        mid.set_hexpand(true);
        let title = label(&c.title, &["row-title"]);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        mid.append(&title);
        let meta = hbox(6);
        meta.append(&ui::project(&c.project));
        if let Some(p) = ui::priority(c.priority) {
            meta.append(&p);
        }
        if !c.frees.is_empty() {
            meta.append(&chip(&format!("frees {}", c.frees.len()), "warn"));
        }
        mid.append(&meta);
        line.append(&mid);
        let b = ui::row_button(&line, &["row"]);
        b.set_tooltip_text(Some("Open it on the board"));
        let (tx, uuid) = (self.tx.clone(), c.uuid.clone());
        b.connect_clicked(move |_| {
            let _ = tx.send_blocking(Msg::Open(uuid.clone()));
        });
        b
    }

    fn show(self: &Rc<Self>, anchor: Option<Anchor>) {
        if let Some(name) = anchor.as_ref().and_then(|a| a.monitor.clone())
            && let Some(m) = find_monitor(&name)
        {
            self.window.set_monitor(Some(&m));
        }
        self.stage.set_anchor(anchor);
        let was_visible = self.visible.replace(true);
        self.said.set_label("");
        self.render();
        self.window.set_visible(true);
        self.window.present();
        if !was_visible {
            self.spring.play_in(0);
            self.spark.play();
        }
    }

    fn hide(self: &Rc<Self>) {
        if !self.visible.replace(false) {
            return;
        }
        let w = Rc::downgrade(self);
        self.spring.play_out(move || {
            if let Some(d) = w.upgrade()
                && !d.visible.get()
            {
                d.window.set_visible(false);
            }
        });
    }

    /// Draws the panel into a PNG, then quits.
    pub fn shoot(self: &Rc<Self>, out: String, app: gtk::Application) {
        self.show(None);
        let d = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(1300), move || {
            d.spring.settle();
            let tries = Rc::new(Cell::new(0));
            glib::timeout_add_local(Duration::from_millis(100), move || {
                let widget: gtk::Widget = d.spring.clone().upcast();
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

/// Listens for `waybill toggle`. Returns false when another drawer already
/// holds the socket.
pub fn listen(tx: Tx) -> bool {
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::{UnixListener, UnixStream};
    let dir = waybill_core::paths::runtime_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("drawer.sock");
    if UnixStream::connect(&path).is_ok() {
        return false;
    }
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else {
        return false;
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(stream).read_line(&mut line).is_err() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if let Some(anchor) = value.get("toggle") {
                let anchor = serde_json::from_value::<Anchor>(anchor.clone()).ok();
                if tx.send_blocking(Msg::Toggle(anchor)).is_err() {
                    return;
                }
            }
        }
    });
    true
}
