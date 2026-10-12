//! The Pet Store in Philadelphia (`pe.prg`): only a destination for courier cargo.
use super::talk::Notice;
use crate::app::{Cx, Screen};
use cardul_sim::world::Building;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Notice::new(cx, Building::PetStore, 8, &["The Pet Store"]))
}
