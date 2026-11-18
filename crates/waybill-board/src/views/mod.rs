//! The five views. Each owns its widgets, takes a snapshot in `set` and
//! plays its charts in when it comes on screen.

pub mod board;
pub mod desk;
pub mod lines;
pub mod log;
pub mod pulse;

use crate::shell::Msg;

pub type Tx = async_channel::Sender<Msg>;

/// Asks the shell to open a card.
pub fn open(tx: &Tx, uuid: &str) {
    let _ = tx.send_blocking(Msg::Open(uuid.to_string()));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Board,
    Desk,
    Lines,
    Log,
    Pulse,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Board, Tab::Desk, Tab::Lines, Tab::Log, Tab::Pulse];

    pub fn name(self) -> &'static str {
        match self {
            Tab::Board => "Board",
            Tab::Desk => "Desk",
            Tab::Lines => "Lines",
            Tab::Log => "Log",
            Tab::Pulse => "Pulse",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Tab::Board => "board",
            Tab::Desk => "desk",
            Tab::Lines => "lines",
            Tab::Log => "log",
            Tab::Pulse => "pulse",
        }
    }

    /// The one question the view answers.
    pub fn question(self) -> &'static str {
        match self {
            Tab::Board => "Where is everything right now?",
            Tab::Desk => "What is waiting on me, and what does it free?",
            Tab::Lines => "What has to happen in what order?",
            Tab::Log => "What happened while I was away?",
            Tab::Pulse => "How is the work flowing?",
        }
    }

    pub fn parse(s: &str) -> Option<Tab> {
        Tab::ALL.into_iter().find(|t| t.id() == s)
    }

    pub fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}
