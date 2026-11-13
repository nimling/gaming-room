//! Arcade core. Reads the game manifests and the seat, and turns
//! "play this game" into a launch plan: the settings files to write,
//! the command to run and the steps the screen shows.

pub mod config;
pub mod ini;
pub mod lan;
pub mod manifest;
pub mod music;
pub mod nick;
pub mod plan;
pub mod ptt;
pub mod seat;

pub use config::Config;
pub use manifest::{Card, Manifest, Tier};
pub use plan::{Mode, Plan};
pub use seat::{Display, Seat};
