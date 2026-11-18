//! Pulse: the charts. What gets added and finished, who the open work
//! waits on, when the work happens, where it goes and how long it takes.

use crate::charts::{self, Canvas, Ring};
use crate::theme::{self, Rgba};
use crate::ui::{self, hbox, label, vbox};
use gtk::prelude::*;
use std::cell::RefCell;
use waybill_core::model::Snapshot;
use waybill_core::pulse::{Bucket, DayStat};

struct Stat {
    root: gtk::Box,
    value: gtk::Label,
    aside: gtk::Label,
    spark: charts::Spark,
}

fn stat(title: &str) -> Stat {
    let root = vbox(2);
    root.add_css_class("tile");
    root.set_hexpand(true);
    let top = hbox(6);
    top.append(&label(title, &["tile-title"]));
    top.append(&ui::spring());
    let aside = label("", &["dim", "small", "num"]);
    top.append(&aside);
    let value = label("0", &["stat", "num"]);
    let spark = Canvas::new(
        0,
        34,
        (vec![], theme::current().accent),
        |f, d: &(Vec<f32>, Rgba)| charts::spark(f, &d.0, d.1),
    );
    root.append(&top);
    root.append(&value);
    root.append(&spark.area);
    Stat {
        root,
        value,
        aside,
        spark,
    }
}

fn tile(title: &str, note: &str, chart: &impl IsA<gtk::Widget>) -> gtk::Box {
    let t = vbox(8);
    t.add_css_class("tile");
    t.set_hexpand(true);
    let head = hbox(8);
    head.append(&label(title, &["tile-title"]));
    head.append(&ui::spring());
    head.append(&label(note, &["dim", "small"]));
    t.append(&head);
    t.append(chart);
    t
}

pub struct PulseView {
    pub root: gtk::Box,
    stats: [Stat; 4],
    flow: Canvas<Vec<DayStat>>,
    open: Canvas<Vec<DayStat>>,
    punch: Canvas<Vec<Vec<u32>>>,
    cycle: Canvas<Vec<Bucket>>,
    ring: Canvas<Ring>,
    ring_legend: gtk::Box,
    projects: gtk::Box,
    sparks: RefCell<Vec<charts::Spark>>,
}

impl PulseView {
    pub fn new() -> PulseView {
        let root = vbox(0);
        root.add_css_class("pulse");
        let page = vbox(12);

        let stats = [
            stat("Finished today"),
            stat("Finished this week"),
            stat("Visits this week"),
            stat("Open now"),
        ];
        let row = hbox(12);
        row.set_homogeneous(true);
        for s in &stats {
            row.append(&s.root);
        }
        page.append(&row);

        let flow = Canvas::new(0, 280, Vec::new(), |f, d: &Vec<DayStat>| charts::flow(f, d));
        let ring = Canvas::new(
            0,
            190,
            Ring {
                parts: vec![],
                big: String::new(),
                small: String::new(),
            },
            charts::ring,
        );
        let ring_legend = vbox(4);
        let ring_box = vbox(10);
        ring_box.append(&ring.area);
        ring_box.append(&ring_legend);
        let row = hbox(12);
        let flow_tile = tile("Flow", "finished and added per day, six weeks", &flow.area);
        let ring_tile = tile("This week", "", &ring_box);
        ring_tile.set_hexpand(false);
        ring_tile.set_size_request(320, -1);
        row.append(&flow_tile);
        row.append(&ring_tile);
        page.append(&row);

        let open = Canvas::new(0, 240, Vec::new(), |f, d: &Vec<DayStat>| {
            charts::open_work(f, d)
        });
        let punch = Canvas::new(0, 240, Vec::new(), |f, d: &Vec<Vec<u32>>| {
            charts::punch(f, d)
        });
        let row = hbox(12);
        row.set_homogeneous(true);
        row.append(&tile("Open work", "at the end of each day", &open.area));
        row.append(&tile(
            "When the work happens",
            "visits by weekday and hour",
            &punch.area,
        ));
        page.append(&row);

        let projects = vbox(4);
        let cycle = Canvas::new(0, 230, Vec::new(), |f, d: &Vec<Bucket>| charts::cycle(f, d));
        let row = hbox(12);
        row.set_homogeneous(true);
        row.append(&tile(
            "Projects",
            "finished per day over two weeks",
            &projects,
        ));
        row.append(&tile(
            "How long a task lives",
            "from added to finished, six weeks",
            &cycle.area,
        ));
        page.append(&row);

        root.append(&ui::scrolled(&page));
        PulseView {
            root,
            stats,
            flow,
            open,
            punch,
            cycle,
            ring,
            ring_legend,
            projects,
            sparks: RefCell::new(vec![]),
        }
    }

    pub fn set(&self, s: &Snapshot) {
        let pal = theme::current();
        let p = &s.pulse;
        let last = |n: usize, f: &dyn Fn(&DayStat) -> u32| -> Vec<f32> {
            p.days
                .iter()
                .skip(p.days.len().saturating_sub(n))
                .map(|d| f(d) as f32)
                .collect()
        };
        let [today, week, visits, open] = &self.stats;
        today.value.set_label(&p.lanes.done.to_string());
        today.aside.set_label("14 days");
        today.spark.set((last(14, &|d| d.done), pal.ok));
        week.value.set_label(&p.done_week.to_string());
        week.aside.set_label(&format!("{} added", p.added_week));
        week.spark.set((last(28, &|d| d.done), pal.accent));
        visits.value.set_label(&p.visits_week.to_string());
        visits
            .aside
            .set_label(&format!("{:.0}% name a commit", p.commit_share * 100.0));
        visits.spark.set((last(28, &|d| d.visits), pal.teal));
        open.value.set_label(&p.lanes.open().to_string());
        open.aside.set_label(&format!("{} need you", p.lanes.yours));
        open.spark
            .set((last(42, &|d| d.open_yours + d.open_agents), pal.warn));

        self.flow.set(p.days.clone());
        self.open.set(p.days.clone());
        self.punch.set(p.punch.clone());
        self.cycle.set(p.cycle.clone());

        // Finished against added this week: are we gaining on the list?
        let parts = [
            ("finished", p.done_week, pal.accent),
            ("added", p.added_week, pal.warn),
        ];
        let net = p.done_week as i64 - p.added_week as i64;
        self.ring.set(Ring {
            parts: parts.iter().map(|(_, v, c)| (*v as f64, *c)).collect(),
            big: format!("{}{}", if net > 0 { "+" } else { "" }, net),
            small: if net >= 0 {
                "ahead of what came in".into()
            } else {
                "behind what came in".into()
            },
        });
        ui::clear(&self.ring_legend);
        for (name, v, color) in parts {
            let line = hbox(8);
            line.append(&ui::markup(
                &format!("<span foreground=\"{}\">●</span> {name}", color.hex()),
                &[],
            ));
            line.append(&ui::spring());
            line.append(&label(&v.to_string(), &["num", "bright"]));
            self.ring_legend.append(&line);
        }

        ui::clear(&self.projects);
        let mut sparks = Vec::new();
        let most = p
            .projects
            .iter()
            .map(|x| x.done_week)
            .max()
            .unwrap_or(0)
            .max(1) as f64;
        for project in p.projects.iter().take(9) {
            let color = pal.project(&project.name);
            let line = hbox(10);
            line.add_css_class("project-row");
            let name = ui::project(&project.name);
            name.set_size_request(150, -1);
            line.append(&name);
            let spark = Canvas::new(
                96,
                20,
                (project.spark.iter().map(|v| *v as f32).collect(), color),
                |f, d: &(Vec<f32>, Rgba)| charts::spark(f, &d.0, d.1),
            );
            spark.area.set_valign(gtk::Align::Center);
            line.append(&spark.area);
            let bar = Canvas::new(
                0,
                8,
                vec![
                    (project.done_week as f64, color),
                    (most - project.done_week as f64, pal.fg.with_alpha(0.0)),
                ],
                |f, parts: &Vec<(f64, Rgba)>| charts::split(f, parts),
            );
            bar.area.set_valign(gtk::Align::Center);
            line.append(&bar.area);
            let done = label(
                &format!("{} this week", project.done_week),
                &["num", "small"],
            );
            done.set_size_request(96, -1);
            done.set_xalign(1.0);
            line.append(&done);
            let mut open = format!("{} open", project.open);
            if project.yours > 0 {
                open.push_str(&format!(", {} yours", project.yours));
            }
            let open = label(&open, &["num", "small", "dim"]);
            open.set_size_request(128, -1);
            open.set_xalign(1.0);
            line.append(&open);
            self.projects.append(&line);
            sparks.push(spark);
        }
        *self.sparks.borrow_mut() = sparks;
    }

    pub fn play(&self) {
        for s in &self.stats {
            s.spark.play();
        }
        self.flow.play();
        self.open.play();
        self.punch.play();
        self.cycle.play();
        self.ring.play();
        for s in self.sparks.borrow().iter() {
            s.play();
        }
    }
}
