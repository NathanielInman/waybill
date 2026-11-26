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

/// The views and their order live in core, beside the words that go to
/// them.
pub use waybill_core::say::View as Tab;
