//! Dependency chains drawn as rail lines. Every group of open tasks tied
//! together by `depends` becomes a line: each task is a stop, placed in a
//! column by how many stops must close before it and on a track of its own
//! where the line branches.

use crate::clock::DAY;
use crate::model::Rules;
use crate::task::{Status, Task};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StopState {
    Done,
    Doing,
    /// Nothing open stands before it.
    Next,
    Waiting,
    Parked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    pub uuid: String,
    /// The title, without the line's name when every stop repeats it.
    pub title: String,
    pub state: StopState,
    pub yours: bool,
    pub col: u32,
    pub track: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub name: String,
    pub project: String,
    /// Ordered by column, then track.
    pub stops: Vec<Stop>,
    /// (from, to) indexes into `stops`: `to` waits on `from`.
    pub links: Vec<(usize, usize)>,
    pub cols: u32,
    pub tracks: u32,
    pub open: u32,
}

/// A finished stop stays on its line this long.
const KEEP_DONE: i64 = 14 * DAY;

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

pub fn build(tasks: &[Task], rules: &Rules, now: i64) -> Vec<Line> {
    let index: HashMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.uuid.as_str(), i))
        .collect();
    let on_a_line = |t: &Task| {
        t.open() || (t.status == Status::Completed && t.end.is_some_and(|e| e >= now - KEEP_DONE))
    };
    // Edges run from a dependency to the task that waits on it.
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut parent: Vec<usize> = (0..tasks.len()).collect();
    for (i, t) in tasks.iter().enumerate().filter(|(_, t)| t.open()) {
        for d in &t.depends {
            let Some(&j) = index.get(d.as_str()) else {
                continue;
            };
            if on_a_line(&tasks[j]) && i != j {
                edges.push((j, i));
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                parent[a] = b;
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(a, b) in &edges {
        for n in [a, b] {
            let root = find(&mut parent, n);
            let g = groups.entry(root).or_default();
            if !g.contains(&n) {
                g.push(n);
            }
        }
    }
    let mut lines: Vec<Line> = groups
        .into_values()
        .map(|nodes| line(tasks, &nodes, &edges, rules, now))
        .collect();
    lines.sort_by(|a, b| b.open.cmp(&a.open).then(a.name.cmp(&b.name)));
    lines
}

fn line(
    tasks: &[Task],
    nodes: &[usize],
    edges: &[(usize, usize)],
    rules: &Rules,
    now: i64,
) -> Line {
    let mut nodes = nodes.to_vec();
    nodes.sort_by_key(|&n| (tasks[n].entry, n));
    let local: HashMap<usize, usize> = nodes.iter().enumerate().map(|(i, &n)| (n, i)).collect();
    let n = nodes.len();
    let mut children: Vec<Vec<usize>> = vec![vec![]; n];
    let mut parents: Vec<Vec<usize>> = vec![vec![]; n];
    for &(a, b) in edges {
        if let (Some(&a), Some(&b)) = (local.get(&a), local.get(&b))
            && !children[a].contains(&b)
        {
            children[a].push(b);
            parents[b].push(a);
        }
    }
    // Column: the longest run of stops before this one. `depends` cannot
    // loop, but a bound keeps a damaged database from hanging the board.
    let mut col = vec![0u32; n];
    for _ in 0..n {
        let mut moved = false;
        for i in 0..n {
            let want = parents[i].iter().map(|&p| col[p] + 1).max().unwrap_or(0);
            if want > col[i] {
                col[i] = want;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    // Height: the longest run after it. The tallest child continues its
    // parent's track; every other branch takes a new one.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(col[i]));
    let mut height = vec![1u32; n];
    for &i in &order {
        height[i] = 1 + children[i].iter().map(|&c| height[c]).max().unwrap_or(0);
    }
    let mut track: Vec<Option<u32>> = vec![None; n];
    let mut next = 0u32;
    let mut roots: Vec<usize> = (0..n).filter(|&i| parents[i].is_empty()).collect();
    roots.sort_by_key(|&i| (std::cmp::Reverse(height[i]), i));
    for root in roots {
        let mut stack = vec![(root, next)];
        next += 1;
        while let Some((i, t)) = stack.pop() {
            if track[i].is_some() {
                continue;
            }
            track[i] = Some(t);
            let mut kids: Vec<usize> = children[i]
                .iter()
                .copied()
                .filter(|&c| track[c].is_none())
                .collect();
            kids.sort_by_key(|&c| (std::cmp::Reverse(height[c]), c));
            // Pushed in reverse so the tallest is laid first.
            let mut plan = Vec::new();
            for (k, c) in kids.into_iter().enumerate() {
                if k == 0 {
                    plan.push((c, t));
                } else {
                    plan.push((c, next));
                    next += 1;
                }
            }
            stack.extend(plan.into_iter().rev());
        }
    }

    let (name, strip) = name(tasks, &nodes, &children, &parents);
    let mut stops: Vec<(usize, Stop)> = (0..n)
        .map(|i| {
            let t = &tasks[nodes[i]];
            let state = if !t.open() {
                StopState::Done
            } else if t.start.is_some() {
                StopState::Doing
            } else if t.status == Status::Waiting || t.wait.is_some_and(|w| w > now) {
                StopState::Parked
            } else if parents[i].iter().all(|&p| !tasks[nodes[p]].open()) {
                StopState::Next
            } else {
                StopState::Waiting
            };
            let title = strip
                .as_deref()
                .and_then(|p| t.description.strip_prefix(p))
                .unwrap_or(&t.description)
                .trim()
                .to_string();
            let stop = Stop {
                uuid: t.uuid.clone(),
                title,
                state,
                yours: rules.yours(t),
                col: col[i],
                track: track[i].unwrap_or(0),
            };
            (i, stop)
        })
        .collect();
    stops.sort_by_key(|(_, s)| (s.col, s.track));
    let place: HashMap<usize, usize> = stops
        .iter()
        .enumerate()
        .map(|(at, (i, _))| (*i, at))
        .collect();
    let mut links: Vec<(usize, usize)> = (0..n)
        .flat_map(|a| children[a].iter().map(move |&b| (a, b)))
        .map(|(a, b)| (place[&a], place[&b]))
        .collect();
    links.sort();
    let stops: Vec<Stop> = stops.into_iter().map(|(_, s)| s).collect();
    let mut projects: Vec<&str> = nodes
        .iter()
        .filter_map(|&i| tasks[i].project.as_deref())
        .collect();
    projects.sort();
    projects.dedup();
    Line {
        name,
        project: projects.join(", "),
        cols: stops.iter().map(|s| s.col + 1).max().unwrap_or(0),
        tracks: stops.iter().map(|s| s.track + 1).max().unwrap_or(0),
        open: stops.iter().filter(|s| s.state != StopState::Done).count() as u32,
        stops,
        links,
    }
}

/// A line's name and the title prefix its stops share. "Netplay: handshake"
/// and "Netplay: clock sync" make the Netplay line; a line with no shared
/// prefix is named for where it ends, or where it starts.
fn name(
    tasks: &[Task],
    nodes: &[usize],
    children: &[Vec<usize>],
    parents: &[Vec<usize>],
) -> (String, Option<String>) {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for &i in nodes {
        if let Some((prefix, _)) = tasks[i].description.split_once(": ") {
            *counts.entry(prefix).or_default() += 1;
        }
    }
    let best = counts
        .into_iter()
        .max_by_key(|&(p, c)| (c, std::cmp::Reverse(p)));
    if let Some((prefix, count)) = best
        && count >= 2
        && count * 2 >= nodes.len()
    {
        return (prefix.to_string(), Some(format!("{prefix}: ")));
    }
    let ends: Vec<usize> = (0..nodes.len())
        .filter(|&i| children[i].is_empty())
        .collect();
    let starts: Vec<usize> = (0..nodes.len())
        .filter(|&i| parents[i].is_empty())
        .collect();
    let title = |i: usize| tasks[nodes[i]].description.clone();
    match (ends.as_slice(), starts.as_slice()) {
        ([end], _) => (format!("To: {}", title(*end)), None),
        (_, [start]) => (format!("From: {}", title(*start)), None),
        _ => (title(ends.first().copied().unwrap_or(0)), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::{NOW, task};

    fn chain() -> Vec<Task> {
        let mut state = task("state", "Netplay: car state");
        state.status = Status::Completed;
        state.end = Some(NOW - 3600);
        let mut shake = task("shake", "Netplay: handshake");
        shake.depends = vec!["state".into()];
        let mut clock = task("clock", "Netplay: clock sync");
        clock.depends = vec!["shake".into()];
        let mut snaps = task("snaps", "Netplay: snapshots");
        snaps.depends = vec!["clock".into()];
        let mut combat = task("combat", "Send combat over netplay");
        combat.depends = vec!["snaps".into()];
        let mut test = task("test", "Netplay: loopback playtest");
        test.depends = vec!["snaps".into()];
        test.tags = vec!["me".into()];
        let mut lobby = task("lobby", "Netplay: lobby");
        lobby.depends = vec!["test".into()];
        vec![
            state,
            shake,
            clock,
            snaps,
            combat,
            test,
            lobby,
            task("alone", "Fix a typo"),
        ]
    }

    #[test]
    fn a_chain_becomes_one_line() {
        let lines = build(&chain(), &Rules::default(), NOW);
        assert_eq!(lines.len(), 1, "a task with no ties is on no line");
        let l = &lines[0];
        assert_eq!(l.name, "Netplay");
        assert_eq!((l.cols, l.tracks, l.open), (6, 2, 6));
        let stop = |u: &str| l.stops.iter().find(|s| s.uuid == u).unwrap();
        assert_eq!(stop("state").state, StopState::Done);
        assert_eq!(stop("shake").state, StopState::Next);
        assert_eq!(stop("shake").title, "handshake");
        assert_eq!(stop("clock").state, StopState::Waiting);
        assert!(stop("test").yours);
        // The trunk runs straight through the longer branch.
        assert_eq!(stop("lobby").track, 0);
        assert_eq!((stop("combat").col, stop("combat").track), (4, 1));
        assert_eq!(stop("combat").title, "Send combat over netplay");
        assert_eq!(l.links.len(), 6);
    }

    #[test]
    fn a_fan_is_named_for_its_start() {
        let mut play = task("play", "Playtest the workbench");
        play.tags = vec!["me".into()];
        let mut a = task("a", "Build the magnet");
        a.depends = vec!["play".into()];
        let mut b = task("b", "Build the piston");
        b.depends = vec!["play".into()];
        let lines = build(&[play, a, b], &Rules::default(), NOW);
        assert_eq!(lines[0].name, "From: Playtest the workbench");
        assert_eq!((lines[0].cols, lines[0].tracks), (2, 2));
    }

    #[test]
    fn old_finished_work_is_not_a_stop() {
        let mut old = task("old", "Draw the map");
        old.status = Status::Completed;
        old.end = Some(NOW - 30 * DAY);
        let mut next = task("next", "Build floods");
        next.depends = vec!["old".into()];
        assert!(build(&[old, next], &Rules::default(), NOW).is_empty());
    }
}
