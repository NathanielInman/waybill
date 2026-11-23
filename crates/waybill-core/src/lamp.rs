//! The one lamp. It is red while an alarm is open (something broke and
//! should be looked at first), amber only when the user is what the work
//! waits on, and the window's header and the bar module both show this same
//! answer.

use crate::config::BarCfg;
use crate::model::Snapshot;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Calm,
    NeedsYou,
    /// An open task carries the alarm tag.
    Alarm,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lamp {
    pub state: State,
    pub why: String,
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Open tasks with the alarm tag, most urgent first.
pub fn alarms(s: &Snapshot) -> Vec<&crate::model::Card> {
    let mut a: Vec<_> = s
        .cards
        .iter()
        .filter(|c| c.alarm && c.ended.is_none())
        .collect();
    a.sort_by(|x, y| y.urgency.total_cmp(&x.urgency));
    a
}

pub fn lamp(s: &Snapshot) -> Lamp {
    let p = &s.pulse;
    let alarms = alarms(s);
    let (state, why) = if s.error.is_some() {
        (State::Error, "The task list could not be read".to_string())
    } else if !alarms.is_empty() {
        let why = match alarms.as_slice() {
            [one] => format!("Alarm: {}", one.title),
            many => format!(
                "{} alarms: {}",
                many.len(),
                many.iter()
                    .map(|c| c.title.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        };
        (State::Alarm, why)
    } else if p.holding > 0 {
        (
            State::NeedsYou,
            format!(
                "{} of yours {} up {}",
                plural(p.holding, "task", "tasks"),
                if p.holding == 1 { "holds" } else { "hold" },
                plural(p.held_up, "other", "others")
            ),
        )
    } else if p.decisions_open > 0 {
        (
            State::NeedsYou,
            format!(
                "{} {} open",
                plural(p.decisions_open, "decision", "decisions"),
                if p.decisions_open == 1 { "is" } else { "are" }
            ),
        )
    } else {
        (
            State::Calm,
            "Nothing of yours holds anything up".to_string(),
        )
    };
    Lamp { state, why }
}

/// One line for waybar's `return-type: json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BarLine {
    pub text: String,
    pub tooltip: String,
    pub class: &'static str,
}

pub fn bar_line(s: &Snapshot, cfg: &BarCfg) -> BarLine {
    let lamp = lamp(s);
    let p = &s.pulse;
    let format = match lamp.state {
        State::Alarm => &cfg.format_alarm,
        State::NeedsYou => &cfg.format,
        _ => &cfg.format_calm,
    };
    // A decision counts as something that holds work up.
    let holding = p.holding.max(p.decisions_open);
    let text = format
        .replace("{lamp}", "●")
        .replace("{holding}", &holding.to_string())
        .replace("{you}", &p.lanes.yours.to_string())
        .replace("{ready}", &p.lanes.ready.to_string())
        .replace("{doing}", &p.lanes.doing.to_string())
        .replace("{done}", &p.lanes.done.to_string())
        .replace("{alarms}", &alarms(s).len().to_string());
    let mut tooltip = vec![lamp.why.clone()];
    if let Some(e) = &s.error {
        tooltip.push(e.clone());
    } else {
        tooltip.push(format!(
            "{} on you",
            plural(p.lanes.yours, "task waits", "tasks wait")
        ));
        tooltip.push(format!(
            "{} ready · {} doing · {} finished today",
            p.lanes.ready, p.lanes.doing, p.lanes.done
        ));
    }
    BarLine {
        text,
        tooltip: tooltip.join("\n"),
        class: match lamp.state {
            State::Calm => "calm",
            State::NeedsYou => "needs-you",
            State::Alarm => "alarm",
            State::Error => "error",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Zone;
    use crate::model::tests::NOW;

    #[test]
    fn amber_only_when_the_user_holds_work_up() {
        let mut s = crate::demo::snapshot(NOW, Zone::Fixed(0));
        let l = lamp(&s);
        assert_eq!(l.state, State::NeedsYou);
        assert!(l.why.contains("of yours hold up"), "{}", l.why);
        let line = bar_line(&s, &BarCfg::default());
        assert_eq!(line.class, "needs-you");
        assert_eq!(line.text, format!("● {}", s.pulse.holding));
        assert!(line.tooltip.contains("finished today"));

        s.pulse.holding = 0;
        s.pulse.decisions_open = 1;
        assert_eq!(lamp(&s).why, "1 decision is open");
        s.pulse.decisions_open = 0;
        assert_eq!(bar_line(&s, &BarCfg::default()).text, "●");
        assert_eq!(lamp(&s).state, State::Calm);
        s.error = Some("no task binary".into());
        assert_eq!(bar_line(&s, &BarCfg::default()).class, "error");
    }

    #[test]
    fn red_while_an_alarm_is_open() {
        let mut s = crate::demo::snapshot(NOW, Zone::Fixed(0));
        assert!(alarms(&s).is_empty());
        let i = s.cards.iter().position(|c| c.ended.is_none()).unwrap();
        s.cards[i].alarm = true;
        s.cards[i].title = "Fix nightly backup".into();
        // An alarm outranks work the user holds up.
        let l = lamp(&s);
        assert_eq!(l.state, State::Alarm);
        assert_eq!(l.why, "Alarm: Fix nightly backup");
        let line = bar_line(&s, &BarCfg::default());
        assert_eq!(line.class, "alarm");
        assert_eq!(line.text, "● 1");
        assert!(line.tooltip.starts_with("Alarm: Fix nightly backup"));

        let j = s
            .cards
            .iter()
            .rposition(|c| c.ended.is_none() && !c.alarm)
            .unwrap();
        s.cards[j].alarm = true;
        s.cards[j].title = "Fix notes sync".into();
        assert!(lamp(&s).why.starts_with("2 alarms: "), "{}", lamp(&s).why);
        // A finished alarm no longer counts.
        s.cards[i].ended = Some(NOW);
        assert_eq!(lamp(&s).why, "Alarm: Fix notes sync");
        // The list failing to load still wins.
        s.error = Some("no task binary".into());
        assert_eq!(lamp(&s).state, State::Error);
    }
}
