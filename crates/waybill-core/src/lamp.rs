//! The one lamp. It is amber only when the user is what the work waits on,
//! and the window's header and the bar module both show this same answer.

use crate::config::BarCfg;
use crate::model::Snapshot;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Calm,
    NeedsYou,
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

pub fn lamp(s: &Snapshot) -> Lamp {
    let p = &s.pulse;
    let (state, why) = if s.error.is_some() {
        (State::Error, "The task list could not be read".to_string())
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
    let format = if lamp.state == State::NeedsYou {
        &cfg.format
    } else {
        &cfg.format_calm
    };
    // A decision counts as something that holds work up.
    let holding = p.holding.max(p.decisions_open);
    let text = format
        .replace("{lamp}", "●")
        .replace("{holding}", &holding.to_string())
        .replace("{you}", &p.lanes.yours.to_string())
        .replace("{ready}", &p.lanes.ready.to_string())
        .replace("{doing}", &p.lanes.doing.to_string())
        .replace("{done}", &p.lanes.done.to_string());
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
}
