//! A task as `task export` writes it. Only the fields Waybill reads.

use crate::clock::parse_stamp;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Completed,
    Deleted,
    Recurring,
    /// Taskwarrior 2 exported hidden tasks this way; 3 keeps them pending
    /// with a `wait` date.
    Waiting,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Annotation {
    #[serde(deserialize_with = "stamp")]
    pub entry: i64,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Task {
    pub uuid: String,
    #[serde(default)]
    pub description: String,
    pub status: Status,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, deserialize_with = "uuids")]
    pub depends: Vec<String>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    #[serde(deserialize_with = "stamp")]
    pub entry: i64,
    #[serde(default, deserialize_with = "opt_stamp")]
    pub start: Option<i64>,
    #[serde(default, deserialize_with = "opt_stamp")]
    pub end: Option<i64>,
    #[serde(default, deserialize_with = "opt_stamp")]
    pub modified: Option<i64>,
    #[serde(default, deserialize_with = "opt_stamp")]
    pub wait: Option<i64>,
    #[serde(default, deserialize_with = "opt_stamp")]
    pub due: Option<i64>,
    #[serde(default)]
    pub urgency: f64,
    /// The `ref` field: the inline marker in code or docs that points here.
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
}

impl Task {
    /// Still to do: pending, whether or not it is hidden until later.
    pub fn open(&self) -> bool {
        matches!(self.status, Status::Pending | Status::Waiting)
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

fn stamp<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let s = String::deserialize(d)?;
    parse_stamp(&s).ok_or_else(|| serde::de::Error::custom(format!("not a timestamp: {s}")))
}

fn opt_stamp<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.and_then(|s| parse_stamp(&s)))
}

/// `depends` is a list of uuids today and was one comma-separated string in
/// older exports.
fn uuids<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        List(Vec<String>),
        Joined(String),
    }
    Ok(match Either::deserialize(d)? {
        Either::List(v) => v,
        Either::Joined(s) => s
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
    })
}

/// Parses the JSON array `task export` prints.
pub fn parse(json: &str) -> anyhow::Result<Vec<Task>> {
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_export() {
        let tasks = parse(
            r#"[
              {"id":3,"description":"Wire the victory screen","entry":"20261118T151505Z",
               "modified":"20261118T152000Z","project":"storefront","priority":"H",
               "status":"pending","uuid":"aaaaaaaa-0000-4000-8000-000000000001",
               "tags":["bug"],"depends":["aaaaaaaa-0000-4000-8000-000000000002"],
               "annotations":[{"entry":"20261118T152000Z","description":"split the loader (a1b2c3d)"}],
               "ref":"13","urgency":9.1,"start":"20261118T151600Z"},
              {"id":0,"description":"Old","entry":"20240101T000000Z","end":"20240102T000000Z",
               "status":"completed","uuid":"aaaaaaaa-0000-4000-8000-000000000002",
               "depends":"x, y","some_uda":"kept out"},
              {"description":"Odd","entry":"20240101T000000Z","status":"template","uuid":"u3"}
            ]"#,
        )
        .unwrap();
        assert_eq!(tasks.len(), 3);
        let t = &tasks[0];
        assert!(t.open() && t.has_tag("bug"));
        assert_eq!(t.reference.as_deref(), Some("13"));
        assert_eq!(t.annotations[0].description, "split the loader (a1b2c3d)");
        assert_eq!(t.start, Some(t.entry + 55));
        assert_eq!(tasks[1].depends, ["x", "y"]);
        assert_eq!(tasks[1].status, Status::Completed);
        assert_eq!(tasks[2].status, Status::Unknown);
    }
}
