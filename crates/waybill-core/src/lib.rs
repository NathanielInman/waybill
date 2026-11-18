//! Waybill's model. Everything here is plain data in and plain data out: the
//! tasks Taskwarrior exports become lanes, dependency lines, a log of visits
//! and the numbers behind the charts. No UI, and nothing here writes to the
//! task database.

pub mod clock;
pub mod config;
pub mod demo;
pub mod lines;
pub mod load;
pub mod model;
pub mod note;
pub mod paths;
pub mod pulse;
pub mod task;
pub mod waybar;
