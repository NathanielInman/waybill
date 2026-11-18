//! The house rules for a tidy list: short titles, known tags, a project and
//! a priority on every open task.

use crate::config::Config;
use crate::task::Task;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub uuid: String,
    pub title: String,
    pub project: String,
    /// `title-long`, `title-period`, `tag-unknown`, `no-project`,
    /// `project-unknown` or `no-priority`.
    pub rule: &'static str,
    pub message: String,
}

/// What is wrong with a title on its own, as (rule, message).
pub fn title(title: &str, limit: usize) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let n = title.chars().count();
    if n > limit {
        out.push((
            "title-long",
            format!("the title is {n} characters; keep it to {limit} and put the rest in the note"),
        ));
    }
    if title.trim_end().ends_with('.') {
        out.push(("title-period", "the title ends with a period".to_string()));
    }
    out
}

fn known_project(project: &str, known: &[String]) -> bool {
    known.is_empty()
        || known.iter().any(|k| {
            project == k
                || project
                    .strip_prefix(k.as_str())
                    .is_some_and(|rest| rest.starts_with('.'))
        })
}

/// Checks every open task.
pub fn check(tasks: &[Task], cfg: &Config) -> Vec<Finding> {
    let mut out = Vec::new();
    for t in tasks.iter().filter(|t| t.open()) {
        let project = t.project.clone().unwrap_or_default();
        let mut add = |rule: &'static str, message: String| {
            out.push(Finding {
                uuid: t.uuid.clone(),
                title: t.description.clone(),
                project: project.clone(),
                rule,
                message,
            })
        };
        for (rule, message) in title(&t.description, cfg.lint.title) {
            add(rule, message);
        }
        for tag in &t.tags {
            let allowed = cfg.tags.hands.contains(tag)
                || *tag == cfg.tags.decision
                || cfg.lint.tags.contains(tag);
            if !allowed {
                add(
                    "tag-unknown",
                    format!("+{tag} is not one of the list's tags"),
                );
            }
        }
        if project.is_empty() {
            add("no-project", "the task has no project".into());
        } else if !known_project(&project, &cfg.lint.projects) {
            add(
                "project-unknown",
                format!("{project} is not a known project"),
            );
        }
        if t.priority.as_deref().is_none_or(str::is_empty) {
            add("no-priority", "the task has no priority".into());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::task;

    #[test]
    fn house_rules() {
        let mut cfg = Config::default();
        cfg.lint.projects = vec!["storefront".into(), "plos".into()];
        let fine = task("fine", "Wire the victory screen");
        let mut long = task(
            "long",
            "Build surface compound, hive warrens, hub and ring, shattered ground.",
        );
        long.tags = vec![
            "me".into(),
            "decision".into(),
            "bug".into(),
            "urgent".into(),
        ];
        long.project = Some("plos.db".into());
        let mut bare = task("bare", "Loose end");
        bare.project = None;
        bare.priority = None;
        let mut odd = task("odd", "Elsewhere");
        odd.project = Some("plosive".into());
        let mut closed = task(
            "closed",
            "A finished task may be as long as it likes, nobody reads it again.",
        );
        closed.status = crate::task::Status::Completed;
        let found = check(&[fine, long, bare, odd, closed], &cfg);
        let rules: Vec<(&str, &str)> = found.iter().map(|f| (f.uuid.as_str(), f.rule)).collect();
        assert_eq!(
            rules,
            [
                ("long", "title-long"),
                ("long", "title-period"),
                ("long", "tag-unknown"),
                ("bare", "no-project"),
                ("bare", "no-priority"),
                ("odd", "project-unknown"),
            ]
        );
        assert!(found[2].message.contains("+urgent"));
        assert!(title("Short and plain", 50).is_empty());
    }
}
