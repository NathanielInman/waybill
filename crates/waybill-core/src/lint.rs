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

/// What is wrong with one task's shape, as (rule, message): the title
/// rules, then tags, project and priority. `waybill lint` runs it over the
/// open list and `waybill add` over a task before it is filed.
pub fn shape(
    title_text: &str,
    project: &str,
    priority: Option<&str>,
    tags: &[String],
    cfg: &Config,
) -> Vec<(&'static str, String)> {
    let mut out = title(title_text, cfg.lint.title);
    for tag in tags {
        let allowed = cfg.tags.hands.contains(tag)
            || *tag == cfg.tags.decision
            || cfg.lint.tags.contains(tag);
        if !allowed {
            out.push((
                "tag-unknown",
                format!("+{tag} is not one of the list's tags"),
            ));
        }
    }
    if project.is_empty() {
        out.push(("no-project", "the task has no project".into()));
    } else if !known_project(project, &cfg.lint.projects) {
        out.push((
            "project-unknown",
            format!("{project} is not a known project"),
        ));
    }
    if priority.is_none_or(str::is_empty) {
        out.push(("no-priority", "the task has no priority".into()));
    }
    out
}

/// What is wrong with a task about to be added.
pub fn new_task(new: &crate::act::NewTask, cfg: &Config) -> Vec<(&'static str, String)> {
    let title = new.title.split_whitespace().collect::<Vec<_>>().join(" ");
    let priority = new.priority.map(|p| p.to_string());
    shape(
        &title,
        new.project.as_deref().unwrap_or("").trim(),
        priority.as_deref(),
        &new.tags,
        cfg,
    )
}

/// Checks every open task.
pub fn check(tasks: &[Task], cfg: &Config) -> Vec<Finding> {
    let mut out = Vec::new();
    for t in tasks.iter().filter(|t| t.open()) {
        let project = t.project.clone().unwrap_or_default();
        for (rule, message) in shape(
            &t.description,
            &project,
            t.priority.as_deref(),
            &t.tags,
            cfg,
        ) {
            out.push(Finding {
                uuid: t.uuid.clone(),
                title: t.description.clone(),
                project: project.clone(),
                rule,
                message,
            });
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

    #[test]
    fn a_new_task_is_held_to_the_same_rules() {
        use crate::act::NewTask;
        let mut cfg = Config::default();
        cfg.lint.projects = vec!["plos".into()];
        let good = NewTask {
            title: "Wire the  victory screen".into(),
            project: Some("plos.db".into()),
            priority: Some('M'),
            tags: vec!["bug".into(), "me".into(), "decision".into()],
            ..Default::default()
        };
        assert!(new_task(&good, &cfg).is_empty());
        let bad = NewTask {
            title: "Wire the victory screen.".into(),
            project: Some("plosive".into()),
            priority: None,
            tags: vec!["urgent".into()],
            ..Default::default()
        };
        let rules: Vec<&str> = new_task(&bad, &cfg).iter().map(|r| r.0).collect();
        assert_eq!(
            rules,
            [
                "title-period",
                "tag-unknown",
                "project-unknown",
                "no-priority"
            ]
        );
        let bare = NewTask {
            title: "Loose end".into(),
            priority: Some('L'),
            ..Default::default()
        };
        let rules: Vec<&str> = new_task(&bare, &cfg).iter().map(|r| r.0).collect();
        assert_eq!(rules, ["no-project"]);
    }
}
