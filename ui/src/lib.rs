//! cardul-ui: Cardul's screens and software renderer, independent of any engine.
//!
//! A front end (the Bevy shell in `game/`, or the headless tool in `examples/`) creates an
//! [`App`], calls [`App::tick`] 60 times per second with the input, and shows [`App::draw`]'s
//! 320 x 200 frame (scaled).

pub mod fb;
pub mod assets;
pub mod text;
pub mod input;
pub mod term;
pub mod app;
pub mod screens;
pub mod install;

pub use app::{App, Cx, Screen, Trans};
pub use input::{Held, Input, Key};
