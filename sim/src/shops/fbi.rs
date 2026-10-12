//! The F.B.I. office in New York (`fb.prg`): the Mr. Big briefing and the end of the story.
//!
//! The brain tape itself is delivered like any cargo when you walk in with your car
//! ([`crate::game::Game::deliver_cargo`] reports `won`).
use crate::game::Game;

pub const NEW_YORK: u8 = 7;
pub const BRIEFING_PRESTIGE: u8 = 95;

/// The briefing repeats on every visit until the tape has been handed over (`fb $A06E`).
pub fn briefing_due(game: &Game) -> bool {
    let d = &game.driver;
    d.city == NEW_YORK && d.prestige >= BRIEFING_PRESTIGE && !d.quest.taken[crate::story::BRAIN_TAPE as usize]
}

/// The briefing, one message-area page per entry.
pub const BRIEFING: [&str; 8] = [
    "We have an important assignment for\nyou.",
    "Mr. Big has gotten one of his men\ninto Gold Cross and wants to set up\na bootleg brain tape operation.",
    "If he succeeds, he'll own the whole\nEast Coast before long.",
    "But if we act now, we can foil him,\nand get the evidence to put him\nbehind bars.",
    "Your mission, should you decide to\naccept it, is to go to the Outlaw HQ,\nwhich we know is in Watertown, and",
    "pose as their courier. Take the brain\ntape they will give you, and bring it\nhere. It is vital evidence.",
    "The password they will want is\n\"Rumplestiltskin\".",
    "Good luck!",
];

/// Mark the briefing as heard (the Outlaw HQ does not require it).
pub fn brief(game: &mut Game) {
    game.fbi_briefed = true;
}

/// What the agents say when the tape arrives.
pub const WIN: [&str; 3] = [
    "This is the break we've been waiting\nfor!",
    "With this evidence, we can finally\nput Mr. Big behind bars for a very\nlong time.",
    "Read tomorrow's paper to see what\nhappens...",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn briefing_conditions() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        assert!(!briefing_due(&g));
        g.driver.prestige = 95;
        assert!(briefing_due(&g));
        g.driver.city = 0;
        assert!(!briefing_due(&g));
        g.driver.city = 7;
        g.driver.quest.taken[4] = true;
        assert!(!briefing_due(&g));
        // every page fits the three-line, 38-column message area as written
        let fits = |s: &str| s.lines().count() <= 3 && s.lines().all(|l| l.chars().count() <= 38);
        assert!(BRIEFING.iter().chain(WIN.iter()).all(|p| fits(p)));
    }
}
