//! Small widget helpers shared by every view.

use crate::theme;
use gtk::glib;
use gtk::prelude::*;
use waybill_core::clock;
use waybill_core::model::{Card, Snapshot};

pub fn esc(s: &str) -> String {
    glib::markup_escape_text(s).to_string()
}

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

pub fn markup(text: &str, classes: &[&str]) -> gtk::Label {
    let l = label("", classes);
    l.set_markup(text);
    l
}

/// A label that wraps to at most `lines` lines, then ends in an ellipsis.
pub fn wrapped(text: &str, classes: &[&str], lines: i32) -> gtk::Label {
    let l = label(text, classes);
    l.set_wrap(true);
    l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    l.set_lines(lines);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l.set_hexpand(true);
    l
}

pub fn hbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, spacing)
}

pub fn vbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}

pub fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

/// Pushes everything after it to the far end of a box.
pub fn spring() -> gtk::Box {
    let s = hbox(0);
    s.set_hexpand(true);
    s
}

pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let s = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(child)
        .build();
    s.set_vexpand(true);
    s
}

/// A project name behind a dot of its color.
pub fn project(name: &str) -> gtk::Label {
    let shown = if name.is_empty() { "no project" } else { name };
    let color = theme::current().project(name).hex();
    let l = markup(
        &format!("<span foreground=\"{color}\">●</span> {}", esc(shown)),
        &["project"],
    );
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l
}

pub fn chip(text: &str, class: &str) -> gtk::Label {
    let l = label(text, &["chip", class]);
    l.set_valign(gtk::Align::Center);
    l
}

/// The priority letter as a small badge; nothing for a task without one.
pub fn priority(p: char) -> Option<gtk::Label> {
    let class = match p {
        'H' => "pri-h",
        'M' => "pri-m",
        'L' => "pri-l",
        _ => return None,
    };
    Some(chip(&p.to_string(), class))
}

pub fn tag_class(tag: &str) -> &'static str {
    match tag {
        "bug" => "bad",
        "research" => "accent",
        "verify" => "warn",
        _ => "plain",
    }
}

/// A clock time in the snapshot's zone.
pub fn hm(s: &Snapshot, ts: i64) -> String {
    clock::hm(ts + s.offset)
}

/// "today 09:22", "yesterday 17:05" or "Nov 12".
pub fn when(s: &Snapshot, ts: i64) -> String {
    let day = |t: i64| (t + s.offset).div_euclid(clock::DAY);
    match day(s.now) - day(ts) {
        0 => hm(s, ts),
        1 => format!("yesterday {}", hm(s, ts)),
        _ => clock::day_label(day(ts)),
    }
}

/// How long a card has been open, or how long it has been held.
pub fn age(s: &Snapshot, c: &Card) -> String {
    clock::span(s.now - c.started.unwrap_or(c.entry))
}

/// A flat, full-width button around `child`.
pub fn row_button(child: &impl IsA<gtk::Widget>, classes: &[&str]) -> gtk::Button {
    let b = gtk::Button::builder().child(child).build();
    b.set_has_frame(false);
    for c in classes {
        b.add_css_class(c);
    }
    b
}

/// Breaks `text` into at most `lines` lines of `width` characters, ending
/// the last one with an ellipsis when it does not fit.
pub fn wrap(text: &str, width: usize, lines: usize) -> Vec<String> {
    let mut out: Vec<String> = vec![String::new()];
    for word in text.split_whitespace() {
        let full = out.len() >= lines;
        let cur = out.last_mut().expect("never empty");
        let need = cur.chars().count() + usize::from(!cur.is_empty()) + word.chars().count();
        if need <= width || cur.is_empty() {
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        } else if full {
            cur.push('…');
            return fit(out, width);
        } else {
            out.push(word.to_string());
        }
    }
    fit(out, width)
}

fn fit(mut lines: Vec<String>, width: usize) -> Vec<String> {
    for l in &mut lines {
        if l.chars().count() > width {
            *l = l.chars().take(width.saturating_sub(1)).collect::<String>() + "…";
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_station_names() {
        assert_eq!(
            wrap("Net autoload, ENet, handshake", 16, 2),
            ["Net autoload,", "ENet, handshake"]
        );
        assert_eq!(
            wrap("Loopback playtest at 0 and 150 ms", 12, 2),
            ["Loopback", "playtest at…"]
        );
        assert_eq!(wrap("Supercalifragilistic", 8, 2), ["Superca…"]);
        assert_eq!(wrap("", 8, 2), [""]);
    }
}
