//! The numbers behind the charts: what was added, visited and finished each
//! day, when the work happens, how long tasks live and who they wait on.

use crate::clock::{DAY, HOUR, Zone, day_label, weekday};
use crate::model::{Card, Lane, Rules};
use crate::task::{Status, Task};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// How many days the charts cover.
pub const SPAN: usize = 42;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DayStat {
    pub label: String,
    /// 0 is Monday.
    pub weekday: usize,
    pub done: u32,
    pub added: u32,
    pub visits: u32,
    /// Open at the end of the day, split by who it waited on.
    pub open_yours: u32,
    pub open_agents: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProjectStat {
    pub name: String,
    pub open: u32,
    pub yours: u32,
    pub doing: u32,
    pub done_week: u32,
    pub done_span: u32,
    /// Finished per day over the last two weeks, oldest first.
    pub spark: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Bucket {
    pub label: String,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct LaneCounts {
    pub ready: u32,
    pub doing: u32,
    pub yours: u32,
    pub held: u32,
    pub parked: u32,
    pub done: u32,
}

impl LaneCounts {
    pub fn open(&self) -> u32 {
        self.ready + self.doing + self.yours + self.held + self.parked
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Pulse {
    /// One entry per day, oldest first, ending today.
    pub days: Vec<DayStat>,
    /// Visits by weekday (Monday first) and hour, over the last four weeks.
    pub punch: Vec<Vec<u32>>,
    pub projects: Vec<ProjectStat>,
    /// How long finished tasks lived, from added to done.
    pub cycle: Vec<Bucket>,
    pub lanes: LaneCounts,
    pub done_week: u32,
    pub added_week: u32,
    pub visits_week: u32,
    /// The share of visits that name a commit, 0 to 1.
    pub commit_share: f32,
    pub decisions_open: u32,
    /// Hours a decision stayed open, at the median.
    pub decision_hours: Option<f32>,
    /// How many of the user's tasks hold others up, and how many they hold.
    pub holding: u32,
    pub held_up: u32,
}

const CYCLE: [(&str, i64); 6] = [
    ("under 1 h", HOUR),
    ("1 to 4 h", 4 * HOUR),
    ("4 to 24 h", DAY),
    ("1 to 3 d", 3 * DAY),
    ("3 to 7 d", 7 * DAY),
    ("over 7 d", i64::MAX),
];

pub fn build(tasks: &[Task], cards: &[Card], rules: &Rules, now: i64, zone: Zone) -> Pulse {
    let today = zone.day(now);
    let first = today - SPAN as i64 + 1;
    let slot = |ts: i64| -> Option<usize> {
        let d = zone.day(ts) - first;
        (0..SPAN as i64).contains(&d).then_some(d as usize)
    };
    let mut days: Vec<DayStat> = (0..SPAN as i64)
        .map(|i| DayStat {
            label: day_label(first + i),
            weekday: weekday(first + i),
            ..Default::default()
        })
        .collect();
    let mut punch = vec![vec![0u32; 24]; 7];
    let mut cycle: Vec<Bucket> = CYCLE
        .iter()
        .map(|(label, _)| Bucket {
            label: (*label).into(),
            count: 0,
        })
        .collect();
    let mut projects: BTreeMap<String, ProjectStat> = BTreeMap::new();
    let mut decision_hours: Vec<f32> = Vec::new();
    let (mut visits_span, mut visits_with_commit) = (0u32, 0u32);
    let week = now - 7 * DAY;
    let (mut done_week, mut added_week, mut visits_week) = (0, 0, 0);

    for t in tasks {
        if t.status == Status::Recurring {
            continue;
        }
        let name = t.project.clone().unwrap_or_else(|| "(none)".into());
        if let Some(i) = slot(t.entry) {
            days[i].added += 1;
        }
        added_week += u32::from(t.entry >= week);
        for a in &t.annotations {
            if let Some(i) = slot(a.entry) {
                days[i].visits += 1;
                visits_span += 1;
                visits_with_commit += u32::from(!crate::model::shas(&a.description).is_empty());
            }
            visits_week += u32::from(a.entry >= week);
            if a.entry >= now - 28 * DAY {
                punch[weekday(zone.day(a.entry))][zone.hour(a.entry)] += 1;
            }
        }
        if t.open() {
            let p = projects.entry(name.clone()).or_default();
            p.open += 1;
            p.yours += u32::from(rules.yours(t));
            p.doing += u32::from(t.start.is_some());
        }
        if let (Status::Completed, Some(end)) = (t.status, t.end) {
            if let Some(i) = slot(end) {
                days[i].done += 1;
                let p = projects.entry(name).or_default();
                p.done_span += 1;
                p.done_week += u32::from(end >= week);
                if p.spark.is_empty() {
                    p.spark = vec![0; 14];
                }
                let back = today - zone.day(end);
                if (0..14).contains(&back) {
                    p.spark[13 - back as usize] += 1;
                }
                let lived = end - t.entry;
                if let Some(b) = CYCLE.iter().position(|(_, limit)| lived < *limit) {
                    cycle[b].count += 1;
                }
                if t.has_tag(&rules.decision) {
                    decision_hours.push(lived as f32 / HOUR as f32);
                }
            }
            done_week += u32::from(end >= week);
        }
    }

    // Open work at the end of each day. A deleted task was open until it was
    // deleted.
    for (i, day) in days.iter_mut().enumerate() {
        let end = zone.day_start(first + i as i64 + 1).min(now);
        for t in tasks.iter().filter(|t| t.status != Status::Recurring) {
            if t.entry <= end && t.end.is_none_or(|e| e > end) {
                if rules.yours(t) {
                    day.open_yours += 1;
                } else {
                    day.open_agents += 1;
                }
            }
        }
    }

    let mut lanes = LaneCounts::default();
    let mut held_up: HashSet<&str> = HashSet::new();
    let mut holding = 0;
    for c in cards {
        match c.lane {
            Lane::Ready => lanes.ready += 1,
            Lane::Doing => lanes.doing += 1,
            Lane::Yours => lanes.yours += 1,
            Lane::Held => lanes.held += 1,
            Lane::Parked => lanes.parked += 1,
            Lane::Done => lanes.done += 1,
            Lane::Past => {}
        }
        if c.yours && c.ended.is_none() && !c.frees.is_empty() {
            holding += 1;
            held_up.extend(c.frees.iter().map(String::as_str));
        }
    }

    let mut projects: Vec<ProjectStat> = projects
        .into_iter()
        .map(|(name, mut p)| {
            p.name = name;
            if p.spark.is_empty() {
                p.spark = vec![0; 14];
            }
            p
        })
        .filter(|p| p.open > 0 || p.done_span > 0)
        .collect();
    projects.sort_by(|a, b| {
        (b.done_week + b.open, b.done_span)
            .cmp(&(a.done_week + a.open, a.done_span))
            .then(a.name.cmp(&b.name))
    });

    decision_hours.sort_by(f32::total_cmp);
    Pulse {
        days,
        punch,
        projects,
        cycle,
        lanes,
        done_week,
        added_week,
        visits_week,
        commit_share: if visits_span == 0 {
            0.0
        } else {
            visits_with_commit as f32 / visits_span as f32
        },
        decisions_open: cards
            .iter()
            .filter(|c| c.decision && c.ended.is_none())
            .count() as u32,
        decision_hours: (!decision_hours.is_empty())
            .then(|| decision_hours[decision_hours.len() / 2]),
        holding,
        held_up: held_up.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::{NOW, task};
    use crate::task::Annotation;

    #[test]
    fn counts_days_in_the_users_zone() {
        let chicago = Zone::Fixed(-6 * HOUR);
        // Finished at 03:00 UTC on the 18th: the evening of the 17th.
        let mut late = task("late", "Late");
        late.status = Status::Completed;
        late.entry = NOW - 5 * DAY;
        late.end = Some(NOW - 12 * HOUR);
        late.annotations = vec![Annotation {
            entry: NOW - 12 * HOUR,
            description: "done in a1b2c3d".into(),
        }];
        let mut mine = task("mine", "Playtest");
        mine.tags = vec!["me".into()];
        mine.entry = NOW - 2 * DAY;
        let snap = crate::model::build(
            &[late, mine, task("open", "Open")],
            &Default::default(),
            &Rules::default(),
            NOW,
            chicago,
        );
        let p = &snap.pulse;
        assert_eq!(p.days.len(), SPAN);
        let (today, yesterday) = (&p.days[SPAN - 1], &p.days[SPAN - 2]);
        assert_eq!((today.label.as_str(), today.weekday), ("Nov 18", 2));
        assert_eq!((yesterday.done, yesterday.visits, today.done), (1, 1, 0));
        assert_eq!((today.open_yours, today.open_agents), (1, 1));
        assert_eq!(
            p.days[SPAN - 3].open_agents,
            2,
            "the late task was still open on the 16th"
        );
        // 21:00 on a Tuesday.
        assert_eq!(p.punch[1][21], 1);
        assert_eq!(p.cycle[4].count, 1, "five days from added to done");
        assert_eq!(p.commit_share, 1.0);
        assert_eq!((p.lanes.yours, p.lanes.ready, p.lanes.open()), (1, 1, 2));
        assert_eq!(p.projects[0].name, "storefront");
        assert_eq!(
            (
                p.projects[0].open,
                p.projects[0].yours,
                p.projects[0].done_week
            ),
            (2, 1, 1)
        );
        assert_eq!(p.projects[0].spark[12], 1);
    }
}
