//! Gold Cross Health Care (`go.prg`, Boston and New York): clones, braintapes, treatment and
//! the clone-heart quest.
//!
//! | key | service | price |
//! |---|---|---|
//! | 1 | create a clone | $5,000 |
//! | 2 | update its braintape (the clone still wakes where it was first made) | $3,000 |
//! | 3 | treatment: (3 - health) weeks of 7 days, health back to 3 | $100 a week |
//! | 4 | leave | |
//!
//! A clone or a braintape update uses up the day. While you carry Mr. Big's brain tape Gold
//! Cross won't make you a clone.
use crate::game::Game;

pub const CLONE_PRICE: u32 = 5000;
pub const UPDATE_PRICE: u32 = 3000;
pub const TREATMENT_PER_WEEK: u32 = 100;
pub const DAYS_PER_WEEK: u32 = 7;

/// The clone-heart quest (quest index 1 in [`crate::story::QUESTS`]).
pub const HEART_QUEST: usize = 1;
const HEART_CITY: u8 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneError {
    /// "I'm sorry, but I've been told not to make you a clone."
    Refused,
    /// "You already have a clone! Would you like to update its braintape?"
    AlreadyHave,
    /// "I'm sorry, but our records don't show that you have a clone to update."
    NoClone,
    /// "I hate to be the one to bring this to your attention, but you don't have enough money."
    Money,
}

/// 1 - "Create a clone $5,000" (`go $A541`). The clone will wake in this city.
pub fn create_clone(game: &mut Game) -> Result<(), CloneError> {
    if game.driver.quest.carrying_tape() {
        return Err(CloneError::Refused);
    }
    if game.driver.clone.is_some() {
        return Err(CloneError::AlreadyHave);
    }
    if !game.driver.pay(CLONE_PRICE) {
        return Err(CloneError::Money);
    }
    let city = game.driver.city;
    game.driver.clone = Some(game.driver.snapshot(city));
    game.driver.day_used = true;
    Ok(())
}

/// 2 - "Update a braintape $3,000" (`go $A584`): today's name and skills, but the clone keeps
/// waking in the city where it was first created.
pub fn update_braintape(game: &mut Game) -> Result<(), CloneError> {
    let Some(city) = game.driver.clone.as_ref().map(|t| t.city) else {
        return Err(CloneError::NoClone);
    };
    if !game.driver.pay(UPDATE_PRICE) {
        return Err(CloneError::Money);
    }
    game.driver.clone = Some(game.driver.snapshot(city));
    game.driver.day_used = true;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreatmentError {
    /// "Are you kidding me? You're as fit as a fiddle!"
    Fit,
    Money,
}

/// A treatment quote: weeks and price.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Treatment {
    pub weeks: u32,
    pub price: u32,
}

/// 3 - "Undergo treatment": (3 - health) weeks at $100 a week (`go $A65B`).
pub fn treatment_quote(game: &Game) -> Result<Treatment, TreatmentError> {
    let health = game.driver.health.min(3);
    if health >= 3 {
        return Err(TreatmentError::Fit);
    }
    let weeks = 3 - health as u32;
    let t = Treatment { weeks, price: weeks * TREATMENT_PER_WEEK };
    if !game.driver.can_afford(t.price) {
        return Err(TreatmentError::Money);
    }
    Ok(t)
}

/// Take the treatment: pay, 7 days per week pass, health is 3 again.
pub fn treatment(game: &mut Game) -> Result<Treatment, TreatmentError> {
    let t = treatment_quote(game)?;
    game.driver.pay(t.price);
    game.pass_days(t.weeks * DAYS_PER_WEEK);
    game.driver.health = 3;
    Ok(t)
}

/// Result of handing a quest item to the player after the right password.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandOver {
    /// "...but you don't have a car. Come back later."
    NoCar,
    /// "...but you don't have room for it/them. Come back later."
    NoRoom,
    /// "...someone will load it immediately."
    Loaded,
}

/// Load quest item `q` into the car you have with you, if there is a free cargo slot.
/// Shared by the quest givers (Gold Cross, Salvage Yard, ...).
pub fn hand_over_quest_item(game: &mut Game, q: usize) -> HandOver {
    match game.car_here() {
        None => HandOver::NoCar,
        Some(c) if c.free_cargo_slot().is_none() => HandOver::NoRoom,
        Some(_) => {
            if game.give_quest_item(q) { HandOver::Loaded } else { HandOver::NoRoom }
        }
    }
}

/// Does the New York Gold Cross ask for the password (`go $A1E9`)? Prestige 40+, the heart
/// quest not taken yet and no quest item being carried.
pub fn wants_password(game: &Game) -> bool {
    let d = &game.driver;
    d.city == HEART_CITY && d.prestige >= 40 && !d.quest.taken[HEART_QUEST] && d.quest.free()
}

/// The password answer: None = wrong password (nothing happens).
pub fn give_password(game: &mut Game, typed: &str) -> Option<HandOver> {
    if !crate::story::password_matches(typed, HEART_QUEST) {
        return None;
    }
    Some(hand_over_quest_item(game, HEART_QUEST))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::Car;
    use crate::game::DeathOutcome;
    use crate::story::BRAIN_TAPE;

    fn game() -> Option<Game> {
        let d = crate::data::test_data()?;
        Some(Game::new("Test", false, &d, 9))
    }

    #[test]
    fn clone_and_update_keep_the_first_city() {
        let Some(mut g) = game() else { return };
        g.driver.money = 4999;
        g.driver.city = 5;
        assert_eq!(create_clone(&mut g), Err(CloneError::Money));
        assert_eq!(update_braintape(&mut g), Err(CloneError::NoClone));
        g.driver.money = 10_000;
        assert_eq!(create_clone(&mut g), Ok(()));
        assert!(g.driver.day_used);
        assert_eq!(g.driver.money, 5000);
        assert_eq!(create_clone(&mut g), Err(CloneError::AlreadyHave));
        // later, in New York, with better skills
        g.driver.city = 7;
        g.driver.driving = 40;
        assert_eq!(update_braintape(&mut g), Ok(()));
        assert_eq!(g.driver.money, 2000);
        let t = g.driver.clone.as_ref().unwrap();
        assert_eq!((t.city, t.driving), (5, 40));
    }

    #[test]
    fn clone_revival_after_update() {
        let Some(mut g) = game() else { return };
        g.driver.money = 9000;
        g.driver.city = 5;
        g.driver.mechanic = 10;
        create_clone(&mut g).unwrap();
        g.driver.city = 7;
        g.driver.mechanic = 30;
        update_braintape(&mut g).unwrap();
        // skills learnt after the update are lost; money and prestige carry over (prestige -4)
        g.driver.mechanic = 50;
        g.driver.prestige = 30;
        g.driver.money = 1234;
        g.add_car(Car::default_design("Doomed", 7));
        assert_eq!(g.kill_driver(), DeathOutcome::Cloned { city: 5 });
        let d = &g.driver;
        assert_eq!((d.mechanic, d.prestige, d.money, d.city, d.health), (30, 26, 1234, 5, 3));
        assert!(d.clone.is_none());
        assert!(g.car().is_none(), "the car he died in is gone");
        // a new clone can be bought again
        g.driver.money = 5000;
        assert_eq!(create_clone(&mut g), Ok(()));
    }

    #[test]
    fn no_clone_while_carrying_the_brain_tape() {
        let Some(mut g) = game() else { return };
        g.driver.money = 10_000;
        g.driver.quest.carrying = Some(BRAIN_TAPE);
        assert_eq!(create_clone(&mut g), Err(CloneError::Refused));
        g.driver.quest.carrying = None;
        assert_eq!(create_clone(&mut g), Ok(()));
    }

    #[test]
    fn treatment_weeks_and_days() {
        let Some(mut g) = game() else { return };
        assert_eq!(treatment_quote(&g), Err(TreatmentError::Fit));
        g.driver.health = 1;
        g.driver.money = 150;
        assert_eq!(treatment_quote(&g), Err(TreatmentError::Money));
        g.driver.money = 250;
        let day = g.driver.date;
        assert_eq!(treatment(&mut g), Ok(Treatment { weeks: 2, price: 200 }));
        assert_eq!((g.driver.health, g.driver.money), (3, 50));
        assert_eq!(g.driver.date, day.plus_days(14));
        assert!(!g.driver.day_used, "treatment doesn't close the shops (14 days passed anyway)");
    }

    #[test]
    fn clone_heart_password() {
        let Some(mut g) = game() else { return };
        g.driver.city = 7;
        assert!(!wants_password(&g));
        g.driver.prestige = 40;
        assert!(wants_password(&g));
        assert_eq!(give_password(&mut g, "great white shark"), None);
        assert_eq!(give_password(&mut g, "great  white whale"), Some(HandOver::NoCar));
        assert!(!g.driver.quest.taken[HEART_QUEST]);
        g.add_car(Car::default_design("X", 7));
        assert_eq!(give_password(&mut g, "Great White Whale"), Some(HandOver::Loaded));
        assert!(g.driver.quest.taken[HEART_QUEST]);
        assert_eq!(g.driver.quest.carrying, Some(HEART_QUEST as u8));
        assert!(!wants_password(&g));
        let item = g.car().unwrap().cargo[0].as_ref().unwrap();
        assert_eq!(item.name, "Cloned heart for Senator.");
        assert_eq!(item.due, Some(g.driver.date.plus_days(5)));
    }
}
