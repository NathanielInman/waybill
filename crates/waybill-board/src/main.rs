//! `waybill-board`: the Waybill window. A GTK4 app that draws the task list
//! as a board, a desk, rail lines, a log and charts, and redraws when the
//! task database changes.

#![cfg_attr(not(target_os = "linux"), allow(unused))]

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("waybill-board is built for Linux (GTK4)");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
mod anim;
#[cfg(target_os = "linux")]
mod app;
#[cfg(target_os = "linux")]
mod charts;
#[cfg(target_os = "linux")]
mod detail;
#[cfg(target_os = "linux")]
mod drawer;
#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
mod shell;
#[cfg(target_os = "linux")]
mod stage;
#[cfg(target_os = "linux")]
mod theme;
#[cfg(target_os = "linux")]
mod ui;
#[cfg(target_os = "linux")]
mod views;

#[cfg(target_os = "linux")]
fn main() -> gtk::glib::ExitCode {
    app::main()
}
