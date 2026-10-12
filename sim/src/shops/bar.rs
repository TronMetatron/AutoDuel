//! Joe's Bar (`di.prg`): drinks, rumors and the black market for courier cargo.
//!
//! Black market (`di $A33A`, `$A4DD`): Joe offers 25% of the declared value (10% if overdue).
//! Every sale costs prestige −5; selling the active quest item costs −20 and loses that quest.
//! One sale in sixteen (if you have at least $10) is a sting: an undercover agent takes the goods
//! and fines you 10% of your cash.
use crate::cargo::{Cargo, CargoKind};
use crate::date::Date;
use crate::game::Game;

pub const DRINK_PRICE: u32 = 5;
pub const MAX_DRINKS: u8 = 2;
pub const SALE_PRESTIGE: i32 = 5;
pub const QUEST_SALE_PRESTIGE: i32 = 20;

/// Per-visit state: drinks bought, whether a drink was asked for (rumors need one), whether
/// the rumor was told.
#[derive(Clone, Copy, Debug, Default)]
pub struct Visit {
    pub drinks: u8,
    pub drank: bool,
    pub rumor_told: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drink {
    /// "If I give you another drink, you'll be in no shape to drive. Sorry."
    TooMany,
    /// "Ya bum! You can't pay for it."
    Broke,
    /// "Here ya go.  Enjoy it."
    Served,
}

pub fn buy_drink(game: &mut Game, visit: &mut Visit) -> Drink {
    // the original lets you ask for rumors after asking for a drink, even a refused one
    visit.drank = true;
    if visit.drinks >= MAX_DRINKS {
        return Drink::TooMany;
    }
    if !game.driver.pay(DRINK_PRICE) {
        return Drink::Broke;
    }
    visit.drinks += 1;
    Drink::Served
}

/// What Joe answers to "Listen for rumors".
/// FIX: in the original, asking before buying a drink ("There are no rumors today.") used up the
/// visit's rumor; now only a real answer does.
pub fn rumor(game: &mut Game, visit: &mut Visit) -> &'static str {
    if visit.rumor_told {
        return super::rumors::TOLD_ALREADY;
    }
    if !visit.drank {
        return super::rumors::BAR_RUMORS[5];
    }
    visit.rumor_told = true;
    super::rumors::bar(game)
}

/// Joe's offer for a cargo: 25% of its value, 10% if it is overdue.
pub fn offer(cargo: &Cargo, today: Date) -> u32 {
    let pct = if cargo.is_overdue(today) { 10 } else { 25 };
    cargo.value / 100 * pct
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sale {
    /// "Great! I'll have someone take care of it."
    Sold { paid: u32 },
    /// "...An undercover agent steps up, takes the goods, and fines you $X."
    Stung { fine: u32 },
}

/// Sell cargo slot `slot` of the car you have with you. None if the slot is empty.
pub fn sell(game: &mut Game, slot: usize) -> Option<Sale> {
    let today = game.driver.date;
    let cargo = game.car_here()?.cargo.get(slot)?.clone()?;
    let price = offer(&cargo, today);
    let quest_item = matches!(cargo.kind, CargoKind::Quest(q) if game.driver.quest.carrying == Some(q));
    game.remove_cargo(slot);
    let sale = if game.rng.upto(15) == 0 && game.driver.money >= 10 {
        let fine = game.driver.money / 10;
        game.driver.money -= fine;
        Sale::Stung { fine }
    } else {
        game.driver.add_money(price as i64);
        Sale::Sold { paid: price }
    };
    game.driver.add_prestige(-if quest_item { QUEST_SALE_PRESTIGE } else { SALE_PRESTIGE });
    Some(sale)
}

/// The brain-tape welcome (`di $A0CF`): always hit for 1-2 points and thrown out.
/// Returns (damage, died), or None if you are not carrying the tape.
pub fn tape_ambush(game: &mut Game) -> Option<(u32, bool)> {
    if !game.driver.quest.carrying_tape() {
        return None;
    }
    let dmg = 1 + (game.rng.byte() & 1) as u32;
    Some((dmg, game.driver.hurt(dmg)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::Car;
    use crate::world::Building;

    fn game() -> Option<Game> {
        let d = crate::data::test_data()?;
        Some(Game::new("T", false, &d, 1))
    }

    fn job(value: u32, due: Date) -> Cargo {
        Cargo {
            kind: CargoKind::Job(3),
            name: "Rare stamps".into(),
            dest: Some((Building::Casino, 12)),
            weight: 20,
            spaces: 2,
            due: Some(due),
            value,
            pay: 5000,
        }
    }

    #[test]
    fn black_market_price() {
        let today = Date::start();
        assert_eq!(offer(&job(10_000, today), today), 2500);
        assert_eq!(offer(&job(10_000, today), today.plus_days(1)), 1000);
        assert_eq!(offer(&job(0, today), today), 0);
        assert_eq!(offer(&job(250, today), today), 50);
    }

    #[test]
    fn drinks_and_rumors() {
        let Some(mut g) = game() else { return };
        let mut v = Visit::default();
        assert_eq!(rumor(&mut g, &mut v), "There are no rumors today.");
        assert!(!v.rumor_told);
        assert_eq!(buy_drink(&mut g, &mut v), Drink::Served);
        assert_eq!(buy_drink(&mut g, &mut v), Drink::Served);
        assert_eq!(buy_drink(&mut g, &mut v), Drink::TooMany);
        assert_eq!(g.driver.money, 1990);
        let r = rumor(&mut g, &mut v);
        assert!(super::super::rumors::BAR_RUMORS.contains(&r));
        assert_eq!(rumor(&mut g, &mut v), super::super::rumors::TOLD_ALREADY);
        let mut v = Visit::default();
        g.driver.money = 4;
        assert_eq!(buy_drink(&mut g, &mut v), Drink::Broke);
        assert!(v.drank);
    }

    #[test]
    fn selling_costs_prestige_and_may_be_a_sting() {
        let Some(mut g) = game() else { return };
        g.add_car(Car::default_design("C", 7));
        assert_eq!(sell(&mut g, 0), None);
        let (mut sold, mut stung) = (0, 0);
        for _ in 0..400 {
            g.driver.money = 1000;
            g.driver.prestige = 50;
            g.car_mut().unwrap().cargo[0] = Some(job(10_000, g.driver.date));
            match sell(&mut g, 0).unwrap() {
                Sale::Sold { paid } => {
                    sold += 1;
                    assert_eq!((paid, g.driver.money), (2500, 3500));
                }
                Sale::Stung { fine } => {
                    stung += 1;
                    assert_eq!((fine, g.driver.money), (100, 900));
                }
            }
            assert_eq!(g.driver.prestige, 45);
            assert!(g.car().unwrap().cargo[0].is_none());
        }
        assert!(stung > 5 && stung < 50, "{} {}", sold, stung);
        // no sting below $10
        for _ in 0..100 {
            g.driver.money = 9;
            g.car_mut().unwrap().cargo[0] = Some(job(100, g.driver.date));
            assert_eq!(sell(&mut g, 0), Some(Sale::Sold { paid: 25 }));
        }
        // the quest item: -20 and the quest is lost
        g.driver.prestige = 50;
        g.give_quest_item(3);
        let slot = g.car().unwrap().cargo.iter().position(|c| c.is_some()).unwrap();
        sell(&mut g, slot);
        assert_eq!(g.driver.prestige, 30);
        assert_eq!(g.driver.quest.carrying, None);
        assert!(g.driver.quest.taken[3]);
    }

    #[test]
    fn tape_welcome() {
        let Some(mut g) = game() else { return };
        assert_eq!(tape_ambush(&mut g), None);
        g.add_car(Car::default_design("C", 0));
        g.give_quest_item(4);
        for _ in 0..50 {
            g.driver.health = 3;
            let (dmg, died) = tape_ambush(&mut g).unwrap();
            assert!((1..=2).contains(&dmg) && !died);
            assert_eq!(g.driver.health as u32, 3 - dmg);
        }
    }
}
