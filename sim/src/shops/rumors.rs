//! Rumors, clues and quest hand-overs in the world buildings (docs/world.md §5).
//!
//! Each quest starts with a prestige-gated rumor at a Truck Stop or Joe's Bar and continues
//! through a chain of clues that each give one word of a password. The first matching tier wins,
//! highest prestige first.
//!
//! FIX: the third clue of the mob-evidence chain (Scranton Truck Stop) said "Sells" and sent you
//! back to Dover; it now gives "Shells" and points to Watertown, where the password is wanted.
use crate::game::Game;

pub const RUMOR_FBI: &str = "They want you at the FBI in New York.";

/// Truck Stop: "I've told you what I know already!" (once per visit, and again after a room).
pub const TOLD_ALREADY: &str = "I've told you what I know already!";

/// Filler rumors at the Truck Stop (`tr` messages $16-$18).
pub const TRUCK_STOP_FILLER: [&str; 3] = [
    "I hear they actually put up speed\nlimit signs last week.",
    "Be careful on the less travelled\nroads.",
    "I hear they pay cash for payloads at\nJoe's.",
];

/// Joe's Bar rumors (`di` messages $0F-$14); the last one is "no rumor".
pub const BAR_RUMORS: [&str; 6] = [
    "People have really been happy with\nbody armor.",
    "Try Atlantic City if you're ever low\non cash.",
    "The IRS is really cracking down on\ntax evaders.",
    "Fnord's releasing its Super Limited\nEdition High-Performance 340 ZXY bi-\nturbo Trans-Am GTO Cobra next month.",
    "I hear four-fifths of autoduellists\nwithout clones don't make it past\na month of duelling.",
    "There are no rumors today.",
];

/// City indices used by the clues.
mod city {
    pub const WATERTOWN: u8 = 0;
    pub const ALBANY: u8 = 4;
    pub const SCRANTON: u8 = 6;
    pub const PROVIDENCE: u8 = 8;
    pub const BALTIMORE: u8 = 13;
    pub const DOVER: u8 = 14;
    pub const WASHINGTON: u8 = 15;
}

/// The Truck Stop rumor for the driver's current city (`tr $A62C`).
pub fn truck_stop(game: &mut Game) -> &'static str {
    let d = &game.driver;
    let (p, taken, c) = (d.prestige, d.quest.taken, d.city);
    if p >= 95 && !taken[4] {
        return RUMOR_FBI;
    }
    if p >= 80 && !taken[3] {
        return "Word is a driver's needed at Joe's\nBar in Baltimore.";
    }
    if p >= 60 && !taken[2] {
        return match c {
            city::DOVER => "The Feds need a driver. More info at\nthe Providence Truck Stop. Remember\n\"She\".",
            city::PROVIDENCE => {
                "The Watertown boys have some\nvaluable information. They know more\nin Scranton. Remember \"Sells\"."
            }
            city::SCRANTON => {
                "The D.A. has an airtight case against\nthe Philly mob. Ask in Watertown.\nRemember the word \"Shells\"."
            }
            _ => "Go to the Dover Truck Stop for some\ninfo on a nice courier job.",
        };
    }
    if p >= 40 && !taken[1] {
        return match c {
            city::WASHINGTON => {
                "Senator Phogbound needs a new heart.\nThey know more at the Buffalo AADA.\nRemember the word \"Great\"."
            }
            _ => "They're looking for you at the\nWashington Truck Stop.",
        };
    }
    if p >= 20 && !taken[0] {
        return "They have some information you would\nbe interested in at the weapon shop\nin Scranton.";
    }
    TRUCK_STOP_FILLER[game.rng.upto(2) as usize]
}

/// The Joe's Bar rumor (`di $A487`); the caller checks that a drink was bought.
pub fn bar(game: &mut Game) -> &'static str {
    let d = &game.driver;
    if d.prestige >= 95 && !d.quest.taken[4] {
        return RUMOR_FBI;
    }
    if d.prestige >= 80 && !d.quest.taken[3] && d.city == city::BALTIMORE {
        return "The FBI needs a good driver at the\nWash. Salvage Yard. Go to the Old\nBuilding in Albany. Remember \"Horn\".";
    }
    BAR_RUMORS[game.rng.upto(5) as usize]
}

/// The Albany abandoned building clue (`ab $A060`): prestige ≥ 80, fixed-autoduels quest not
/// taken. When it fires there is no mugging.
pub fn albany_clue(game: &Game) -> Option<&'static str> {
    let d = &game.driver;
    (d.prestige >= 80 && !d.quest.taken[3] && d.city == city::ALBANY)
        .then_some("The mob has been fixing races. They\nknow more at the Buffalo Salvage\nyard. Remember \"Little\".")
}

/// Is the Watertown Truck Stop asking for "SHE SELLS SHELLS" (`tr $A2CC`)?
pub fn mob_password_due(game: &Game) -> bool {
    let d = &game.driver;
    d.city == city::WATERTOWN && d.prestige >= 60 && !d.quest.taken[2] && d.quest.free()
}

/// What a quest giver says after the right password: "..., but you don't have a car. Come back
/// later." / "..., but you don't have room for it. Come back later." / "..., someone will load it
/// immediately."
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handover {
    NoCar,
    NoRoom,
    Loaded,
}

impl Handover {
    /// The end of the giver's sentence.
    pub fn text(self) -> &'static str {
        match self {
            Handover::NoCar => "but you don't have a car. Come back later.",
            Handover::NoRoom => "but you don't have room for it. Come back later.",
            Handover::Loaded => "someone will load it immediately.",
        }
    }
}

/// Hand over quest `q`'s item if you have your car here with a free cargo slot.
pub fn hand_over(game: &mut Game, q: usize) -> Handover {
    match game.car_here() {
        None => Handover::NoCar,
        Some(_) if game.driver.loaner => Handover::NoCar,
        Some(c) if c.free_cargo_slot().is_none() => Handover::NoRoom,
        Some(_) => {
            if game.give_quest_item(q) {
                Handover::Loaded
            } else {
                Handover::NoRoom
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::Car;

    fn game() -> Option<Game> {
        let d = crate::data::test_data()?;
        Some(Game::new("T", false, &d, 1))
    }

    #[test]
    fn truck_stop_tiers() {
        let Some(mut g) = game() else { return };
        g.driver.prestige = 10;
        assert!(TRUCK_STOP_FILLER.contains(&truck_stop(&mut g)));
        g.driver.prestige = 20;
        assert!(truck_stop(&mut g).contains("Scranton"));
        g.driver.quest.taken[0] = true;
        assert!(TRUCK_STOP_FILLER.contains(&truck_stop(&mut g)));
        g.driver.prestige = 40;
        assert!(truck_stop(&mut g).contains("Washington Truck Stop"));
        g.driver.city = 15;
        assert!(truck_stop(&mut g).contains("\"Great\""));
        g.driver.prestige = 60;
        assert!(truck_stop(&mut g).contains("Dover Truck Stop"));
        g.driver.city = 14;
        assert!(truck_stop(&mut g).contains("\"She\""));
        g.driver.city = 8;
        assert!(truck_stop(&mut g).contains("\"Sells\""));
        g.driver.city = 6;
        assert!(truck_stop(&mut g).contains("\"Shells\""));
        g.driver.prestige = 80;
        assert!(truck_stop(&mut g).contains("Baltimore"));
        g.driver.prestige = 95;
        assert_eq!(truck_stop(&mut g), RUMOR_FBI);
        g.driver.quest.taken = [true; 5];
        assert!(TRUCK_STOP_FILLER.contains(&truck_stop(&mut g)));
    }

    #[test]
    fn bar_and_albany() {
        let Some(mut g) = game() else { return };
        assert!(BAR_RUMORS.contains(&bar(&mut g)));
        g.driver.prestige = 80;
        g.driver.city = 13;
        assert!(bar(&mut g).contains("\"Horn\""));
        g.driver.prestige = 95;
        assert_eq!(bar(&mut g), RUMOR_FBI);
        assert!(albany_clue(&g).is_none());
        g.driver.city = 4;
        assert!(albany_clue(&g).unwrap().contains("\"Little\""));
        g.driver.quest.taken[3] = true;
        assert!(albany_clue(&g).is_none());
    }

    #[test]
    fn handover_needs_car_and_room() {
        let Some(mut g) = game() else { return };
        g.driver.city = 0;
        g.driver.prestige = 60;
        assert!(mob_password_due(&g));
        assert_eq!(hand_over(&mut g, 2), Handover::NoCar);
        assert!(!g.driver.quest.taken[2]);
        g.add_car(Car::default_design("C", 0));
        assert_eq!(hand_over(&mut g, 2), Handover::Loaded);
        assert!(g.driver.quest.taken[2]);
        assert_eq!(g.driver.quest.carrying, Some(2));
        assert!(!mob_password_due(&g));
    }
}
