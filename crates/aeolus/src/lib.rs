#![doc = include_str!("../README.md")]
pub mod gate;
pub mod grid;
pub mod hash;
pub mod offer;
pub mod palette;
pub mod path;
pub mod rng;
pub mod world;

pub use gate::Gate;
pub use grid::{Cell, Dir, Grid};
pub use hash::StableHasher;
pub use offer::offers;
pub use path::next_step;
pub use rng::Pcg32;
pub use world::{Bump, Cost, Event, Intent, Kind, Rules, Unit, UnitId, World};
