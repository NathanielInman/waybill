//! Everything drawn rather than laid out: the charts, the rail lines and the
//! small marks on cards. Each one is a `Canvas`: a drawing area with its
//! data, a grow-in animation and the pointer's position.

use crate::theme::{self, Palette, Rgba};
use crate::ui::wrap;
use gtk::cairo;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use waybill_core::lines::{Line, StopState};
use waybill_core::pulse::{Bucket, DayStat};

/// Set for screenshots: every chart draws fully grown and nothing pulses.
pub static STILL: AtomicBool = AtomicBool::new(false);

pub fn animated() -> bool {
    !STILL.load(Ordering::Relaxed)
        && gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_animations())
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// What a draw function gets: the context, the size, and where things stand.
pub struct Frame<'a> {
    pub cr: &'a cairo::Context,
    pub w: f64,
    pub h: f64,
    /// 0 to 1 while the chart grows in.
    pub grow: f64,
    pub hover: Option<(f64, f64)>,
    pub pal: Palette,
    /// Seconds since the canvas was made, for things that pulse.
    pub beat: f64,
}

impl Frame<'_> {
    pub fn ink(&self, c: Rgba, alpha: f64) {
        self.cr.set_source_rgba(c.0, c.1, c.2, c.3 * alpha);
    }

    pub fn font(&self, size: f64, bold: bool) {
        self.cr.select_font_face(
            &self.pal.font,
            cairo::FontSlant::Normal,
            if bold {
                cairo::FontWeight::Bold
            } else {
                cairo::FontWeight::Normal
            },
        );
        self.cr.set_font_size(size);
    }

    pub fn width(&self, s: &str) -> f64 {
        self.cr
            .text_extents(s)
            .map(|e| e.x_advance())
            .unwrap_or(0.0)
    }

    /// Draws `s` with its baseline at `y`; returns its width.
    pub fn text(&self, s: &str, x: f64, y: f64, align: Align) -> f64 {
        let w = self.width(s);
        let x = match align {
            Align::Left => x,
            Align::Center => x - w / 2.0,
            Align::Right => x - w,
        };
        self.cr.move_to(x.round(), y.round());
        let _ = self.cr.show_text(s);
        w
    }

    pub fn round_rect(&self, x: f64, y: f64, w: f64, h: f64, r: f64) {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let cr = self.cr;
        cr.new_sub_path();
        cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
        cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
        cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
        cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
        cr.close_path();
    }

    fn fill(&self) {
        let _ = self.cr.fill();
    }

    fn stroke(&self) {
        let _ = self.cr.stroke();
    }

    /// A small box of lines next to the pointer, kept inside the canvas.
    fn tooltip(&self, x: f64, y: f64, lines: &[(String, Rgba)]) {
        self.font(11.0, false);
        let w = lines.iter().map(|(l, _)| self.width(l)).fold(0.0, f64::max) + 18.0;
        let h = lines.len() as f64 * 16.0 + 10.0;
        let x = if x + 14.0 + w > self.w {
            x - 14.0 - w
        } else {
            x + 14.0
        };
        let y = (y - h / 2.0).clamp(2.0, (self.h - h - 2.0).max(2.0));
        self.round_rect(x, y, w, h, 6.0);
        self.ink(self.pal.well, 0.96);
        let _ = self.cr.fill_preserve();
        self.ink(self.pal.border, 1.0);
        self.cr.set_line_width(1.0);
        self.stroke();
        for (i, (line, color)) in lines.iter().enumerate() {
            self.ink(*color, 1.0);
            self.text(line, x + 9.0, y + 17.0 + i as f64 * 16.0, Align::Left);
        }
    }
}

/// A drawing area that owns its data.
pub struct Canvas<T: 'static> {
    pub area: gtk::DrawingArea,
    data: Rc<RefCell<T>>,
    grow: Rc<Cell<f64>>,
}

impl<T: 'static> Clone for Canvas<T> {
    fn clone(&self) -> Self {
        Canvas {
            area: self.area.clone(),
            data: self.data.clone(),
            grow: self.grow.clone(),
        }
    }
}

impl<T: 'static> Canvas<T> {
    /// `width` 0 means "as wide as there is room".
    pub fn new(width: i32, height: i32, data: T, draw: impl Fn(&Frame, &T) + 'static) -> Canvas<T> {
        let area = gtk::DrawingArea::new();
        area.set_content_height(height);
        if width > 0 {
            area.set_content_width(width);
        } else {
            area.set_hexpand(true);
        }
        let data = Rc::new(RefCell::new(data));
        let grow = Rc::new(Cell::new(1.0));
        let hover: Rc<Cell<Option<(f64, f64)>>> = Rc::new(Cell::new(None));
        let motion = gtk::EventControllerMotion::new();
        let (h, a) = (hover.clone(), area.clone());
        motion.connect_motion(move |_, x, y| {
            h.set(Some((x, y)));
            a.queue_draw();
        });
        let (h, a) = (hover.clone(), area.clone());
        motion.connect_leave(move |_| {
            h.set(None);
            a.queue_draw();
        });
        area.add_controller(motion);
        let born = std::time::Instant::now();
        let (d, g) = (data.clone(), grow.clone());
        area.set_draw_func(move |_, cr, w, h| {
            let frame = Frame {
                cr,
                w: w as f64,
                h: h as f64,
                grow: g.get(),
                hover: hover.get(),
                pal: theme::current(),
                beat: if animated() {
                    born.elapsed().as_secs_f64()
                } else {
                    0.0
                },
            };
            draw(&frame, &d.borrow());
        });
        Canvas { area, data, grow }
    }

    pub fn set(&self, data: T) {
        *self.data.borrow_mut() = data;
        self.area.queue_draw();
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.data.borrow())
    }

    /// Grows the chart in from nothing.
    pub fn play(&self) {
        if !animated() {
            self.grow.set(1.0);
            self.area.queue_draw();
            return;
        }
        self.grow.set(0.0);
        let grow = self.grow.clone();
        let start: Cell<Option<i64>> = Cell::new(None);
        self.area.add_tick_callback(move |a, clock| {
            let now = clock.frame_time();
            let t0 = start.get().unwrap_or(now);
            start.set(Some(t0));
            let t = ((now - t0) as f64 / 700_000.0).min(1.0);
            grow.set(1.0 - (1.0 - t).powi(3));
            a.queue_draw();
            if t >= 1.0 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Keeps redrawing while on screen, for charts with something that
    /// pulses.
    pub fn keep_beating(&self) {
        if animated() {
            self.area.add_tick_callback(|a, _| {
                a.queue_draw();
                glib::ControlFlow::Continue
            });
        }
    }
}

/// A sparkline: its values and its color.
pub type Spark = Canvas<(Vec<f32>, Rgba)>;

/// A round number at or above `v` for the top of an axis, chosen so the
/// four grid lines under it are whole numbers too.
fn nice(v: f64) -> f64 {
    if v <= 8.0 {
        return if v <= 4.0 { 4.0 } else { 8.0 };
    }
    let mag = 10f64.powf((v / 1.2).log10().floor()).max(10.0);
    for step in [1.2, 1.6, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0] {
        if v <= step * mag {
            return step * mag;
        }
    }
    12.0 * mag
}

/// A line through `pts` that eases between them without overshooting.
fn smooth(cr: &cairo::Context, pts: &[(f64, f64)]) {
    for (i, &(x, y)) in pts.iter().enumerate() {
        if i == 0 {
            cr.line_to(x, y);
        } else {
            let (px, py) = pts[i - 1];
            let mx = (px + x) / 2.0;
            cr.curve_to(mx, py, mx, y, x, y);
        }
    }
}

struct Plot {
    x0: f64,
    y0: f64,
    w: f64,
    h: f64,
    top: f64,
}

impl Plot {
    fn x(&self, i: usize, n: usize) -> f64 {
        self.x0 + self.w * i as f64 / (n.max(2) - 1) as f64
    }

    fn y(&self, v: f64, grow: f64) -> f64 {
        self.y0 + self.h - (v / self.top).clamp(0.0, 1.0) * self.h * grow
    }
}

/// Grid lines, their values, weekend bands and a date under every Monday.
fn axes(f: &Frame, p: &Plot, days: &[DayStat]) {
    let n = days.len();
    let step = p.w / (n.max(2) - 1) as f64;
    for (i, d) in days.iter().enumerate() {
        if d.weekday >= 5 {
            f.ink(f.pal.fg, 0.035);
            f.cr.rectangle(p.x(i, n) - step / 2.0, p.y0, step, p.h);
            f.fill();
        }
    }
    f.font(10.0, false);
    f.cr.set_line_width(1.0);
    for k in 0..=4 {
        let v = p.top * k as f64 / 4.0;
        let y = (p.y0 + p.h - p.h * k as f64 / 4.0).round() + 0.5;
        f.ink(f.pal.fg, if k == 0 { 0.22 } else { 0.07 });
        f.cr.move_to(p.x0, y);
        f.cr.line_to(p.x0 + p.w, y);
        f.stroke();
        f.ink(f.pal.dim, 1.0);
        f.text(&format!("{}", v.round()), p.x0 - 8.0, y + 3.5, Align::Right);
    }
    for (i, d) in days.iter().enumerate() {
        if d.weekday == 0 {
            f.ink(f.pal.dim, 1.0);
            f.text(&d.label, p.x(i, n), p.y0 + p.h + 16.0, Align::Center);
        }
    }
}

fn area(f: &Frame, p: &Plot, pts: &[(f64, f64)], color: Rgba, strength: f64) {
    let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
        return;
    };
    let base = p.y0 + p.h;
    f.cr.move_to(first.0, base);
    smooth(f.cr, pts);
    f.cr.line_to(last.0, base);
    f.cr.close_path();
    let g = cairo::LinearGradient::new(0.0, p.y0, 0.0, base);
    g.add_color_stop_rgba(0.0, color.0, color.1, color.2, 0.46 * strength);
    g.add_color_stop_rgba(1.0, color.0, color.1, color.2, 0.02);
    let _ = f.cr.set_source(&g);
    f.fill();
}

fn line(f: &Frame, pts: &[(f64, f64)], color: Rgba, width: f64) {
    f.cr.new_path();
    smooth(f.cr, pts);
    f.ink(color, 1.0);
    f.cr.set_line_width(width);
    f.cr.set_line_join(cairo::LineJoin::Round);
    f.stroke();
}

/// A dot with a soft ring around it: the end of a line, or a hovered point.
fn bead(f: &Frame, x: f64, y: f64, color: Rgba) {
    f.ink(color, 0.22);
    f.cr.arc(x, y, 7.0, 0.0, TAU);
    f.fill();
    f.ink(f.pal.panel, 1.0);
    f.cr.arc(x, y, 4.0, 0.0, TAU);
    f.fill();
    f.ink(color, 1.0);
    f.cr.arc(x, y, 2.8, 0.0, TAU);
    f.fill();
}

fn nearest(f: &Frame, p: &Plot, n: usize) -> Option<usize> {
    let (x, _) = f.hover?;
    if n == 0 || x < p.x0 - 6.0 || x > p.x0 + p.w + 6.0 {
        return None;
    }
    let i = ((x - p.x0) / p.w * (n.max(2) - 1) as f64).round();
    Some((i.max(0.0) as usize).min(n - 1))
}

fn legend(f: &Frame, x: f64, y: f64, items: &[(&str, Rgba)]) {
    f.font(10.5, false);
    let mut x = x;
    for (name, color) in items {
        f.ink(*color, 1.0);
        f.round_rect(x, y - 8.0, 10.0, 10.0, 3.0);
        f.fill();
        f.ink(f.pal.fg, 1.0);
        x += 16.0 + f.text(name, x + 15.0, y + 1.0, Align::Left) + 14.0;
    }
}

/// Finished and added per day: the main chart.
pub fn flow(f: &Frame, days: &[DayStat]) {
    let p = Plot {
        x0: 40.0,
        y0: 30.0,
        w: f.w - 56.0,
        h: f.h - 56.0,
        top: nice(days.iter().map(|d| d.done.max(d.added)).max().unwrap_or(0) as f64),
    };
    if p.w < 40.0 || days.is_empty() {
        return;
    }
    let n = days.len();
    axes(f, &p, days);
    let done: Vec<(f64, f64)> = (0..n)
        .map(|i| (p.x(i, n), p.y(days[i].done as f64, f.grow)))
        .collect();
    let added: Vec<(f64, f64)> = (0..n)
        .map(|i| (p.x(i, n), p.y(days[i].added as f64, f.grow)))
        .collect();
    area(f, &p, &done, f.pal.accent, 1.0);
    f.cr.set_dash(&[2.0, 4.0], 0.0);
    f.cr.set_line_cap(cairo::LineCap::Round);
    line(f, &added, f.pal.warn, 1.6);
    f.cr.set_dash(&[], 0.0);
    line(f, &done, f.pal.accent, 2.2);
    legend(
        f,
        p.x0,
        14.0,
        &[("finished", f.pal.accent), ("added", f.pal.warn)],
    );
    match nearest(f, &p, n) {
        Some(i) => {
            let x = done[i].0;
            f.ink(f.pal.fg, 0.25);
            f.cr.set_line_width(1.0);
            f.cr.move_to(x.round() + 0.5, p.y0);
            f.cr.line_to(x.round() + 0.5, p.y0 + p.h);
            f.stroke();
            bead(f, x, added[i].1, f.pal.warn);
            bead(f, x, done[i].1, f.pal.accent);
            let d = &days[i];
            f.tooltip(
                x,
                done[i].1.min(added[i].1),
                &[
                    (d.label.clone(), f.pal.bright),
                    (format!("{} finished", d.done), f.pal.accent),
                    (format!("{} added", d.added), f.pal.warn),
                    (format!("{} visits", d.visits), f.pal.dim),
                ],
            );
        }
        None => {
            if let Some(&(x, y)) = done.last() {
                bead(f, x, y, f.pal.accent);
            }
        }
    }
}

/// Open work at the end of each day, the user's stacked on the agents'.
pub fn open_work(f: &Frame, days: &[DayStat]) {
    let p = Plot {
        x0: 40.0,
        y0: 30.0,
        w: f.w - 56.0,
        h: f.h - 56.0,
        top: nice(
            days.iter()
                .map(|d| d.open_yours + d.open_agents)
                .max()
                .unwrap_or(0) as f64,
        ),
    };
    if p.w < 40.0 || days.is_empty() {
        return;
    }
    let n = days.len();
    axes(f, &p, days);
    let agents: Vec<(f64, f64)> = (0..n)
        .map(|i| (p.x(i, n), p.y(days[i].open_agents as f64, f.grow)))
        .collect();
    let both: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            (
                p.x(i, n),
                p.y((days[i].open_agents + days[i].open_yours) as f64, f.grow),
            )
        })
        .collect();
    // The top band is the user's: fill between the two lines.
    f.cr.new_path();
    smooth(f.cr, &both);
    let back: Vec<(f64, f64)> = agents.iter().rev().copied().collect();
    smooth(f.cr, &back);
    f.cr.close_path();
    f.ink(f.pal.warn, 0.30);
    f.fill();
    area(f, &p, &agents, f.pal.accent, 0.8);
    line(f, &agents, f.pal.accent, 1.8);
    line(f, &both, f.pal.warn, 1.8);
    legend(
        f,
        p.x0,
        14.0,
        &[
            ("waiting on you", f.pal.warn),
            ("waiting on agents", f.pal.accent),
        ],
    );
    if let Some(i) = nearest(f, &p, n) {
        let x = both[i].0;
        f.ink(f.pal.fg, 0.25);
        f.cr.set_line_width(1.0);
        f.cr.move_to(x.round() + 0.5, p.y0);
        f.cr.line_to(x.round() + 0.5, p.y0 + p.h);
        f.stroke();
        bead(f, x, agents[i].1, f.pal.accent);
        bead(f, x, both[i].1, f.pal.warn);
        let d = &days[i];
        f.tooltip(
            x,
            both[i].1,
            &[
                (d.label.clone(), f.pal.bright),
                (format!("{} waiting on you", d.open_yours), f.pal.warn),
                (format!("{} waiting on agents", d.open_agents), f.pal.accent),
            ],
        );
    } else if let (Some(&(x, y)), Some(&(_, ya))) = (both.last(), agents.last()) {
        bead(f, x, ya, f.pal.accent);
        bead(f, x, y, f.pal.warn);
    }
}

/// Visits by weekday and hour.
pub fn punch(f: &Frame, grid: &[Vec<u32>]) {
    let (x0, y0) = (40.0, 8.0);
    let cell = ((f.w - x0 - 8.0) / 24.0)
        .min((f.h - y0 - 22.0) / 7.0)
        .max(4.0);
    let gap = (cell * 0.14).max(1.5);
    let max = grid.iter().flatten().copied().max().unwrap_or(0).max(1) as f64;
    let width = cell * 24.0;
    let x0 = x0 + ((f.w - x0 - 8.0 - width) / 2.0).max(0.0);
    f.font(10.0, false);
    let mut tip = None;
    for (row, hours) in grid.iter().enumerate().take(7) {
        let y = y0 + row as f64 * cell;
        f.ink(f.pal.dim, 1.0);
        f.text(
            waybill_core::clock::WEEKDAYS[row],
            x0 - 8.0,
            y + cell / 2.0 + 3.5,
            Align::Right,
        );
        for (hour, &v) in hours.iter().enumerate().take(24) {
            let x = x0 + hour as f64 * cell;
            // The square root keeps a quiet hour visible next to a loud one.
            let heat = (v as f64 / max).sqrt() * f.grow;
            f.round_rect(
                x + gap / 2.0,
                y + gap / 2.0,
                cell - gap,
                cell - gap,
                cell * 0.22,
            );
            if v == 0 {
                f.ink(f.pal.fg, 0.045);
            } else {
                let c = f.pal.accent.mix(f.pal.bright, (heat - 0.75).max(0.0));
                f.ink(c, 0.14 + 0.86 * heat);
            }
            f.fill();
            if let Some((hx, hy)) = f.hover
                && hx >= x
                && hx < x + cell
                && hy >= y
                && hy < y + cell
            {
                f.round_rect(x + 0.5, y + 0.5, cell - 1.0, cell - 1.0, cell * 0.24);
                f.ink(f.pal.bright, 0.9);
                f.cr.set_line_width(1.2);
                f.stroke();
                tip = Some((x + cell, y + cell / 2.0, row, hour, v));
            }
        }
    }
    f.ink(f.pal.dim, 1.0);
    for hour in [0, 6, 12, 18, 23] {
        f.text(
            &format!("{hour:02}"),
            x0 + (hour as f64 + 0.5) * cell,
            y0 + 7.0 * cell + 15.0,
            Align::Center,
        );
    }
    if let Some((x, y, row, hour, v)) = tip {
        f.tooltip(
            x,
            y,
            &[
                (
                    format!("{} {hour:02}:00", waybill_core::clock::WEEKDAYS[row]),
                    f.pal.bright,
                ),
                (format!("{v} visits in 4 weeks"), f.pal.accent),
            ],
        );
    }
}

/// A ring cut into shares, with a number in the middle.
pub struct Ring {
    pub parts: Vec<(f64, Rgba)>,
    pub big: String,
    pub small: String,
}

pub fn ring(f: &Frame, r: &Ring) {
    let (cx, cy) = (f.w / 2.0, f.h / 2.0);
    let radius = (f.w.min(f.h) / 2.0 - 8.0).max(10.0);
    let thick = (radius * 0.26).max(8.0);
    let total: f64 = r.parts.iter().map(|p| p.0).sum();
    f.cr.set_line_width(thick);
    f.cr.set_line_cap(cairo::LineCap::Butt);
    f.ink(f.pal.fg, 0.06);
    f.cr.arc(cx, cy, radius - thick / 2.0, 0.0, TAU);
    f.stroke();
    if total > 0.0 {
        let gap = 0.035;
        let mut at = -FRAC_PI_2;
        for (v, color) in &r.parts {
            let sweep = v / total * TAU * f.grow;
            if sweep > gap {
                f.ink(*color, 1.0);
                f.cr.arc(
                    cx,
                    cy,
                    radius - thick / 2.0,
                    at + gap / 2.0,
                    at + sweep - gap / 2.0,
                );
                f.stroke();
            }
            at += sweep;
        }
    }
    f.font(radius * 0.46, true);
    f.ink(f.pal.bright, 1.0);
    f.text(&r.big, cx, cy + radius * 0.10, Align::Center);
    f.font(10.5, false);
    f.ink(f.pal.dim, 1.0);
    f.text(&r.small, cx, cy + radius * 0.10 + 17.0, Align::Center);
}

/// How long tasks lived: one bar per bucket.
pub fn cycle(f: &Frame, buckets: &[Bucket]) {
    let n = buckets.len().max(1) as f64;
    let (x0, y0, h) = (8.0, 22.0, f.h - 46.0);
    let slot = (f.w - 16.0) / n;
    let bar = (slot * 0.62).min(64.0);
    let max = buckets.iter().map(|b| b.count).max().unwrap_or(0).max(1) as f64;
    let total: u32 = buckets.iter().map(|b| b.count).sum();
    for (i, b) in buckets.iter().enumerate() {
        let x = x0 + slot * i as f64 + (slot - bar) / 2.0;
        let bh = (b.count as f64 / max * h * f.grow).max(2.0);
        let y = y0 + h - bh;
        let hot = f
            .hover
            .is_some_and(|(hx, _)| hx >= x0 + slot * i as f64 && hx < x0 + slot * (i + 1) as f64);
        let g = cairo::LinearGradient::new(0.0, y, 0.0, y0 + h);
        let c = f.pal.teal;
        g.add_color_stop_rgba(0.0, c.0, c.1, c.2, if hot { 1.0 } else { 0.9 });
        g.add_color_stop_rgba(1.0, c.0, c.1, c.2, if hot { 0.5 } else { 0.25 });
        f.round_rect(x, y, bar, bh, 5.0);
        let _ = f.cr.set_source(&g);
        f.fill();
        f.font(11.0, true);
        f.ink(f.pal.bright, 1.0);
        let shown = if hot && total > 0 {
            format!("{}%", (b.count as f64 / total as f64 * 100.0).round())
        } else {
            b.count.to_string()
        };
        f.text(&shown, x + bar / 2.0, y - 6.0, Align::Center);
        f.font(10.0, false);
        f.ink(f.pal.dim, 1.0);
        f.text(&b.label, x + bar / 2.0, y0 + h + 16.0, Align::Center);
    }
    f.ink(f.pal.fg, 0.22);
    f.cr.set_line_width(1.0);
    f.cr.move_to(x0, (y0 + h).round() + 0.5);
    f.cr.line_to(f.w - 8.0, (y0 + h).round() + 0.5);
    f.stroke();
}

/// A small line with an area under it and a dot at its end, in `color`.
pub fn spark(f: &Frame, values: &[f32], color: Rgba) {
    if values.len() < 2 {
        return;
    }
    let max = values.iter().copied().fold(1.0f32, f32::max) as f64;
    let n = values.len();
    let h = f.h - 6.0;
    let pts: Vec<(f64, f64)> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = 3.0 + (f.w - 6.0) * i as f64 / (n - 1) as f64;
            (x, 3.0 + h - (*v as f64 / max) * h * f.grow)
        })
        .collect();
    f.cr.move_to(pts[0].0, f.h);
    smooth(f.cr, &pts);
    f.cr.line_to(pts[n - 1].0, f.h);
    f.cr.close_path();
    let g = cairo::LinearGradient::new(0.0, 0.0, 0.0, f.h);
    g.add_color_stop_rgba(0.0, color.0, color.1, color.2, 0.34);
    g.add_color_stop_rgba(1.0, color.0, color.1, color.2, 0.0);
    let _ = f.cr.set_source(&g);
    f.fill();
    line(f, &pts, color, 1.6);
    let (x, y) = pts[n - 1];
    f.ink(color, 1.0);
    f.cr.arc(x, y, 2.4, 0.0, TAU);
    f.fill();
}

/// Fourteen tiny bars: a card's visits per day.
pub fn activity(f: &Frame, days: &[u16]) {
    let n = days.len().max(1) as f64;
    let slot = f.w / n;
    let max = days.iter().copied().max().unwrap_or(0).max(1) as f64;
    for (i, &v) in days.iter().enumerate() {
        let h = if v == 0 {
            1.5
        } else {
            (v as f64 / max * f.h).max(3.0)
        };
        f.round_rect(
            i as f64 * slot + 0.5,
            f.h - h,
            (slot - 1.5).max(1.0),
            h,
            1.0,
        );
        f.ink(
            if v == 0 { f.pal.dim } else { f.pal.accent },
            if v == 0 { 0.35 } else { 0.9 },
        );
        f.fill();
    }
}

/// One thin bar split by share.
pub fn split(f: &Frame, parts: &[(f64, Rgba)]) {
    let total: f64 = parts.iter().map(|p| p.0).sum();
    f.round_rect(0.0, 0.0, f.w, f.h, f.h / 2.0);
    f.cr.clip();
    f.ink(f.pal.fg, 0.08);
    f.cr.rectangle(0.0, 0.0, f.w, f.h);
    f.fill();
    if total <= 0.0 {
        return;
    }
    let mut x = 0.0;
    for (v, color) in parts {
        let w = v / total * f.w * f.grow;
        f.ink(*color, 1.0);
        f.cr.rectangle(x, 0.0, (w - 1.5).max(0.0), f.h);
        f.fill();
        x += w;
    }
}

/// A week of visits by the hour, with a mark for each finished task.
pub struct Ribbon {
    /// One entry per hour, oldest first, ending with the current hour.
    pub visits: Vec<u32>,
    pub done: Vec<u32>,
    /// (hour index, label) where a day starts.
    pub days: Vec<(usize, String)>,
}

pub fn ribbon(f: &Frame, r: &Ribbon) {
    let n = r.visits.len();
    if n == 0 {
        return;
    }
    let (x0, y0, h) = (4.0, 16.0, f.h - 40.0);
    let slot = (f.w - 8.0) / n as f64;
    let max = r.visits.iter().copied().max().unwrap_or(0).max(1) as f64;
    f.font(10.0, false);
    for (at, label) in &r.days {
        let x = (x0 + *at as f64 * slot).round() + 0.5;
        f.ink(f.pal.fg, 0.12);
        f.cr.set_line_width(1.0);
        f.cr.move_to(x, 4.0);
        f.cr.line_to(x, y0 + h + 4.0);
        f.stroke();
        f.ink(f.pal.dim, 1.0);
        f.text(label, x + 5.0, y0 + h + 18.0, Align::Left);
    }
    let mut tip = None;
    for i in 0..n {
        let x = x0 + i as f64 * slot;
        let v = r.visits[i] as f64;
        let hot = f.hover.is_some_and(|(hx, _)| hx >= x && hx < x + slot);
        if v > 0.0 {
            let bh = (v / max * h * f.grow).max(2.0);
            f.round_rect(x + 0.5, y0 + h - bh, (slot - 1.0).max(1.0), bh, 1.5);
            f.ink(
                if hot { f.pal.bright } else { f.pal.accent },
                0.35 + 0.65 * (v / max),
            );
            f.fill();
        }
        if r.done.get(i).copied().unwrap_or(0) > 0 {
            f.ink(f.pal.ok, 1.0);
            f.cr.arc(x + slot / 2.0, 9.0, (slot / 2.6).clamp(1.5, 3.0), 0.0, TAU);
            f.fill();
        }
        if hot {
            tip = Some((x, i));
        }
    }
    f.ink(f.pal.fg, 0.22);
    f.cr.move_to(x0, (y0 + h).round() + 0.5);
    f.cr.line_to(f.w - 4.0, (y0 + h).round() + 0.5);
    f.stroke();
    if let Some((x, i)) = tip {
        let back = n - 1 - i;
        let ago = match back {
            0 => "this hour".to_string(),
            1..24 => format!("{back} h ago"),
            _ => format!("{} d {} h ago", back / 24, back % 24),
        };
        f.tooltip(
            x,
            f.h / 2.0,
            &[
                (ago, f.pal.bright),
                (format!("{} visits", r.visits[i]), f.pal.accent),
                (
                    format!("{} finished", r.done.get(i).copied().unwrap_or(0)),
                    f.pal.ok,
                ),
            ],
        );
    }
}

/// A rail line and the stops the pointer can reach, filled in as it draws.
pub struct Rail {
    pub line: Line,
    pub hits: RefCell<Vec<(f64, f64, String)>>,
}

pub const RAIL_DX: f64 = 176.0;
pub const RAIL_DY: f64 = 86.0;
const RAIL_X0: f64 = 96.0;
const RAIL_Y0: f64 = 30.0;

pub fn rail_size(line: &Line) -> (i32, i32) {
    (
        (RAIL_X0 * 2.0 + (line.cols.max(1) - 1) as f64 * RAIL_DX) as i32,
        (RAIL_Y0 + 52.0 + (line.tracks.max(1) - 1) as f64 * RAIL_DY) as i32,
    )
}

/// The stop under the pointer, if any.
pub fn rail_hit(rail: &Rail, x: f64, y: f64) -> Option<String> {
    rail.hits
        .borrow()
        .iter()
        .find(|(sx, sy, _)| (sx - x).abs() < 80.0 && y > sy - 18.0 && y < sy + 48.0)
        .map(|(_, _, uuid)| uuid.clone())
}

pub fn rail(f: &Frame, r: &Rail) {
    let line = &r.line;
    let color = f.pal.project(&line.project);
    let at = |i: usize| {
        let s = &line.stops[i];
        (
            RAIL_X0 + s.col as f64 * RAIL_DX,
            RAIL_Y0 + s.track as f64 * RAIL_DY,
        )
    };
    let hot = f.hover.and_then(|(x, y)| rail_hit(r, x, y));
    f.cr.set_line_cap(cairo::LineCap::Round);
    // Track first, travelled track over it.
    for pass in 0..2 {
        for &(a, b) in &line.links {
            let travelled = line.stops[a].state == StopState::Done;
            if (pass == 1) != travelled {
                continue;
            }
            let ((x1, y1), (x2, y2)) = (at(a), at(b));
            let reach = if travelled { f.grow } else { 1.0 };
            f.cr.move_to(x1, y1);
            if (y1 - y2).abs() < 0.5 {
                f.cr.line_to(x1 + (x2 - x1) * reach, y2);
            } else {
                // Leave along the track, swing over, arrive along the track.
                let bend = (x2 - x1).min(RAIL_DX) * 0.5;
                f.cr.line_to(x2 - bend * 2.0 + bend * 0.4, y1);
                f.cr.curve_to(
                    x2 - bend * 0.7,
                    y1,
                    x2 - bend * 0.9,
                    y2,
                    x2 - bend * 0.2,
                    y2,
                );
                f.cr.line_to(x2, y2);
            }
            f.cr.set_line_width(7.0);
            f.ink(
                if travelled {
                    color
                } else {
                    color.mix(f.pal.tile, 0.66)
                },
                1.0,
            );
            f.stroke();
        }
    }
    let mut hits = Vec::with_capacity(line.stops.len());
    for (i, s) in line.stops.iter().enumerate() {
        let (x, y) = at(i);
        hits.push((x, y, s.uuid.clone()));
        let is_hot = hot.as_deref() == Some(s.uuid.as_str());
        let live = matches!(s.state, StopState::Next | StopState::Doing);
        let mark = match (s.state, s.yours) {
            (StopState::Doing, _) => f.pal.ok,
            (_, true) => f.pal.warn,
            (StopState::Done, _) => color,
            (StopState::Next, _) => f.pal.bright,
            _ => f.pal.dim,
        };
        if live || is_hot {
            let pulse = if live {
                (f.beat * 2.4).sin() * 0.5 + 0.5
            } else {
                1.0
            };
            f.ink(mark, 0.10 + 0.14 * pulse);
            f.cr.arc(x, y, 15.0 + 3.0 * pulse, 0.0, TAU);
            f.fill();
        }
        let shape = |size: f64| {
            if s.yours {
                f.round_rect(x - size, y - size, size * 2.0, size * 2.0, 3.5);
            } else {
                f.cr.new_sub_path();
                f.cr.arc(x, y, size, 0.0, TAU);
            }
        };
        shape(10.0);
        f.ink(f.pal.panel, 1.0);
        f.fill();
        shape(8.0);
        let solid = matches!(s.state, StopState::Done | StopState::Doing)
            || (s.yours && s.state == StopState::Next);
        if solid {
            f.ink(mark, 1.0);
            f.fill();
        } else {
            f.ink(f.pal.well, 1.0);
            let _ = f.cr.fill_preserve();
            f.ink(mark, 1.0);
            f.cr.set_line_width(2.6);
            if s.state == StopState::Parked {
                f.cr.set_dash(&[3.0, 3.0], 0.0);
            }
            f.stroke();
            f.cr.set_dash(&[], 0.0);
        }
        if s.state == StopState::Done {
            // A tick.
            f.ink(f.pal.panel, 1.0);
            f.cr.set_line_width(2.2);
            f.cr.move_to(x - 3.6, y + 0.2);
            f.cr.line_to(x - 1.0, y + 2.8);
            f.cr.line_to(x + 3.8, y - 2.8);
            f.stroke();
        }
        f.font(11.0, live || is_hot);
        f.ink(
            match s.state {
                StopState::Done => f.pal.dim,
                _ if live || is_hot => f.pal.bright,
                _ => f.pal.fg,
            },
            1.0,
        );
        for (k, text) in wrap(&s.title, 22, 2).iter().enumerate() {
            f.text(text, x, y + 28.0 + k as f64 * 14.0, Align::Center);
        }
    }
    *r.hits.borrow_mut() = hits;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_tops_are_round() {
        assert_eq!(nice(0.0), 4.0);
        assert_eq!(nice(7.0), 8.0);
        assert_eq!(nice(11.0), 12.0);
        assert_eq!(nice(23.0), 40.0);
        assert_eq!(nice(45.0), 60.0);
        assert_eq!(nice(83.0), 100.0);
        assert_eq!(nice(197.0), 200.0);
        assert_eq!(nice(309.0), 400.0);
        assert_eq!(nice(1100.0), 1200.0);
    }
}
