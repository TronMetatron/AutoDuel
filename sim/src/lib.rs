//! cardul-sim: the rules of Cardul (a remake of Origin's Autoduel), free of any engine.
//!
//! Everything here is plain data + functions so it can be unit-tested and reused by any
//! front end. Numbers come from the decoded C64 game (see `docs/CAR_DUEL_SPEC.md`).
//! Original bugs that are repaired are marked `FIX:` in the code.

pub mod rng;
pub mod data;
pub mod tables;
pub mod date;
pub mod driver;
pub mod car;
pub mod cargo;
pub mod world;
pub mod story;
pub mod game;
pub mod shops;
pub mod casino;
pub mod combat;

pub use car::Car;
pub use data::GameData;
pub use date::Date;
pub use driver::Driver;
pub use game::Game;
pub use rng::Rng;
pub use world::Building;
pub use combat::Sfx;
