//! Waybill's model. The tasks Taskwarrior exports become lanes, dependency
//! lines, a log of visits and the numbers behind the charts: plain data in,
//! plain data out, no UI. Everything that changes the task list is in `act`
//! and goes through the `task` binary; `agent` builds the pick-up protocol
//! on top of it, and `claim` records which session holds a started task.

pub mod act;
pub mod agent;
pub mod claim;
pub mod clock;
pub mod config;
pub mod demo;
pub mod find;
pub mod lamp;
pub mod lines;
pub mod lint;
pub mod load;
pub mod model;
pub mod note;
pub mod paths;
pub mod pulse;
pub mod task;
pub mod waybar;
