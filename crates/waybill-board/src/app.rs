//! Startup: styles, the thread that reads the task list, and the file
//! watches that reread it when the database changes and restyle the board
//! when the bar's stylesheet or Waybill's config does.

use crate::shell::{Msg, Shell, World};
use crate::theme::Palette;
use crate::views::Tab;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use waybill_core::clock::{self, Zone};
use waybill_core::config::Config;
use waybill_core::model::Snapshot;

const DEFAULT_CSS: &str = include_str!("style.css");

const HELP: &str = "\
waybill-board: the Waybill window

Usage:
  waybill-board                 Open the board on your task list, or bring
                                the one already open forward
  waybill-board --demo          Open it on a made-up studio's list
  waybill-board --card <uuid>   Open it with one card showing
  waybill-board --fullscreen    Open it fullscreen (--windowed overrides the config)
  waybill-board --drawer        Run the Desk drawer (`waybill toggle` starts it)

For screenshots and checking a layout without a desktop:
  --snapshot <file.json>        Draw a snapshot saved by `waybill snapshot`
  --shot <out.png>              Draw one frame into a PNG and exit
  --tab <board|desk|lines|log|pulse>
  --open <text>                 Open the first card whose title has <text>
  --do <words>                  Type <words> on the / line and press Enter
  --size <width>x<height>";

thread_local! {
    static THEME: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    static USER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    static WATCHES: RefCell<Vec<gio::FileMonitor>> = const { RefCell::new(Vec::new()) };
}

fn provider(
    slot: &'static std::thread::LocalKey<RefCell<Option<gtk::CssProvider>>>,
    priority: u32,
) -> Option<gtk::CssProvider> {
    let display = gdk::Display::default()?;
    Some(slot.with(|s| {
        s.borrow_mut()
            .get_or_insert_with(|| {
                let p = gtk::CssProvider::new();
                gtk::style_context_add_provider_for_display(&display, &p, priority);
                p
            })
            .clone()
    }))
}

/// Builds the palette (built-in, then the bar's look) and loads it with the
/// stylesheet, then the user's own stylesheet over both.
pub fn apply_styles() {
    let (cfg, _) = Config::load();
    let bar = waybill_core::waybar::load_theme();
    let palette = Palette::from(bar.as_ref(), &cfg);
    if let Some(p) = provider(&THEME, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION) {
        p.load_from_string(&format!("{}{DEFAULT_CSS}", palette.css()));
    }
    crate::theme::set_current(palette);
    if let Some(p) = provider(&USER, gtk::STYLE_PROVIDER_PRIORITY_USER) {
        let user = std::fs::read_to_string(waybill_core::paths::style_file()).unwrap_or_default();
        p.load_from_string(&user);
    }
}

fn watch(path: PathBuf, folder: bool, on_change: impl Fn() + 'static) {
    let file = gio::File::for_path(&path);
    let monitor = if folder {
        file.monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
    } else {
        file.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
    };
    if let Ok(m) = monitor {
        m.connect_changed(move |_, _, _, ev| {
            if matches!(
                ev,
                gio::FileMonitorEvent::ChangesDoneHint
                    | gio::FileMonitorEvent::Created
                    | gio::FileMonitorEvent::Deleted
            ) {
                on_change();
            }
        });
        WATCHES.with(|w| w.borrow_mut().push(m));
    }
}

/// What this process shows: the window, or the drawer.
enum Face {
    Board(std::rc::Rc<Shell>),
    Drawer(std::rc::Rc<crate::drawer::Drawer>),
}

/// Where the board's tasks come from.
enum Source {
    Live(Box<Config>),
    Demo,
    File(PathBuf),
}

fn millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Reads the task list, sends it, then waits for a change (or a minute, so
/// "held 12 min" keeps counting). Reading the database can touch its files,
/// so changes seen until `quiet` are our own and are ignored.
fn loader(
    source: Source,
    notes: Option<PathBuf>,
    tx: async_channel::Sender<Msg>,
    wake: mpsc::Receiver<()>,
    quiet: Arc<AtomicI64>,
) {
    loop {
        quiet.store(millis() + 10_000, Ordering::Relaxed);
        let now = clock::now();
        let snap = match &source {
            Source::Live(cfg) => {
                waybill_core::load::snapshot(cfg, notes.as_deref(), now, Zone::Local)
            }
            Source::Demo => waybill_core::demo::snapshot(now, Zone::Local),
            Source::File(path) => std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<Snapshot>(&t).map_err(|e| e.to_string()))
                .unwrap_or_else(|e| {
                    Snapshot::failed(now, Zone::Local, format!("{}: {e}", path.display()))
                }),
        };
        quiet.store(millis() + 1_500, Ordering::Relaxed);
        if std::env::var_os("WAYBILL_TRACE").is_some() {
            eprintln!(
                "waybill-board: read {} cards, {} log lines at {}",
                snap.cards.len(),
                snap.log.len(),
                clock::hm(now + snap.offset)
            );
        }
        if tx.send_blocking(Msg::Snapshot(Box::new(snap))).is_err() {
            return;
        }
        if matches!(source, Source::File(_)) {
            return;
        }
        match wake.recv_timeout(Duration::from_secs(60)) {
            Ok(()) => {
                // Let a burst of writes finish, then read once.
                std::thread::sleep(Duration::from_millis(350));
                while wake.try_recv().is_ok() {}
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// The application id, which compositors see as the window's class.
pub const APP_ID: &str = "io.github.nathanielinman.Waybill";

fn board_socket() -> PathBuf {
    waybill_core::paths::runtime_dir().join("board.sock")
}

/// Hands this launch to a board that is already running: it comes forward,
/// on `card` when one was asked for. False when no board answers.
fn hand_off(card: Option<&str>) -> bool {
    use std::io::Write;
    let Ok(mut s) = std::os::unix::net::UnixStream::connect(board_socket()) else {
        return false;
    };
    writeln!(s, "{}", serde_json::json!({ "present": { "card": card } })).is_ok()
}

/// Listens for later launches. One board holds the socket; a stale file
/// left by a crash is replaced.
fn listen(tx: async_channel::Sender<Msg>) {
    use std::io::{BufRead, BufReader};
    let path = board_socket();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = std::os::unix::net::UnixListener::bind(&path) else {
        return;
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
            if let Some(present) = value.get("present") {
                let card = present
                    .get("card")
                    .and_then(|c| c.as_str())
                    .map(String::from);
                if tx.send_blocking(Msg::Present(card)).is_err() {
                    return;
                }
            }
        }
    });
}

/// Asks the compositor to bring the board forward, wherever it is. A
/// window cannot do this for itself on Wayland; without a compositor we
/// know how to ask, it stays where it is and shows as wanting attention.
pub fn raise() {
    let run = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    match bar_anchor::compositor() {
        Some("Hyprland") => {
            // Hyprland with a Lua config takes a dispatcher expression;
            // older ones take the dispatcher's name.
            let lua = format!("hl.dsp.focus({{ window = \"class:{APP_ID}\" }})");
            if run("hyprctl", &["dispatch", &lua]).contains("error") {
                run(
                    "hyprctl",
                    &["dispatch", "focuswindow", &format!("class:{APP_ID}")],
                );
            }
        }
        Some("sway") => {
            run("swaymsg", &[&format!("[app_id=\"{APP_ID}\"]"), "focus"]);
        }
        _ => {}
    }
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

pub fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("waybill-board {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return glib::ExitCode::SUCCESS;
    }
    // The board on the real list is one window. A second launch hands
    // itself to the first, which is how the drawer and the bar reach a
    // board that lives on its own workspace. The demo, a saved snapshot, a
    // screenshot and the drawer are their own processes.
    let resident = !["--demo", "--snapshot", "--shot", "--drawer"]
        .iter()
        .any(|flag| args.iter().any(|a| a == flag));
    if resident && hand_off(arg(&args, "--card").as_deref()) {
        return glib::ExitCode::SUCCESS;
    }
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        // A second window (a demo beside the real one) is its own process.
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let built = std::rc::Rc::new(std::cell::Cell::new(false));
    app.connect_activate(move |app| {
        if built.replace(true) {
            return;
        }
        let shot = arg(&args, "--shot");
        if shot.is_some() {
            crate::charts::STILL.store(true, Ordering::Relaxed);
        }
        apply_styles();
        let (cfg, problem) = Config::load();
        if let Some(p) = problem {
            eprintln!("waybill-board: config ignored: {p}");
        }
        let (source, notes, data) = if args.iter().any(|a| a == "--demo") {
            (Source::Demo, None, None)
        } else if let Some(file) = arg(&args, "--snapshot") {
            (
                Source::File(PathBuf::from(file)),
                waybill_core::load::notes_dir(&cfg),
                None,
            )
        } else {
            let data = waybill_core::load::data_dir(&cfg);
            (
                Source::Live(Box::new(cfg.clone())),
                waybill_core::load::notes_dir(&cfg),
                data,
            )
        };
        let size = arg(&args, "--size")
            .and_then(|s| {
                let (w, h) = s.split_once('x')?;
                Some((w.parse().ok()?, h.parse().ok()?))
            })
            .unwrap_or((1560, 960));

        let (tx, rx) = async_channel::unbounded::<Msg>();
        let (wake, woken) = mpsc::channel::<()>();
        let world = World {
            desk: matches!(source, Source::Live(_)).then(|| {
                Arc::new(waybill_core::agent::Desk {
                    cfg: cfg.clone(),
                    notes: notes.clone(),
                    zone: Zone::Local,
                })
            }),
            demo: matches!(source, Source::Demo),
        };
        let drawer = args.iter().any(|a| a == "--drawer");
        let face = if drawer {
            if shot.is_none() && !crate::drawer::listen(tx.clone()) {
                eprintln!("waybill-board: a drawer is already running");
                app.quit();
                return;
            }
            // One drawer open at a time across the apps: another app's
            // drawer opening closes this one.
            if shot.is_none() {
                let tx = tx.clone();
                match bar_anchor::drawers::listen("waybill", move || {
                    let _ = tx.send_blocking(Msg::OtherDrawerOpened);
                }) {
                    // Lives as long as the drawer process does.
                    Ok(l) => std::mem::forget(l),
                    Err(e) => eprintln!("waybill-board: {e}"),
                }
            }
            // Asking a display that is not Wayland makes the library complain.
            let wayland = gdk::Display::default().is_some_and(|d| d.backend().is_wayland());
            let layer = shot.is_none() && wayland && gtk4_layer_shell::is_supported();
            Face::Drawer(crate::drawer::Drawer::new(
                app,
                tx.clone(),
                world,
                wake.clone(),
                layer,
            ))
        } else {
            let shell = Shell::new(app, tx.clone(), world, wake.clone(), size);
            if resident {
                listen(tx.clone());
            }
            let fullscreen = !args.iter().any(|a| a == "--windowed")
                && (cfg.board.fullscreen || args.iter().any(|a| a == "--fullscreen"));
            if fullscreen && shot.is_none() {
                shell.window.fullscreen();
            }
            if let Some(tab) = arg(&args, "--tab").as_deref().and_then(Tab::parse) {
                shell.set_tab(tab);
            }
            Face::Board(shell)
        };

        let quiet = Arc::new(AtomicI64::new(0));
        if shot.is_none() {
            // The database and the notes beside it.
            for dir in data.into_iter().chain(notes.clone()) {
                let (wake, quiet) = (wake.clone(), quiet.clone());
                watch(dir, true, move || {
                    if millis() > quiet.load(Ordering::Relaxed) {
                        let _ = wake.send(());
                    }
                });
            }
            for path in [
                waybill_core::waybar::style_path(),
                waybill_core::paths::style_file(),
                waybill_core::paths::config_file(),
            ] {
                let tx = tx.clone();
                watch(path, false, move || {
                    let _ = tx.send_blocking(Msg::Restyle);
                });
            }
        }
        let loader_tx = tx.clone();
        std::thread::spawn(move || {
            // Held so the channel stays open for as long as the loader runs.
            let _wake = wake;
            loader(source, notes, loader_tx, woken, quiet)
        });

        let open = arg(&args, "--open");
        let card = arg(&args, "--card");
        let typed = arg(&args, "--do");
        let app = app.clone();
        match &face {
            Face::Board(shell) => shell.window.present(),
            // The drawer stays hidden until it is toggled; hold the app open.
            Face::Drawer(_) => std::mem::forget(app.hold()),
        }
        glib::spawn_future_local(async move {
            let mut first = true;
            while let Ok(msg) = rx.recv().await {
                let is_snapshot = matches!(msg, Msg::Snapshot(_));
                match &face {
                    Face::Board(shell) => shell.handle(msg),
                    Face::Drawer(d) => d.handle(msg),
                }
                if !(first && is_snapshot) {
                    continue;
                }
                first = false;
                match &face {
                    Face::Board(shell) => {
                        if let Some(text) = &open {
                            shell.open_titled(text);
                        }
                        if let Some(id) = &card {
                            shell.open_card(id);
                        }
                        if let Some(text) = &typed {
                            shell.type_line(text);
                        }
                        if let Some(out) = shot.clone() {
                            shell.shoot(out, size, app.clone());
                        }
                    }
                    Face::Drawer(d) => {
                        if let Some(out) = shot.clone() {
                            d.shoot(out, app.clone());
                        }
                    }
                }
            }
        });
    });
    app.run_with_args::<&str>(&[])
}
