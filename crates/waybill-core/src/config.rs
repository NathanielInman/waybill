//! `~/.config/waybill/config.toml`. Every key is optional.

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub task: TaskCfg,
    pub notes: NotesCfg,
    pub tags: TagsCfg,
    pub theme: ThemeCfg,
    pub lint: LintCfg,
    pub bar: BarCfg,
}

/// The house rules `waybill lint` checks open tasks against.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct LintCfg {
    /// The longest a card title may be, in characters.
    pub title: usize,
    /// Tags allowed besides the hands and decision tags.
    pub tags: Vec<String>,
    /// Known projects. Empty means any project is fine. A dotted project
    /// passes when its family is listed.
    pub projects: Vec<String>,
}

impl Default for LintCfg {
    fn default() -> Self {
        LintCfg {
            title: 50,
            tags: vec!["research".into(), "bug".into(), "verify".into()],
            projects: vec![],
        }
    }
}

/// The waybar module.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct BarCfg {
    /// What the module shows. `{lamp}` is a dot, `{you}` the tasks waiting
    /// on you, `{holding}` those that hold others up, `{ready}`, `{doing}`
    /// and `{done}` the lane counts.
    pub format: String,
    /// The same, when nothing of yours holds anything up.
    pub format_calm: String,
}

impl Default for BarCfg {
    fn default() -> Self {
        BarCfg {
            format: "● {holding}".into(),
            format_calm: "●".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct TaskCfg {
    /// The Taskwarrior command. May carry arguments (`task rc:~/other.rc`).
    pub command: String,
}

impl Default for TaskCfg {
    fn default() -> Self {
        TaskCfg {
            command: "task".into(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct NotesCfg {
    /// Folder of `<uuid>.md` notes. Empty means `notes/` under Taskwarrior's
    /// data location.
    pub dir: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct TagsCfg {
    /// Tags that mean "this needs a person's hands". An agent leaves these.
    pub hands: Vec<String>,
    /// The tag for a task blocked on a choice.
    pub decision: String,
}

impl Default for TagsCfg {
    fn default() -> Self {
        TagsCfg {
            hands: vec!["me".into()],
            decision: "decision".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeCfg {
    /// "waybar" borrows the bar's colors and font; "none" keeps the built-in
    /// theme.
    pub from: String,
    pub font: Option<String>,
    pub accent: Option<String>,
}

impl Default for ThemeCfg {
    fn default() -> Self {
        ThemeCfg {
            from: "waybar".into(),
            font: None,
            accent: None,
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// The config and, when the file would not parse, why. A broken file
    /// falls back to the defaults so the board still opens.
    pub fn load() -> (Config, Option<String>) {
        match std::fs::read_to_string(crate::paths::config_file()) {
            Ok(text) => match Config::parse(&text) {
                Ok(c) => (c, None),
                Err(e) => (Config::default(), Some(e)),
            },
            Err(_) => (Config::default(), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_overrides() {
        let c = Config::parse("").unwrap();
        assert_eq!(c.task.command, "task");
        assert_eq!(c.tags.hands, ["me"]);
        let c = Config::parse("[tags]\nhands = [\"nate\"]\n[theme]\nfrom = \"none\"").unwrap();
        assert_eq!(c.tags.hands, ["nate"]);
        assert_eq!(c.tags.decision, "decision");
        assert_eq!(c.theme.from, "none");
        assert!(Config::parse("[tags]\nhand = 1").is_err());
    }
}
