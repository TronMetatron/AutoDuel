//! Origin Systems in Manchester (`or.prg`): a destination for courier cargo, with an
//! advertisement. The original advertised OSI's 1985 products; this is a neutral stand-in.
use super::talk::Notice;
use crate::app::{Cx, Screen};
use cardul_sim::world::Building;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Notice::new(
        cx,
        Building::Origin,
        2,
        &[
            "Origin Systems, Inc.",
            "",
            "",
            "Watch for new games",
            "from OSI:",
            "",
            "Role-playing worlds to explore,",
            "",
            "strategy for armchair generals,",
            "",
            "and duels on the open road.",
            "",
            "",
            "Ask your local software dealer.",
        ],
    ))
}
