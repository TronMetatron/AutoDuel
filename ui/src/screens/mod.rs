//! All screens.
pub mod setup;
pub mod city;
pub mod info;
pub mod death;
pub mod drive;
pub mod ending;
pub mod buildings;

use crate::app::{Cx, Trans};
use crate::input::{Input, Key};

/// Help text for the global keys.
pub const HELP: &[&str] = &[
    "Cardul - keys",
    "",
    "Arrows/WASD/pad walk, drive, menus",
    "Ctrl, Z, pad A  fire",
    "Space, pad B    next weapon",
    "1..0            select weapon",
    "G               leave/enter car",
    "P               pause (driving)",
    "",
    "F1 help         F2 driver status",
    "F3 car sheet    F4 your cars",
    "F5 road map     F6 save game",
    "F7 sound        F8 cargo",
    "F9 drive speed  F10 graphics",
    "F11 full screen Esc back/cancel",
];

/// Global information keys, usable in the city, in buildings and on the road. Returns a
/// screen to push.
pub fn hotkeys(cx: &mut Cx, inp: &Input, allow_examine: bool) -> Option<Trans> {
    cx.game.as_ref()?;
    for k in &inp.keys {
        match k {
            Key::F(1) => return Some(Trans::Push(Box::new(info::InfoScreen::help()))),
            Key::F(2) => return Some(Trans::Push(Box::new(info::InfoScreen::status(cx)))),
            Key::F(3) => return Some(Trans::Push(Box::new(info::InfoScreen::car_sheet(cx, None)))),
            Key::F(4) if allow_examine => return Some(Trans::Push(Box::new(info::ExamineScreen::new(cx)))),
            Key::F(5) => return Some(Trans::Push(Box::new(info::MapScreen::new()))),
            Key::F(8) => return Some(Trans::Push(Box::new(info::InfoScreen::cargo(cx)))),
            Key::F(6) => {
                cx.save();
                cx.toast("Game saved");
                cx.sfx(cardul_sim::Sfx::Click);
                return Some(Trans::None);
            }
            _ => {}
        }
    }
    None
}
