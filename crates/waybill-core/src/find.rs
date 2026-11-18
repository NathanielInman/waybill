//! Searching the list before adding to it. Every word must appear somewhere
//! in a task (title, ref, project, visits or note), case aside, so a search
//! narrows as words are added.

use crate::model::{Lane, Snapshot};
use crate::task::{Status, Task};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Found {
    pub uuid: String,
    pub title: String,
    pub project: String,
    /// `open` or `done`.
    pub status: &'static str,
    /// The board lane, for open and recently closed tasks.
    pub lane: Option<Lane>,
    /// When a closed task closed.
    pub end: Option<i64>,
    /// Where the words turned up: `title`, `ref`, `project`, `visit`, `note`.
    pub matched: Vec<&'static str>,
}

/// Whether `project` is `filter` or sits under it (`plos` takes `plos.db`).
pub fn in_family(project: &str, filter: &str) -> bool {
    project == filter
        || project
            .strip_prefix(filter)
            .is_some_and(|r| r.starts_with('.'))
}

/// Open tasks first, most urgent first; then (with `closed`) finished ones,
/// latest first. Deleted tasks never match. `note` gives a task's note text.
pub fn search(
    tasks: &[Task],
    snap: &Snapshot,
    note: impl Fn(&str) -> Option<String>,
    words: &[String],
    project: Option<&str>,
    closed: bool,
) -> Vec<Found> {
    let words: Vec<String> = words
        .iter()
        .flat_map(|w| w.split_whitespace())
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return Vec::new();
    }
    let lanes: HashMap<&str, Lane> = snap
        .cards
        .iter()
        .map(|c| (c.uuid.as_str(), c.lane))
        .collect();
    let mut hits: Vec<(&Task, Found)> = Vec::new();
    for t in tasks {
        let done = t.status == Status::Completed;
        if !(t.open() || (closed && done)) {
            continue;
        }
        let proj = t.project.as_deref().unwrap_or("");
        if project.is_some_and(|f| !in_family(proj, f)) {
            continue;
        }
        let visits: Vec<String> = t
            .annotations
            .iter()
            .map(|a| a.description.to_lowercase())
            .collect();
        let note = note(&t.uuid).map(|n| n.to_lowercase());
        let fields: [(&'static str, Vec<String>); 5] = [
            ("title", vec![t.description.to_lowercase()]),
            (
                "ref",
                t.reference.iter().map(|r| r.to_lowercase()).collect(),
            ),
            ("project", vec![proj.to_lowercase()]),
            ("visit", visits),
            ("note", note.into_iter().collect()),
        ];
        let has = |w: &str, texts: &[String]| texts.iter().any(|x| x.contains(w));
        if !words.iter().all(|w| fields.iter().any(|(_, f)| has(w, f))) {
            continue;
        }
        let matched = fields
            .iter()
            .filter(|(_, f)| words.iter().any(|w| has(w, f)))
            .map(|(name, _)| *name)
            .collect();
        hits.push((
            t,
            Found {
                uuid: t.uuid.clone(),
                title: t.description.clone(),
                project: proj.to_string(),
                status: if done { "done" } else { "open" },
                lane: lanes.get(t.uuid.as_str()).copied(),
                end: if done { t.end } else { None },
                matched,
            },
        ));
    }
    hits.sort_by(|(a, _), (b, _)| {
        b.open()
            .cmp(&a.open())
            .then(b.urgency.total_cmp(&a.urgency))
            .then(b.end.cmp(&a.end))
    });
    hits.into_iter().map(|(_, f)| f).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::task;

    #[test]
    fn every_word_must_land_somewhere() {
        let mut cart = task("cart", "Wire the cart total");
        cart.project = Some("plos.db".into());
        cart.reference = Some("checkout".into());
        cart.urgency = 2.0;
        let mut loot = task("loot", "Roll loot tables");
        loot.project = Some("etb".into());
        loot.urgency = 9.0;
        let mut old = task("old", "Wire the old cart");
        old.status = Status::Completed;
        old.end = Some(5);
        let mut gone = task("gone", "Wire a deleted cart");
        gone.status = Status::Deleted;
        let tasks = [cart, loot, old, gone];
        let snap = Snapshot::failed(0, crate::clock::Zone::Fixed(0), String::new());
        let note = |u: &str| (u == "loot").then(|| "The CART drop table".to_string());
        let w = |s: &str| vec![s.to_string()];

        let found = search(&tasks, &snap, note, &w("cart"), None, false);
        let uuids: Vec<&str> = found.iter().map(|f| f.uuid.as_str()).collect();
        // The more urgent open task first; closed and deleted ones left out.
        assert_eq!(uuids, ["loot", "cart"]);
        assert_eq!(found[0].matched, ["note"]);

        let found = search(&tasks, &snap, note, &w("wire CHECKOUT"), None, false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].matched, ["title", "ref"]);

        let found = search(&tasks, &snap, note, &w("wire cart"), None, true);
        let uuids: Vec<&str> = found.iter().map(|f| f.uuid.as_str()).collect();
        assert_eq!(uuids, ["cart", "old"]);
        assert_eq!(found[1].status, "done");

        assert!(search(&tasks, &snap, note, &w("cart"), Some("plos"), false).len() == 1);
        assert!(search(&tasks, &snap, note, &w("cart"), Some("plo"), false).is_empty());
        assert!(search(&tasks, &snap, note, &[], None, true).is_empty());
    }
}
