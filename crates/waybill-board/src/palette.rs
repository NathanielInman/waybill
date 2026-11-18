//! The `/` line: a few plain words instead of a key to remember. With a
//! card open the words act on it ("high", "done shipped it", "wait
//! friday"); otherwise they find a card or change the view.

use crate::actions::Action;
use crate::views::Tab;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Do(Action),
    View(Tab),
    /// Open the new-task form with this title.
    New(String),
}

/// What `text` means for the open card `uuid`, with the words to show for
/// it. `None` when it is not a command (it is then a search).
pub fn parse(text: &str, uuid: Option<&str>) -> Option<(Command, String)> {
    let text = text.trim();
    let lower = text.to_lowercase();
    let (word, rest) = match lower.split_once(char::is_whitespace) {
        Some((w, _)) => (w, text[w.len()..].trim()),
        None => (lower.as_str(), ""),
    };
    if let Some(tab) = Tab::parse(&lower) {
        return Some((Command::View(tab), format!("Go to {}", tab.name())));
    }
    if matches!(word, "new" | "add") && !rest.is_empty() {
        return Some((Command::New(rest.to_string()), format!("New task: {rest}")));
    }
    let uuid = uuid?.to_string();
    let act = |a: Action, say: &str| Some((Command::Do(a), say.to_string()));
    match lower.as_str() {
        "start" => return act(Action::Start(uuid), "Start it"),
        "stop" => return act(Action::Stop(uuid), "Stop it"),
        "h" | "high" | "high priority" => {
            return act(Action::Priority(uuid, 'H'), "Set priority high");
        }
        "m" | "medium" | "medium priority" | "normal" => {
            return act(Action::Priority(uuid, 'M'), "Set priority medium");
        }
        "l" | "low" | "low priority" | "someday maybe" => {
            return act(Action::Priority(uuid, 'L'), "Set priority low");
        }
        "mine" | "take" | "take it" | "needs me" => {
            return act(Action::Hands(uuid, true), "Put it on your desk");
        }
        "agent" | "agents" | "hand to agent" | "hand to agents" | "hand off" => {
            return act(Action::Hands(uuid, false), "Hand it to the agents");
        }
        "park" | "someday" => {
            return act(
                Action::Park(uuid, "someday".into()),
                "Park it until someday",
            );
        }
        "unpark" | "show" | "wake" => {
            return act(
                Action::Park(uuid, String::new()),
                "Put it back on the board",
            );
        }
        _ => {}
    }
    match word {
        "done" | "finish" => Some((
            Command::Do(Action::Done(uuid, rest.to_string())),
            if rest.is_empty() {
                "Finish it".to_string()
            } else {
                format!("Finish it: {rest}")
            },
        )),
        "visit" | "log" | "say" if !rest.is_empty() => Some((
            Command::Do(Action::Visit(uuid, rest.to_string())),
            format!("Log a visit: {rest}"),
        )),
        "rule" | "decide" if !rest.is_empty() => Some((
            Command::Do(Action::Rule(uuid, rest.to_string())),
            format!("Rule: {rest}"),
        )),
        "wait" | "park" => {
            // "wait until friday" and "wait friday" both end in the date.
            let date = rest
                .rsplit(char::is_whitespace)
                .next()
                .unwrap_or("")
                .to_lowercase();
            (!date.is_empty() && date != "until").then(|| {
                (
                    Command::Do(Action::Park(uuid, date.clone())),
                    format!("Park it until {date}"),
                )
            })
        }
        _ => None,
    }
}

/// How well `text` matches a card: lower is better, `None` is no match.
/// Every word must appear somewhere in the title, project, tags or uuid.
pub fn score(text: &str, title: &str, project: &str, tags: &[String], uuid: &str) -> Option<usize> {
    let hay_title = title.to_lowercase();
    let mut total = 0;
    for word in text.to_lowercase().split_whitespace() {
        total += if let Some(at) = hay_title.find(word) {
            at
        } else if project.to_lowercase().contains(word)
            || tags.iter().any(|t| t.to_lowercase() == word)
            || uuid.starts_with(word)
        {
            200
        } else {
            return None;
        };
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(text: &str) -> Option<Action> {
        match parse(text, Some("u"))?.0 {
            Command::Do(a) => Some(a),
            _ => None,
        }
    }

    #[test]
    fn plain_words_act_on_the_open_card() {
        assert_eq!(
            act("high priority"),
            Some(Action::Priority("u".into(), 'H'))
        );
        assert_eq!(act("Low"), Some(Action::Priority("u".into(), 'L')));
        assert_eq!(act("hand to agent"), Some(Action::Hands("u".into(), false)));
        assert_eq!(act("mine"), Some(Action::Hands("u".into(), true)));
        assert_eq!(
            act("wait until Friday"),
            Some(Action::Park("u".into(), "friday".into()))
        );
        assert_eq!(
            act("park"),
            Some(Action::Park("u".into(), "someday".into()))
        );
        assert_eq!(
            act("done Shipped in a1b2c3d"),
            Some(Action::Done("u".into(), "Shipped in a1b2c3d".into()))
        );
        assert_eq!(act("done"), Some(Action::Done("u".into(), String::new())));
        assert_eq!(
            act("visit Tried it, feels slow"),
            Some(Action::Visit("u".into(), "Tried it, feels slow".into()))
        );
        assert_eq!(
            act("rule Keep 4 s"),
            Some(Action::Rule("u".into(), "Keep 4 s".into()))
        );
        assert_eq!(act("netplay"), None, "anything else is a search");
        assert_eq!(act("wait until"), None);
    }

    #[test]
    fn some_words_need_no_card() {
        assert_eq!(
            parse("pulse", None).map(|c| c.0),
            Some(Command::View(Tab::Pulse))
        );
        assert_eq!(
            parse("new Fix the typo", None).map(|c| c.0),
            Some(Command::New("Fix the typo".into()))
        );
        assert_eq!(parse("high", None), None, "nothing is open to act on");
    }

    #[test]
    fn search_wants_every_word() {
        let tags = vec!["bug".to_string()];
        let s = |q: &str| {
            score(
                q,
                "Netplay: lobby and discovery",
                "detour-33",
                &tags,
                "c5f00fc0-aaaa",
            )
        };
        assert_eq!(s("netplay"), Some(0));
        assert!(s("lobby") > s("netplay"));
        assert_eq!(s("lobby detour"), Some(209));
        assert_eq!(s("bug"), Some(200));
        assert_eq!(s("c5f0"), Some(200));
        assert_eq!(s("lobby kepler"), None);
    }
}
