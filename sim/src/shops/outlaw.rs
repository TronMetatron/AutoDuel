//! The Outlaw Headquarters in Watertown (`ou.prg`), where the brain tape is picked up.
//!
//! Below 95 prestige (or after the game is won) it looks like any abandoned building.
//! FIX: in the original, losing the tape (car destroyed, sold or left behind) made the game
//! unwinnable: the outlaws never gave out another one. Now they hand out a new tape to anyone
//! who comes back without one and knows the password.
use crate::game::Game;
use crate::story::BRAIN_TAPE;

pub const PRESTIGE: u8 = 95;

/// "Outlaw Headquarters" or "An Abandoned Building".
pub fn title(game: &Game) -> &'static str {
    if game.driver.prestige >= PRESTIGE && !game.driver.quest.won {
        "Outlaw Headquarters"
    } else {
        "An Abandoned Building"
    }
}

/// Walking back in with the tape gets you killed (`ou $A0F6`).
pub fn double_cross(game: &Game) -> bool {
    game.driver.quest.carrying_tape()
}

/// Do they ask for the password? Prestige 95+, not carrying another quest item, and the tape
/// not handed out yet — or lost since (the FIX above).
pub fn password_due(game: &Game) -> bool {
    let q = &game.driver.quest;
    game.driver.prestige >= PRESTIGE && !q.won && q.free() && !q.done[BRAIN_TAPE as usize]
}

pub const DOUBLE_CROSS: [&str; 2] = [
    "You double-crossing cop-lover! We'll\nteach you to work with the FBI!",
    "You scream as the gang grabs you and\ncarries you back to the torture\nroom, never to be seen again...",
];
/// The start of their sentence; [`super::rumors::Handover::text`] finishes it.
pub const GIVE: &str = "With this tape in Pittsburgh, we'll\nhave it made,";
pub const NO_CLONE: &str = "And just to make sure your heart is\nin this, you won't have a clone to\nfall back on.";
pub const ESCAPE: &str = "As you leave, you hear someone yell\n\"Get that guy! He's with the FBI!\".\nYou barely make it out the door.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::Car;
    use crate::shops::rumors::{hand_over, Handover};

    #[test]
    fn tape_can_be_reissued_after_loss() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        g.driver.city = 0;
        assert_eq!(title(&g), "An Abandoned Building");
        assert!(!password_due(&g));
        g.driver.prestige = 95;
        assert_eq!(title(&g), "Outlaw Headquarters");
        assert!(password_due(&g));
        g.driver.clone = Some(g.driver.snapshot(7));
        g.add_car(Car::default_design("C", 0));
        assert_eq!(hand_over(&mut g, 4), Handover::Loaded);
        assert!(g.driver.clone.is_none());
        assert!(double_cross(&g));
        assert!(!password_due(&g));
        // the car is lost: the tape goes with it
        g.lose_current_car();
        assert!(!double_cross(&g));
        assert!(password_due(&g), "FIX: a new tape is offered");
        g.add_car(Car::default_design("D", 0));
        assert_eq!(hand_over(&mut g, 4), Handover::Loaded);
        assert!(double_cross(&g));
        g.driver.quest.won = true;
        assert!(!double_cross(&g));
        assert_eq!(title(&g), "An Abandoned Building");
    }
}
